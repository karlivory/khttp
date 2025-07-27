use crate::body_reader::BodyReader;
use crate::common::{Headers, Method, RequestUri, Status};
use crate::parser::{HttpParsingError, RequestParser, RequestParts};
use crate::printer::HttpPrinter;
use crate::router::{AppRouter, DefaultRouter};
use crate::threadpool::ThreadPool;
use std::collections::HashMap;
use std::io::{self, Read};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, LazyLock};
use std::time::Instant;

const DEFAULT_THREAD_COUNT: usize = 20;
pub type RouteFn =
    dyn Fn(RequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static;
pub type StreamSetupFn = dyn Fn(io::Result<TcpStream>) -> StreamSetupAction + Send + Sync + 'static;
pub type PreRoutingHookFn = dyn Fn(RequestParts<TcpStream>, &mut ResponseHandle) -> PreRoutingAction
    + Send
    + Sync
    + 'static;

impl Server<DefaultRouter<Box<RouteFn>>> {
    pub fn builder<A: ToSocketAddrs>(
        addr: A,
    ) -> io::Result<ServerBuilder<DefaultRouter<Box<RouteFn>>>> {
        let bind_addrs: Vec<SocketAddr> = addr.to_socket_addrs()?.collect();

        if bind_addrs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid address",
            ));
        }

        Ok(ServerBuilder {
            bind_addrs,
            thread_count: DEFAULT_THREAD_COUNT,
            router: DefaultRouter::<Box<RouteFn>>::new(),
            fallback_route: Arc::new(Box::new(|_, r| {
                r.send(&Status::NOT_FOUND, Headers::new(), &[][..])
            })),
            stream_setup_hook: None,
            pre_routing_hook: None,
        })
    }
}

pub struct Server<R> {
    bind_addrs: Vec<SocketAddr>,
    thread_count: usize,
    router: Arc<R>,
    fallback_route: Arc<Box<RouteFn>>,
    stream_setup_hook: Option<Arc<StreamSetupFn>>,
    pre_routing_hook: Option<Arc<PreRoutingHookFn>>,
}

pub enum StreamSetupAction {
    Accept(TcpStream),
    Skip,
    StopAccepting,
}

pub enum PreRoutingAction {
    Proceed(RequestParts<TcpStream>),
    Skip,
    Disconnect(io::Result<()>),
}

pub struct ServerBuilder<R> {
    bind_addrs: Vec<SocketAddr>,
    thread_count: usize,
    router: R,
    fallback_route: Arc<Box<RouteFn>>,
    stream_setup_hook: Option<Arc<StreamSetupFn>>,
    pre_routing_hook: Option<Arc<PreRoutingHookFn>>,
}

