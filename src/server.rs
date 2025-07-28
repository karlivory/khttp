use crate::threadpool::ThreadPool;
use crate::{
    BodyReader, Headers, HttpParsingError, HttpPrinter, HttpRouter, Method, Parser, RequestParts,
    RequestUri, Router, Status,
};
use std::collections::HashMap;
use std::io::{self, Read};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::{Arc, LazyLock};
use std::time::Instant;

const DEFAULT_THREAD_COUNT: usize = 20;

pub type RouteFn =
    dyn Fn(RequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static;
pub type StreamSetupFn = dyn Fn(io::Result<TcpStream>) -> StreamSetupAction + Send + Sync + 'static;
pub type PreRoutingHookFn = dyn Fn(RequestParts<TcpStream>, &ConnectionMeta, &mut ResponseHandle) -> PreRoutingAction
    + Send
    + Sync
    + 'static;

struct HandlerConfig {
    fallback_route: Box<RouteFn>,
    pre_routing_hook: Option<Box<PreRoutingHookFn>>,
    // parser options
    max_status_line_length: Option<usize>,
    max_header_line_length: Option<usize>,
    max_header_count: Option<usize>,
}

impl Server<Router<Box<RouteFn>>> {
    pub fn builder<A: ToSocketAddrs>(addr: A) -> io::Result<ServerBuilder<Router<Box<RouteFn>>>> {
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
            router: Router::<Box<RouteFn>>::new(),
            fallback_route: Box::new(|_, r| r.send(&Status::NOT_FOUND, Headers::new(), &[][..])),
            stream_setup_hook: None,
            pre_routing_hook: None,
            max_status_line_length: None,
            max_header_line_length: None,
            max_header_count: None,
        })
    }
}

pub struct Server<R> {
    bind_addrs: Vec<SocketAddr>,
    thread_count: usize,
    router: Arc<R>,
    stream_setup_hook: Option<Box<StreamSetupFn>>,
    handler_config: Arc<HandlerConfig>,
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
    fallback_route: Box<RouteFn>,
    stream_setup_hook: Option<Box<StreamSetupFn>>,
    pre_routing_hook: Option<Box<PreRoutingHookFn>>,
    max_status_line_length: Option<usize>,
    max_header_line_length: Option<usize>,
    max_header_count: Option<usize>,
}

impl<R> ServerBuilder<R>
where
    R: HttpRouter<Route = Box<RouteFn>> + Send + Sync + 'static,
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
        self.stream_setup_hook = Some(Box::new(f));
    }

    pub fn set_pre_routing_hook<F>(&mut self, f: F)
    where
        F: Fn(RequestParts<TcpStream>, &ConnectionMeta, &mut ResponseHandle) -> PreRoutingAction
            + Send
            + Sync
            + 'static,
    {
        self.pre_routing_hook = Some(Box::new(f));
    }

    pub fn set_fallback_route<F>(&mut self, f: F)
    where
        F: Fn(RequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static,
    {
        self.fallback_route = Box::new(f);
    }

    pub fn set_max_status_line_length(&mut self, value: Option<usize>) {
        self.max_status_line_length = value;
    }

    pub fn set_max_header_line_length(&mut self, value: Option<usize>) {
        self.max_header_line_length = value;
    }

    pub fn set_max_header_count(&mut self, value: Option<usize>) {
        self.max_header_count = value;
    }

    pub fn remove_route(&mut self, method: Method, path: &str) -> Option<R::Route> {
        self.router.remove_route(&method, path)
    }

    pub fn build(self) -> Server<R> {
        Server {
            bind_addrs: self.bind_addrs,
            thread_count: self.thread_count,
            router: Arc::new(self.router),
            stream_setup_hook: self.stream_setup_hook,
            handler_config: Arc::new(HandlerConfig {
                fallback_route: self.fallback_route,
                pre_routing_hook: self.pre_routing_hook,
                max_status_line_length: self.max_status_line_length,
                max_header_line_length: self.max_header_line_length,
                max_header_count: self.max_header_count,
            }),
        }
    }
}

