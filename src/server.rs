// src/server.rs
use crate::common::{HttpBodyReader, HttpHeaders, HttpMethod, HttpStatus};
use crate::http_parser::{HttpParsingError, HttpRequestParser};
use crate::http_printer::HttpPrinter;
use crate::router::{AppRouter, DefaultRouter};
use crate::threadpool::ThreadPool;
use std::io::Read;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;

pub struct App {}

static DEFAULT_THREAD_COUNT: usize = 20;

impl App {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(bind_address: &str, port: u16) -> HttpServer<DefaultRouter<Box<RouteFn>>> {
        HttpServer {
            bind_address: bind_address.to_string(),
            tcp_nodelay: false,
            port,
            thread_count: DEFAULT_THREAD_COUNT,
            router: DefaultRouter::<Box<RouteFn>>::new(),
        }
    }
}

pub type RouteFn = dyn Fn(HttpRequestContext, &mut ResponseHandle) + Send + Sync + 'static;

pub struct HttpServer<R>
where
    R: AppRouter<Route = Box<RouteFn>>,
{
    bind_address: String,
    tcp_nodelay: bool,
    port: u16,
    thread_count: usize,
    router: R,
}

impl<R> HttpServer<R>
where
    R: AppRouter<Route = Box<RouteFn>> + Send + 'static,
{
    pub fn map_route<F>(&mut self, method: HttpMethod, path: &str, route_fn: F)
    where
        F: Fn(HttpRequestContext, &mut ResponseHandle) + Send + Sync + 'static,
    {
        self.router.add_route(&method, path, Box::new(route_fn))
    }

    pub fn set_thread_count(&mut self, thread_count: usize) {
        self.thread_count = thread_count;
    }

    pub fn set_tcp_nodelay(&mut self, tcp_nodelay: bool) {
        self.tcp_nodelay = tcp_nodelay;
    }

    pub fn port(&self) -> &u16 {
        &self.port
    }

    pub fn unmap_route(&mut self, method: HttpMethod, path: &str) -> Option<Arc<R::Route>> {
        self.router.remove_route(&method, path)
    }

    pub fn serve_n(&self, n: u64) {
        if n == 0 {
            return;
        }

        let listener = TcpListener::bind((self.bind_address.as_str(), self.port)).unwrap();
        let pool = ThreadPool::new(self.thread_count);

        let mut i = 0;
        for stream in listener.incoming() {
            let stream = stream.unwrap();
            if self.tcp_nodelay {
                stream.set_nodelay(true).unwrap();
            }
            let router = self.router.clone(); // TODO: this seems inefficient...
            pool.execute(move || handle_request_from_stream(stream, &router));

            i += 1;
            if i == n {
                break;
            }
        }
    }

    pub fn serve(&self) {
        let listener = TcpListener::bind((self.bind_address.as_str(), self.port)).unwrap();
        let pool = ThreadPool::new(self.thread_count);

        for stream in listener.incoming() {
            let stream = stream.unwrap();
            if self.tcp_nodelay {
                stream.set_nodelay(true).unwrap();
            }
            let router = self.router.clone();
            pool.execute(move || handle_request_from_stream(stream, &router));
        }
    }
}

pub struct ResponseHandle<'a> {
    stream: &'a mut TcpStream,
}

impl ResponseHandle<'_> {
    pub fn ok(&mut self, headers: HttpHeaders, body: impl Read) {
        self.send(&HttpStatus::of(200), headers, body);
    }

    pub fn send(&mut self, status: &HttpStatus, headers: HttpHeaders, body: impl Read) {
        let should_close = headers
            .get("connection")
            .map(|v| v.eq_ignore_ascii_case("close"))
            .unwrap_or(false);

        // TODO: what to do about io errors?
        let _ = HttpPrinter::new(&mut self.stream).write_response(status, headers, body);

        if should_close {
            let _ = self.stream.shutdown(Shutdown::Both);
        }
    }
}

pub struct HttpRequestContext {
    pub headers: HttpHeaders,
    pub method: HttpMethod,
    pub uri: String,
    body: HttpBodyReader<TcpStream>,
}

impl HttpRequestContext {
    pub fn get_body_reader(&mut self) -> &mut HttpBodyReader<TcpStream> {
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

fn handle_request<R>(ctx: HttpRequestContext, response: &mut ResponseHandle, router: &R)
where
    R: AppRouter<Route = Box<RouteFn>>,
{
    let route = router.match_route(&ctx.method, &ctx.uri);
    match route {
        Some(route) => (route)(ctx, response),
        None => default_404_handler(ctx, response),
    }
}

fn handle_request_from_stream<R>(mut stream: TcpStream, router: &R)
where
    R: AppRouter<Route = Box<RouteFn>>,
{
    loop {
        let parsed = HttpRequestParser::new(stream.try_clone().unwrap()).parse();

        match parsed {
            Ok(parts) => {
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

                let mut response = ResponseHandle {
                    stream: &mut stream,
                };

                handle_request(ctx, &mut response, router);
            }

            Err(HttpParsingError::IOError) => {
                let _ = stream.shutdown(Shutdown::Both);
                break;
            }

            Err(_) => {
                let _ = HttpPrinter::new(&mut stream).write_response(
                    &HttpStatus::of(400),
                    HttpHeaders::new(),
                    &[][..],
                );
                let _ = stream.shutdown(Shutdown::Both);
                break;
            }
        }
    }
}

fn default_404_handler(_ctx: HttpRequestContext, response: &mut ResponseHandle) {
    let mut headers = HttpHeaders::new();
    headers.set_content_length(0);
    headers.add("connection", "close");
    response.send(&HttpStatus::of(404), headers, &[][..]);
}