impl<R> ServerBuilder<R>
where
    R: AppRouter<Route = Box<RouteFn>> + Send + Sync + 'static,
{
    pub fn route<F>(&mut self, method: Method, path: &str, route_fn: F)
    where
        F: Fn(RequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static,
    {
        self.router.add_route(&method, path, Box::new(route_fn));
    }

    pub fn set_thread_count(&mut self, thread_count: usize) {
        self.thread_count = thread_count;
    }

    pub fn set_stream_setup_hook<F>(&mut self, f: F)
    where
        F: Fn(io::Result<TcpStream>) -> StreamSetupAction + Send + Sync + 'static,
    {
        self.stream_setup_hook = Some(Arc::new(f));
    }

    pub fn set_pre_routing_hook<F>(&mut self, f: F)
    where
        F: Fn(RequestParts<TcpStream>, &mut ResponseHandle) -> PreRoutingAction
            + Send
            + Sync
            + 'static,
    {
        self.pre_routing_hook = Some(Arc::new(f));
    }

    pub fn set_fallback_route<F>(&mut self, f: F)
    where
        F: Fn(RequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static,
    {
        self.fallback_route = Arc::new(Box::new(f));
    }

    pub fn remove_route(&mut self, method: Method, path: &str) -> Option<Arc<R::Route>> {
        self.router.remove_route(&method, path)
    }

    pub fn build(self) -> Server<R> {
        Server {
            bind_addrs: self.bind_addrs,
            thread_count: self.thread_count,
            router: Arc::new(self.router),
            fallback_route: self.fallback_route,
            stream_setup_hook: self.stream_setup_hook,
            pre_routing_hook: self.pre_routing_hook,
        }
    }
}

impl<R> Server<R>
where
    R: AppRouter<Route = Box<RouteFn>> + Send + Sync + 'static,
{
    pub fn serve_n(self, n: u64) -> io::Result<()> {
        if n == 0 {
            return Ok(());
        }
        self.serve_loop(Some(n))
    }

    pub fn port(&self) -> Option<u16> {
        self.bind_addrs.first().map(|a| a.port())
    }

    pub fn serve(self) -> io::Result<()> {
        self.serve_loop(None)
    }

    fn serve_loop(self, limit: Option<u64>) -> io::Result<()> {
        let listener = TcpListener::bind(&*self.bind_addrs)?;
        let pool = ThreadPool::new(self.thread_count);

        let mut i = 0;
        for stream in listener.incoming() {
            let stream = match &self.stream_setup_hook {
                Some(hook) => match (hook)(stream) {
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
            let fallback_route = self.fallback_route.clone();
            let pre_routing_hook = self.pre_routing_hook.clone();

            pool.execute(move || {
                let _ = handle_connection(stream, &router, &fallback_route, &pre_routing_hook);
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
        handle_connection(
            stream,
            &self.router,
            &self.fallback_route,
            &self.pre_routing_hook,
        )
    }
}

pub struct ResponseHandle<'a> {
    stream: &'a mut TcpStream,
}

impl ResponseHandle<'_> {
    pub fn ok(&mut self, headers: Headers, body: impl Read) -> io::Result<()> {
        self.send(&Status::of(200), headers, body)
    }

    pub fn send_chunked(
        &mut self,
        status: &Status,
        mut headers: Headers,
        body: impl Read,
    ) -> io::Result<()> {
        headers.remove(Headers::CONTENT_LENGTH);
        headers.set_transfer_encoding_chunked();
        self.send(status, headers, body)
    }

    pub fn send(&mut self, status: &Status, headers: Headers, body: impl Read) -> io::Result<()> {
        let mut p = HttpPrinter::new(&mut self.stream);
        p.write_response(status, headers, body)
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

pub struct RequestContext<'a, 'r> {
    pub headers: Headers,
    pub method: Method,
    pub route_params: &'r HashMap<&'a str, &'r str>,
    pub uri: &'r RequestUri,
    pub http_version: &'r str,
    body: BodyReader<TcpStream>,
}

impl RequestContext<'_, '_> {
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
    fallback_route: &Arc<Box<RouteFn>>,
    pre_routing_hook: &Option<Arc<PreRoutingHookFn>>,
) -> io::Result<()>
where
    R: AppRouter<Route = Box<RouteFn>>,
{
    loop {
        let keep_alive = handle_one_request(&mut stream, router, fallback_route, pre_routing_hook)?;
        if !keep_alive {
            return Ok(());
        }
    }
}

fn handle_one_request<R>(
    stream: &mut TcpStream,
    router: &Arc<R>,
    handler_404: &Arc<Box<RouteFn>>,
    pre_routing_hook: &Option<Arc<PreRoutingHookFn>>,
) -> io::Result<bool>
where
    R: AppRouter<Route = Box<RouteFn>>,
{
    let read_stream = stream.try_clone()?;
    let mut parts = match RequestParser::new(read_stream).parse() {
        Ok(p) => p,
        Err(HttpParsingError::IOError(e)) if e.kind() == io::ErrorKind::WouldBlock => {
            return Ok(true);
        }
        Err(HttpParsingError::UnexpectedEof) => return Ok(false),
        Err(_) => {
            HttpPrinter::new(stream).write_response(&Status::of(400), Headers::new(), &[][..])?;
            return Ok(false);
        }
    };

    if parts.headers.is_100_continue() {
        HttpPrinter::new(stream.try_clone().unwrap()).write_100_continue()?;
    }

    let mut response = ResponseHandle { stream };

    if let Some(hook) = pre_routing_hook {
        parts = match (hook)(parts, &mut response) {
            PreRoutingAction::Proceed(p) => p,
            PreRoutingAction::Skip => return Ok(true),
            PreRoutingAction::Disconnect(r) => return r.map(|_| false),
        };
    }

    let matched = router.match_route(&parts.method, parts.uri.path());
    let (handler, params) = match &matched {
        Some(r) => (r.route, &r.params),
        None => (handler_404, &*EMPTY_PARAMS),
    };

    let body = BodyReader::from(&parts.headers, parts.reader);
    let ctx = RequestContext {
        method: parts.method,
        headers: parts.headers,
        uri: &parts.uri,
        http_version: &parts.http_version,
        route_params: params,
        body,
    };

    let should_close = ctx.headers.is_connection_close();
    (handler)(ctx, &mut response)?;

    Ok(!should_close)
}

        }

        let matched = router.match_route(&parts.method, parts.uri.path());
        let (handler, params) = match &matched {
            Some(r) => (r.route, &r.params),
            None => (handler_404, &*EMPTY_PARAMS),
        };

        let body = BodyReader::from(&parts.headers, parts.reader);
        let ctx = RequestContext {
            method: parts.method,
            headers: parts.headers,
            uri: &parts.uri,
            http_version: &parts.http_version,
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

}