impl<R> Server<R>
where
    R: HttpRouter<Route = Box<RouteFn>> + Send + Sync + 'static,
{
    pub fn port(&self) -> Option<u16> {
        self.bind_addrs.first().map(|a| a.port())
    }

    pub fn serve(self) -> io::Result<()> {
        let listener = TcpListener::bind(&*self.bind_addrs)?;
        let pool = ThreadPool::new(self.thread_count);

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
            let config = self.handler_config.clone();

            pool.execute(move || {
                let _ = handle_connection(stream, &router, config);
            });
        }
        Ok(())
    }

    pub fn handle(&self, stream: TcpStream) -> io::Result<()> {
        handle_connection(stream, &self.router, self.handler_config.clone())
    }
}

pub struct ResponseHandle<'a> {
    stream: &'a mut TcpStream,
    keep_alive: bool,
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
        if headers.is_connection_close() {
            self.keep_alive = false;
        }
        let mut p = HttpPrinter::new(&mut self.stream);
        p.write_response(status, headers, body)
    }

    pub fn send_100_continue(&mut self) -> io::Result<()> {
        let mut p = HttpPrinter::new(&mut self.stream);
        p.write_100_continue()
    }

    pub fn get_stream(&mut self) -> &TcpStream {
        self.stream
    }

    pub fn get_stream_mut(&mut self) -> &mut TcpStream {
        self.stream
    }
}

pub struct RequestContext<'a, 'r> {
    pub headers: Headers,
    pub method: Method,
    pub route_params: &'r HashMap<&'a str, &'r str>,
    pub uri: &'r RequestUri,
    pub http_version: &'r u8,
    pub conn: &'r ConnectionMeta,
    body: BodyReader<TcpStream>,
}

impl RequestContext<'_, '_> {
    pub fn body(&mut self) -> &mut BodyReader<TcpStream> {
        &mut self.body
    }

    pub fn get_stream(&self) -> &TcpStream {
        self.body.inner().get_ref()
    }

    pub fn get_stream_mut(&mut self) -> &mut TcpStream {
        self.body.inner_mut().get_mut()
    }
}

static EMPTY_PARAMS: LazyLock<HashMap<&str, &str>> = LazyLock::new(HashMap::new);

pub struct ConnectionMeta {
    index: usize,
    conn_start: Instant,
}

impl ConnectionMeta {
    fn new() -> Self {
        Self {
            index: 0,
            conn_start: Instant::now(),
        }
    }

    pub fn increment(&mut self) {
        self.index = self.index.wrapping_add(1);
    }

    pub fn index(&self) -> &usize {
        &self.index
    }

    pub fn conn_start(&self) -> &Instant {
        &self.conn_start
    }
}

fn handle_connection<R>(
    mut stream: TcpStream,
    router: &Arc<R>,
    config: Arc<HandlerConfig>,
) -> io::Result<()>
where
    R: HttpRouter<Route = Box<RouteFn>>,
{
    let mut connection_meta = ConnectionMeta::new();
    loop {
        connection_meta.increment();
        let keep_alive = handle_one_request(&mut stream, router, &config, &connection_meta)?;
        if !keep_alive {
            return Ok(());
        }
    }
}

