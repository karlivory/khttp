// src/server.rs
use crate::common::{HttpHeaders, HttpMethod, HttpRequest, HttpResponse, HttpStatus};
use crate::http_parser::HttpParser;
use crate::http_printer::HttpPrinter;
use crate::router::{AppRouter, DefaultRouter, RouteFn};
use crate::threadpool::ThreadPool;
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
        F: Fn(HttpRequest) -> HttpResponse + Send + Sync + 'static,
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

    pub fn handle(&self, request: HttpRequest) -> HttpResponse {
        handle_request(request, &self.router)
    }
}

fn handle_request<R>(request: HttpRequest, router: &R) -> HttpResponse
where
    R: AppRouter<Route = Box<RouteFn>>,
{
    let route = router.match_route(&request.method, &request.uri);

    let mut response = match route {
        Some(route) => (route)(request),
        None => default_404_handler(request),
    };

    if let Some(ref body) = response.body {
        response.headers.set_content_length(body.len());
    }
    response
}

fn handle_request_from_stream<R>(stream: TcpStream, router: &R)
where
    R: AppRouter<Route = Box<RouteFn>>,
{
    let response = match HttpParser::new(&stream).parse_request() {
        Ok(request) => handle_request(request, router),
        Err(_) => default_http_parsing_error_response(),
    };
    let write_result = HttpPrinter::new(stream).write_response(&response);
    if write_result.is_err() {
        // just log the error
        println!("ERROR: failed to write response");
        dbg!(response);
    }
}

fn default_404_handler(_request: HttpRequest) -> HttpResponse {
    HttpResponse {
        status: HttpStatus::of(404),
        headers: HttpHeaders::new(),
        body: None,
    }
}

fn default_http_parsing_error_response() -> HttpResponse {
    HttpResponse {
        body: None,
        headers: Default::default(),
        status: HttpStatus::of(500),
    }
}
