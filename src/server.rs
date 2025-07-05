// src/server.rs
use crate::common::{HttpRequest, HttpResponse};
use crate::http_parser::HttpParser;
use crate::http_printer::HttpPrinter;
use crate::router::AppRouter;
use crate::threadpool::ThreadPool;
use std::net::{SocketAddr, TcpListener, TcpStream};

pub struct App<R>
where
    R: AppRouter,
{
    port: u16,
    thread_count: usize,
    pub router: R,
}

impl<R> App<R>
where
    R: AppRouter + Send + 'static,
{
    pub fn new(port: u16, thread_count: usize) -> App<R> {
        App {
            port,
            thread_count,
            router: R::new(),
        }
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
            pool.execute(move || handle_request(stream, router));

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
            pool.execute(move || handle_request(stream, router));
        }
    }

    pub fn handle(&self, request: HttpRequest) -> HttpResponse {
        self.router.handle(request)
    }
}

fn handle_request<R>(stream: TcpStream, router: R)
where
    R: AppRouter,
{
    let response = match HttpParser::new(&stream).parse_request() {
        Ok(request) => &router.handle(request),
        Err(_) => router.get_http_parsing_error_response(),
    };
    let write_result = HttpPrinter::new(stream).write_response(response);
    if write_result.is_err() {
        // just log the error
        println!("ERROR: failed to write response");
        dbg!(response);
    }
}
