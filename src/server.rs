// src/server.rs
use crate::common::{HttpHeaders, HttpMethod, HttpStatus};
use crate::http_parser::parse_request_parts;
use crate::http_printer::HttpPrinter;
use crate::router::{AppRouter, DefaultRouter};
use crate::threadpool::ThreadPool;
use std::cmp;
use std::io::{BufReader, Read};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;

pub struct App {}

impl App {
    pub fn with_default_router(
        port: u16,
        thread_count: usize,
    ) -> HttpServer<DefaultRouter<Box<RouteFn>>> {
        HttpServer {
            port,
            thread_count,
            router: DefaultRouter::<Box<RouteFn>>::new(),
        }
    }
}

pub type RouteFn = dyn Fn(HttpRequestContext, Response) + Send + Sync + 'static;

pub struct HttpServer<R>
where
    R: AppRouter<Route = Box<RouteFn>>,
{
    port: u16,
    thread_count: usize,
    router: R,
}

impl<R> HttpServer<R>
where
    R: AppRouter<Route = Box<RouteFn>> + Send + 'static,
{
    pub fn new(port: u16, thread_count: usize) -> HttpServer<R> {
        HttpServer {
            port,
            thread_count,
            router: R::new(),
        }
    }

    pub fn new_with_router(port: u16, thread_count: usize, router: R) -> HttpServer<R> {
        HttpServer {
            port,
            thread_count,
            router,
        }
    }

    pub fn map_route<F>(&mut self, method: HttpMethod, path: &str, route_fn: F)
    where
        F: Fn(HttpRequestContext, Response) + Send + Sync + 'static,
    {
        self.router.add_route(&method, path, Box::new(route_fn))
    }

    pub fn unmap_route(&mut self, method: HttpMethod, path: &str) -> Option<Arc<R::Route>> {
        self.router.remove_route(&method, path)
    }

    pub fn serve_n(&self, n: u64) {
        let listen_addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        let listener = TcpListener::bind(listen_addr).unwrap();
        let pool = ThreadPool::new(self.thread_count);

        if n == 0 {
            return;
        }

        let mut i = 0;
        for stream in listener.incoming() {
            let stream = stream.unwrap();
            let router = self.router.clone(); // TODO: this seems inefficient...
            pool.execute(move || handle_request_from_stream(stream, &router));

            i += 1;
            if i == n {
                break;
            }
        }
    }

    pub fn serve(&self) {
        let listen_addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        let listener = TcpListener::bind(listen_addr).unwrap();
        let pool = ThreadPool::new(self.thread_count);

        for stream in listener.incoming() {
            let stream = stream.unwrap();
            let router = self.router.clone(); // TODO: this seems inefficient...
            pool.execute(move || handle_request_from_stream(stream, &router));
        }
    }

    // pub fn handle(&self, request: HttpRequestContext) {
    //     handle_request(request, &self.router)
    // }
}

pub struct Response {
    stream: TcpStream,
}

impl Response {
    pub fn ok(&self, headers: &HttpHeaders, body: impl Read) {
        HttpPrinter::new(&self.stream)
            .write_response2(&HttpStatus::of(200), headers, body)
            .expect("TODO: handle error");
    }
    pub fn send(&self, status: &HttpStatus, headers: &HttpHeaders, body: impl Read) {
        HttpPrinter::new(&self.stream)
            .write_response2(status, headers, body)
            .expect("TODO: handle error");
    }
}

pub struct HttpRequestContext {
    pub headers: HttpHeaders,
    pub method: HttpMethod,
    pub uri: String,
    body: HttpBodyReader,
}

impl Read for HttpBodyReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }

        let max = cmp::min(buf.len() as u64, self.remaining) as usize;
        let n = self.reader.read(&mut buf[..max])?;
        assert!(
            n as u64 <= self.remaining,
            "number of read bytes exceeds limit"
        );
        self.remaining -= n as u64;
        Ok(n)
    }
}

pub struct HttpBodyReader {
    pub reader: BufReader<TcpStream>,
    pub remaining: u64,
}

impl HttpBodyReader {
    pub fn set_remaining_bytes(&mut self, value: u64) {
        self.remaining = value;
    }
    pub fn get_reader(&mut self) -> &mut BufReader<TcpStream> {
        &mut self.reader
    }
}

impl HttpRequestContext {
    pub fn get_body_reader(&mut self) -> &mut HttpBodyReader {
        &mut self.body
    }

    pub fn read_body(&mut self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.body.read_to_end(&mut buf).unwrap();
        buf
    }

    pub fn read_body_to_string(&mut self) -> String {
        let mut buf = String::new();
        self.body.read_to_string(&mut buf).unwrap();
        buf
    }
}

fn handle_request<R>(ctx: HttpRequestContext, response: Response, router: &R)
where
    R: AppRouter<Route = Box<RouteFn>>,
{
    let route = router.match_route(&ctx.method, &ctx.uri);
    match route {
        Some(route) => (route)(ctx, response),
        None => default_404_handler(ctx, response),
    }
}

fn handle_request_from_stream<R>(stream: TcpStream, router: &R)
where
    R: AppRouter<Route = Box<RouteFn>>,
{
    let parts = parse_request_parts(stream.try_clone().unwrap());

    if parts.is_err() {
        panic!("TODO: handle failed parsing");
    }
    let parts = parts.unwrap();

    let content_len = parts.headers.get_content_length().unwrap_or(0);
    let ctx = HttpRequestContext {
        method: parts.method,
        headers: parts.headers,
        uri: parts.uri,
        body: HttpBodyReader {
            reader: parts.reader,
            remaining: content_len as u64,
        },
    };
    let response = Response { stream };
    handle_request(ctx, response, router);
}

fn default_404_handler(_ctx: HttpRequestContext, response: Response) {
    response.send(&HttpStatus::of(404), &HttpHeaders::new(), &[][..]);
}

fn default_http_parsing_error_handler(_ctx: HttpRequestContext, response: Response) {
    response.send(&HttpStatus::of(500), &HttpHeaders::new(), &[][..]);
}
