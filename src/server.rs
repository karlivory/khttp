use crate::body_reader::BodyReader;
use crate::common::{HttpHeaders, HttpMethod, HttpStatus};
use crate::http_parser::{HttpParsingError, HttpRequestParser};
use crate::http_printer::HttpPrinter;
use crate::router::{AppRouter, DefaultRouter};
use crate::threadpool::ThreadPool;
use std::collections::HashMap;
use std::io::{self, Read};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::{Arc, LazyLock};
use std::time::Instant;

pub struct App {}

const DEFAULT_THREAD_COUNT: usize = 20;

impl App {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(bind_address: &str, port: u16) -> HttpServerBuilder<DefaultRouter<Box<RouteFn>>> {
        HttpServerBuilder {
            bind_address: bind_address.to_string(),
            port,
            thread_count: DEFAULT_THREAD_COUNT,
            router: DefaultRouter::<Box<RouteFn>>::new(),
            fallback_route: Arc::new(Box::new(default_404_handler)),
            stream_setup_fn: None,
        }
    }
}

pub type RouteFn =
    dyn Fn(HttpRequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static;
pub type StreamSetupFn = dyn Fn(io::Result<TcpStream>) -> StreamSetupAction + Send + Sync + 'static;

pub struct HttpServer<R> {
    bind_address: String,
    thread_count: usize,
    port: u16,
    router: Arc<R>,
    fallback_route: Arc<Box<RouteFn>>,
    stream_setup_fn: Option<Arc<StreamSetupFn>>,
}

pub enum StreamSetupAction {
    Accept(TcpStream),
    Skip,
    StopAccepting,
}

pub struct HttpServerBuilder<R> {
    bind_address: String,
    port: u16,
    thread_count: usize,
    router: R,
    fallback_route: Arc<Box<RouteFn>>,
    stream_setup_fn: Option<Arc<StreamSetupFn>>,
}

impl<R> HttpServerBuilder<R>
where
    R: AppRouter<Route = Box<RouteFn>> + Send + Sync + 'static,
{
    pub fn map_route<F>(&mut self, method: HttpMethod, path: &str, route_fn: F)
    where
        F: Fn(HttpRequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static,
    {
        self.router.add_route(&method, path, Box::new(route_fn));
    }

    pub fn set_thread_count(&mut self, thread_count: usize) {
        self.thread_count = thread_count;
    }

    pub fn set_stream_setup_fn<F>(&mut self, f: F)
    where
        F: Fn(io::Result<TcpStream>) -> StreamSetupAction + Send + Sync + 'static,
    {
        self.stream_setup_fn = Some(Arc::new(f));
    }

    pub fn set_fallback_route<F>(&mut self, f: F)
    where
        F: Fn(HttpRequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static,
    {
        self.fallback_route = Arc::new(Box::new(f));
    }

    pub fn port(&self) -> &u16 {
        &self.port
    }

    pub fn unmap_route(&mut self, method: HttpMethod, path: &str) -> Option<Arc<R::Route>> {
        self.router.remove_route(&method, path)
    }

    pub fn build(self) -> HttpServer<R> {
        HttpServer {
            bind_address: self.bind_address,
            thread_count: self.thread_count,
            port: self.port,
            router: Arc::new(self.router),
            fallback_route: self.fallback_route,
            stream_setup_fn: self.stream_setup_fn,
        }
    }
}

impl<R> HttpServer<R>
where
    R: AppRouter<Route = Box<RouteFn>> + Send + Sync + 'static,
{
    pub fn port(&self) -> &u16 {
        &self.port
    }

    pub fn serve_n(self, n: u64) -> io::Result<()> {
        if n == 0 {
            return Ok(());
        }
        self.serve_loop(Some(n))
    }

    pub fn serve(self) -> io::Result<()> {
        self.serve_loop(None)
    }

    fn serve_loop(self, limit: Option<u64>) -> io::Result<()> {
        let listener = TcpListener::bind((self.bind_address.as_str(), self.port))?;
        let pool = ThreadPool::new(self.thread_count);

        let mut i = 0;
        for stream in listener.incoming() {
            let stream = match &self.stream_setup_fn {
                Some(setup_fn) => match (setup_fn)(stream) {
                    StreamSetupAction::Accept(s) => s,
                    StreamSetupAction::Skip => continue,
                    StreamSetupAction::StopAccepting => break,
                },
                None => match stream {
                    Ok(s) => s,
                    Err(_) => continue,
                },
            };

            let router = self.router.clone();
            let handler_404 = self.fallback_route.clone();

            pool.execute(move || {
                let _ = handle_connection(stream, &router, &handler_404);
            });

            if let Some(max) = limit {
                i += 1;
                if i >= max {
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn handle(&self, stream: TcpStream) -> io::Result<()> {
        handle_connection(stream, &self.router, &self.fallback_route)
    }
}

pub struct ResponseHandle<'a> {
    stream: &'a mut TcpStream,
}

impl ResponseHandle<'_> {
    pub fn ok(&mut self, headers: HttpHeaders, body: impl Read) -> io::Result<()> {
        self.send(&HttpStatus::of(200), headers, body)
    }

    pub fn send_chunked(
        &mut self,
        status: &HttpStatus,
        mut headers: HttpHeaders,
        body: impl Read,
    ) -> io::Result<()> {
        headers.remove(HttpHeaders::CONTENT_LENGTH);
        headers.set_transfer_encoding_chunked();
        self.send(status, headers, body)
    }

    pub fn send(
        &mut self,
        status: &HttpStatus,
        headers: HttpHeaders,
        body: impl Read,
    ) -> io::Result<()> {
        let should_close = headers.is_connection_close();

        {
            let mut p = HttpPrinter::new(&mut self.stream);
            p.write_response(status, headers, body)?;
            p.flush()?;
        }

        if should_close {
            self.stream.shutdown(Shutdown::Both)?;
        }

        Ok(())
    }

    pub fn get_stream(&mut self) -> &TcpStream {
        self.stream
    }

    pub fn get_stream_mut(&mut self) -> &mut TcpStream {
        self.stream
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
    pub scheme: Option<&'r str>,
    pub absolute_form_authority: Option<&'r str>,
    pub uri: &'r str,
    pub http_version: &'r str,
    pub conn: &'c ConnectionMeta,
    body: BodyReader<TcpStream>,
}

impl HttpRequestContext<'_, '_> {
    pub fn get_body_reader(&mut self) -> &mut BodyReader<TcpStream> {
        &mut self.body
    }

    pub fn read_body(&mut self) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.body.read_to_end(&mut buf).map(|_| buf)
    }

    pub fn read_body_to_string(&mut self) -> io::Result<String> {
        let mut buf = String::new();
        self.body.read_to_string(&mut buf).map(|_| buf)
    }

    pub fn get_stream(&self) -> &TcpStream {
        self.body.inner().get_ref()
    }

    pub fn get_stream_mut(&mut self) -> &mut TcpStream {
        self.body.inner_mut().get_mut()
    }
}

static EMPTY_PARAMS: LazyLock<HashMap<&str, &str>> = LazyLock::new(HashMap::new);

pub fn handle_connection<R>(
    mut stream: TcpStream,
    router: &Arc<R>,
    handler_404: &Arc<Box<RouteFn>>,
) -> io::Result<()>
where
    R: AppRouter<Route = Box<RouteFn>>,
{
    let mut conn_meta = ConnectionMeta {
        req_index: 0,
        last_activity: Instant::now(),
        started_at: Instant::now(),
    };

    loop {
        let read_stream = stream.try_clone()?;
        let parts = match HttpRequestParser::new(read_stream).parse() {
            Ok(p) => p,
            Err(HttpParsingError::IOError) => {
                return Ok(());
            }
            Err(_) => {
                return HttpPrinter::new(&mut stream).write_response(
                    &HttpStatus::of(400),
                    HttpHeaders::new(),
                    &[][..],
                );
            }
        };

        if parts.headers.is_100_continue() {
            HttpPrinter::new(&mut stream).write_100_continue()?;
        }

        conn_meta.req_index = conn_meta.req_index.wrapping_add(1);
        conn_meta.last_activity = Instant::now();

        let (scheme, absolute_form_authority, uri) = split_uri(&parts.full_uri);

        let matched = router.match_route(&parts.method, uri);
        let (handler, params) = match &matched {
            Some(r) => (r.route, &r.params),
            None => (handler_404, &*EMPTY_PARAMS),
        };

        let mut response = ResponseHandle {
            stream: &mut stream,
        };
        let body = BodyReader::from(&parts.headers, parts.reader);

        let ctx = HttpRequestContext {
            method: parts.method,
            headers: parts.headers,
            scheme,
            absolute_form_authority,
            uri,
            http_version: &parts.http_version,
            conn: &conn_meta,
            route_params: params,
            body,
        };
        let connection_close = ctx.headers.is_connection_close();

        (handler)(ctx, &mut response)?;
        if connection_close {
            return Ok(());
        }
    }
}

pub fn split_uri(full_uri: &str) -> (Option<&str>, Option<&str>, &str) {
    if let Some(scheme_end) = full_uri.find("://") {
        let scheme = &full_uri[..scheme_end];
        let after_scheme = &full_uri[scheme_end + 3..];

        match after_scheme.find('/') {
            Some(path_start) => {
                let authority = &after_scheme[..path_start];
                let path = &after_scheme[path_start..];
                (Some(scheme), Some(authority), path)
            }
            None => (Some(scheme), Some(after_scheme), "/"),
        }
    } else {
        (None, None, full_uri)
    }
}

fn default_404_handler(_ctx: HttpRequestContext, response: &mut ResponseHandle) -> io::Result<()> {
    let headers = HttpHeaders::new();
    response.send(&HttpStatus::of(404), headers, &[][..])
}