/// returns whether to keep the stream alive for the next request
fn handle_one_request<R>(
    stream: &mut TcpStream,
    router: &Arc<R>,
    config: &HandlerConfig,
    connection_meta: &ConnectionMeta,
) -> io::Result<bool>
where
    R: HttpRouter<Route = Box<RouteFn>>,
{
    let read_stream = stream.try_clone()?;
    let mut parts = match Parser::new(read_stream).parse_request(
        &config.max_status_line_length,
        &config.max_header_line_length,
        &config.max_header_count,
    ) {
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

    let mut response = ResponseHandle {
        stream,
        keep_alive: true,
    };

    if let Some(hook) = &config.pre_routing_hook {
        parts = match (hook)(parts, connection_meta, &mut response) {
            PreRoutingAction::Proceed(p) => p,
            PreRoutingAction::Skip => return Ok(true),
            PreRoutingAction::Disconnect(r) => return r.map(|_| false),
        };
    }

    let matched = router.match_route(&parts.method, parts.uri.path());
    let (handler, params) = match &matched {
        Some(r) => (r.route, &r.params),
        None => (&config.fallback_route, &*EMPTY_PARAMS),
    };

    let body = BodyReader::from(&parts.headers, parts.reader);
    let ctx = RequestContext {
        method: parts.method,
        headers: parts.headers,
        uri: &parts.uri,
        http_version: &parts.http_version,
        route_params: params,
        conn: connection_meta,
        body,
    };

    let client_requested_close = ctx.headers.is_connection_close();
    (handler)(ctx, &mut response)?;

    if client_requested_close {
        return Ok(false);
    }
    Ok(response.keep_alive)
}

#[cfg(feature = "epoll")]
impl<R> Server<R>
where
    R: HttpRouter<Route = Box<RouteFn>> + Send + Sync + 'static,
{
    pub fn serve_epoll(self) -> io::Result<()> {
        use libc::{
            EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLLET, EPOLLIN, epoll_create1, epoll_ctl, epoll_event,
            epoll_wait,
        };
        use libc::{EPOLL_CTL_MOD, EPOLLONESHOT};
        use std::os::unix::io::{AsRawFd, RawFd};
        use std::sync::Mutex;

        let listener = TcpListener::bind(&*self.bind_addrs)?;
        listener.set_nonblocking(true)?;

        let epfd = unsafe { epoll_create1(0) };
        if epfd == -1 {
            return Err(io::Error::last_os_error());
        }
        let epfd = Arc::new(epfd);

        let listener_fd = listener.as_raw_fd();
        let mut ev = epoll_event {
            events: (EPOLLIN | EPOLLET) as u32,
            u64: listener_fd as u64,
        };
        if unsafe { epoll_ctl(*epfd, EPOLL_CTL_ADD, listener_fd, &mut ev) } == -1 {
            return Err(io::Error::last_os_error());
        }

        struct Connection(TcpStream, ConnectionMeta);

        let connections: Arc<Mutex<HashMap<RawFd, Connection>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pool = ThreadPool::new(self.thread_count);
        let mut events = vec![epoll_event { events: 0, u64: 0 }; 1024];

        loop {
            let n = unsafe { epoll_wait(*epfd, events.as_mut_ptr(), 1024, -1) };
            if n == -1 {
                return Err(io::Error::last_os_error());
            }

            for ev in &events[..n as usize] {
                let fd = ev.u64 as RawFd;

                if fd == listener_fd {
                    loop {
                        match listener.accept() {
                            Ok((mut stream, _)) => {
                                if let Some(hook) = &self.stream_setup_hook {
                                    stream = match (hook)(Ok(stream)) {
                                        StreamSetupAction::Accept(s) => s,
                                        StreamSetupAction::Skip => continue,
                                        StreamSetupAction::StopAccepting => return Ok(()),
                                    }
                                }

                                let client_fd = stream.as_raw_fd();
                                let mut ev = epoll_event {
                                    events: (EPOLLIN | EPOLLONESHOT | EPOLLET) as u32,
                                    u64: client_fd as u64,
                                };

                                if unsafe { epoll_ctl(*epfd, EPOLL_CTL_ADD, client_fd, &mut ev) }
                                    == -1
                                {
                                    continue;
                                }

                                let conn = Connection(stream, ConnectionMeta::new());
                                connections.lock().unwrap().insert(client_fd, conn);
                            }
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                    }
                } else {
                    let router = Arc::clone(&self.router);
                    let config = Arc::clone(&self.handler_config);

                    let connections = Arc::clone(&connections);
                    let epfd = Arc::clone(&epfd);

                    pool.execute(move || {
                        let mut conn = {
                            match connections.lock().unwrap().remove(&fd) {
                                Some(s) => s,
                                None => return,
                            }
                        };

                        conn.1.increment();
                        let keep_alive = handle_one_request(&mut conn.0, &router, &config, &conn.1)
                            .unwrap_or(false);

                        if keep_alive {
                            {
                                connections.lock().unwrap().insert(fd, conn);
                            }

                            let mut ev = epoll_event {
                                events: (EPOLLIN | EPOLLONESHOT | EPOLLET) as u32,
                                u64: fd as u64,
                            };
                            unsafe {
                                epoll_ctl(*epfd, EPOLL_CTL_MOD, fd, &mut ev);
                            }
                        } else {
                            unsafe {
                                epoll_ctl(*epfd, EPOLL_CTL_DEL, fd, std::ptr::null_mut());
                            }
                        }
                    });
                }
            }
        }
    }
}
