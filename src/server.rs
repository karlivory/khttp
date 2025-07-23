// src/server.rs
use crate::common::{HttpBodyReader, HttpHeaders, HttpMethod, HttpStatus};
use crate::http_parser::{HttpParsingError, HttpRequestParser};
use crate::http_printer::HttpPrinter;
use crate::router::{AppRouter, DefaultRouter};
use crate::threadpool::ThreadPool;
use std::collections::HashMap;
use std::io::Read;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, LazyLock};
use std::time::Instant;

pub struct App {}

const DEFAULT_THREAD_COUNT: usize = 20;

impl App {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(bind_address: &str, port: u16) -> HttpServer<DefaultRouter<Box<RouteFn>>> {
        HttpServer {
            bind_address: bind_address.to_string(),
            tcp_nodelay: false,
            port,
            thread_count: DEFAULT_THREAD_COUNT,
            router: DefaultRouter::<Box<RouteFn>>::new(),
            fallback_route: Arc::new(Box::new(default_404_handler)),
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
    fallback_route: Arc<Box<RouteFn>>,
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

    pub fn set_fallback_route<F>(&mut self, f: F)
    where
        F: Fn(HttpRequestContext, &mut ResponseHandle) + Send + Sync + 'static,
    {
        self.fallback_route = Arc::new(Box::new(f));
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
            let handler_404 = self.fallback_route.clone();
            pool.execute(move || handle_connection(stream, &router, handler_404));

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
            let handler_404 = self.fallback_route.clone();
            pool.execute(move || handle_connection(stream, &router, handler_404));
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

pub struct ConnectionMeta {
    pub req_index: usize,
    pub started_at: Instant,
    pub last_activity: Instant,
}

pub struct HttpRequestContext<'c, 'r> {
    pub headers: HttpHeaders,
    pub method: HttpMethod,
    pub route_params: &'r HashMap<&'r str, &'r str>,
    pub uri: &'r String,
    pub conn: &'c ConnectionMeta,
    body: HttpBodyReader<TcpStream>,
}

impl HttpRequestContext<'_, '_> {
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

static EMPTY_PARAMS: LazyLock<HashMap<&str, &str>> = LazyLock::new(HashMap::new);

fn handle_connection<R>(mut stream: TcpStream, router: &R, handler_404: Arc<Box<RouteFn>>)
where
    R: AppRouter<Route = Box<RouteFn>>,
{
    let mut conn_meta = ConnectionMeta {
        req_index: 0,
        last_activity: Instant::now(),
        started_at: Instant::now(),
    };

    loop {
        let parts = match HttpRequestParser::new(stream.try_clone().unwrap()).parse() {
            Ok(p) => p,
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
        };

        conn_meta.req_index = conn_meta.req_index.wrapping_add(1);
        conn_meta.last_activity = Instant::now();

        let matched = router.match_route(&parts.method, &parts.uri);
        let (handler, params) = match &matched {
            Some(r) => (r.route, &r.params),
            None => (&handler_404, &*EMPTY_PARAMS),
        };

        let content_len = parts.headers.get_content_length().unwrap_or(0) as u64;
        let mut response = ResponseHandle {
            stream: &mut stream,
        };

        let ctx = HttpRequestContext {
            method: parts.method,
            headers: parts.headers,
            uri: &parts.uri,
            conn: &conn_meta,
            route_params: params,
            body: HttpBodyReader {
                reader: parts.reader,
                remaining: content_len,
            },
        };

        (handler)(ctx, &mut response);
    }
}

fn default_404_handler(_ctx: HttpRequestContext, response: &mut ResponseHandle) {
    let headers = HttpHeaders::new();
    response.send(&HttpStatus::of(404), headers, &[][..]);
}
