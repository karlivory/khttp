use crate::parser::Request;
use crate::router::{RouteParams, RouterBuilder};
use crate::threadpool::ThreadPool;
use crate::{BodyReader, Headers, HttpPrinter, HttpRouter, Method, RequestUri, Router, Status};
use std::io::{self, Read};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Instant;

const DEFAULT_THREAD_COUNT: usize = 20;
const DEFAULT_MAX_REQUEST_HEAD: usize = 8192;

pub type RouteFn =
    dyn Fn(RequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static;
pub type StreamSetupFn = dyn Fn(io::Result<TcpStream>) -> StreamSetupAction + Send + Sync + 'static;
pub type PreRoutingHookFn = dyn Fn(&mut Request<'_>, &mut ResponseHandle, &ConnectionMeta) -> PreRoutingAction
    + Send
    + Sync
    + 'static;

impl Server<Router<Box<RouteFn>>> {
    pub fn builder<A: ToSocketAddrs>(addr: A) -> io::Result<ServerBuilder> {
        let bind_addrs: Vec<SocketAddr> = addr.to_socket_addrs()?.collect();

        if bind_addrs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid address",
            ));
        }

        Ok(ServerBuilder {
            bind_addrs,
            router: RouterBuilder::<Box<RouteFn>>::new(Box::new(|_, r| {
                r.send(&Status::NOT_FOUND, Headers::empty(), &[][..])
            })),
            thread_count: None,
            stream_setup_hook: None,
            pre_routing_hook: None,
            max_request_head_size: None,
            max_request_header_count: None,
        })
    }
}

struct HandlerConfig<R> {
    router: R,
    pre_routing_hook: Option<Box<PreRoutingHookFn>>, // TODO: implement
    max_request_head: usize,
}

pub struct Server<R> {
    bind_addrs: Vec<SocketAddr>,
    thread_count: usize,
    stream_setup_hook: Option<Box<StreamSetupFn>>,
    handler_config: Arc<HandlerConfig<R>>,
}

pub enum StreamSetupAction {
    Proceed(TcpStream),
    Drop,
    StopAccepting,
}

pub enum PreRoutingAction {
    Proceed,
    Drop,
}

pub struct ServerBuilder {
    bind_addrs: Vec<SocketAddr>,
    router: RouterBuilder<Box<RouteFn>>,
    stream_setup_hook: Option<Box<StreamSetupFn>>,
    pre_routing_hook: Option<Box<PreRoutingHookFn>>,
    thread_count: Option<usize>,
    max_request_head_size: Option<usize>,
    max_request_header_count: Option<usize>,
}

impl ServerBuilder {
    pub fn route<F>(&mut self, method: Method, path: &str, route_fn: F) -> &mut Self
    where
        F: Fn(RequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static,
    {
        self.router.add_route(&method, path, Box::new(route_fn));
        self
    }

    pub fn thread_count(&mut self, thread_count: usize) -> &mut Self {
        self.thread_count = Some(thread_count);
        self
    }

    pub fn stream_setup_hook<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(io::Result<TcpStream>) -> StreamSetupAction + Send + Sync + 'static,
    {
        self.stream_setup_hook = Some(Box::new(f));
        self
    }

    pub fn pre_routing_hook<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(&mut Request<'_>, &mut ResponseHandle, &ConnectionMeta) -> PreRoutingAction
            + Send
            + Sync
            + 'static,
    {
        self.pre_routing_hook = Some(Box::new(f));
        self
    }

    pub fn fallback_route<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn(RequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static,
    {
        self.router.set_fallback_route(Box::new(f));
        self
    }

    pub fn max_request_head_size(&mut self, value: Option<usize>) -> &mut Self {
        self.max_request_head_size = value;
        self
    }

    pub fn max_request_header_count(&mut self, value: Option<usize>) -> &mut Self {
        self.max_request_header_count = value;
        self
    }

    pub fn build(self) -> Server<Router<Box<RouteFn>>> {
        Server {
            bind_addrs: self.bind_addrs,
            thread_count: self.thread_count.unwrap_or(DEFAULT_THREAD_COUNT),
            stream_setup_hook: self.stream_setup_hook,
            handler_config: Arc::new(HandlerConfig {
                router: self.router.build(),
                pre_routing_hook: self.pre_routing_hook,
                max_request_head: self
                    .max_request_head_size
                    .unwrap_or(DEFAULT_MAX_REQUEST_HEAD),
            }),
        }
    }

    pub fn build_with_router<R>(self, router: R) -> Server<R>
    where
        R: HttpRouter,
    {
        Server {
            bind_addrs: self.bind_addrs,
            thread_count: self.thread_count.unwrap_or(DEFAULT_THREAD_COUNT),
            stream_setup_hook: self.stream_setup_hook,
            handler_config: Arc::new(HandlerConfig {
                router,
                pre_routing_hook: self.pre_routing_hook,
                max_request_head: self
                    .max_request_head_size
                    .unwrap_or(DEFAULT_MAX_REQUEST_HEAD),
            }),
        }
    }
}

impl<R> Server<R>
where
    R: HttpRouter<Route = Box<RouteFn>> + Send + Sync + 'static,
{
    pub fn bind_addrs(&self) -> &Vec<SocketAddr> {
        &self.bind_addrs
    }

    pub fn serve(self) -> io::Result<()> {
        let listener = TcpListener::bind(&*self.bind_addrs)?;
        let pool = ThreadPool::new(self.thread_count);

        for stream in listener.incoming() {
            let stream = match &self.stream_setup_hook {
                Some(hook) => match (hook)(stream) {
                    StreamSetupAction::Proceed(s) => s,
                    StreamSetupAction::Drop => continue,
                    StreamSetupAction::StopAccepting => break,
                },
                None => match stream {
                    Ok(s) => s,
                    Err(_) => continue,
                },
            };

            let config = self.handler_config.clone();

            pool.execute(move || {
                let _ = handle_connection(stream, config);
            });
        }
        Ok(())
    }

    pub fn handle(&self, stream: TcpStream) -> io::Result<()> {
        handle_connection(stream, self.handler_config.clone())
    }
}

pub struct ResponseHandle<'r> {
    stream: &'r mut TcpStream,
    keep_alive: bool,
}

impl ResponseHandle<'_> {
    pub fn ok(&mut self, headers: &Headers, body: impl Read) -> io::Result<()> {
        self.send(&Status::OK, headers, body)
    }

    pub fn send(&mut self, status: &Status, headers: &Headers, body: impl Read) -> io::Result<()> {
        if headers.is_connection_close() {
            self.keep_alive = false;
        }
        HttpPrinter::new(&mut self.stream).write_response(status, headers, body)
    }

    pub fn send_100_continue(&mut self) -> io::Result<()> {
        HttpPrinter::new(&mut self.stream).write_100_continue()
    }

    pub fn get_stream(&mut self) -> &TcpStream {
        self.stream
    }

    pub fn get_stream_mut(&mut self) -> &mut TcpStream {
        self.stream
    }
}

pub struct RequestContext<'r> {
    pub method: Method,
    pub uri: &'r RequestUri<'r>,
    pub headers: Headers<'r>,
    pub route_params: &'r RouteParams<'r, 'r>,
    pub http_version: u8,
    pub conn: &'r ConnectionMeta,
    body: BodyReader<'r, &'r mut TcpStream>,
}

impl<'r> RequestContext<'r> {
    pub fn body(&mut self) -> &mut BodyReader<'r, &'r mut TcpStream> {
        &mut self.body
    }

    pub fn get_stream(&self) -> &TcpStream {
        self.body.inner()
    }

    pub fn get_stream_mut(&mut self) -> &mut TcpStream {
        self.body.inner_mut()
    }

    pub fn into_parts(
        self,
    ) -> (
        Method,
        &'r RequestUri<'r>,
        Headers<'r>,
        &'r RouteParams<'r, 'r>,
        u8,
        &'r ConnectionMeta,
        BodyReader<'r, &'r mut TcpStream>,
    ) {
        (
            self.method,
            self.uri,
            self.headers,
            self.route_params,
            self.http_version,
            self.conn,
            self.body,
        )
    }
}

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

fn handle_connection<R>(mut stream: TcpStream, config: Arc<HandlerConfig<R>>) -> io::Result<()>
where
    R: HttpRouter<Route = Box<RouteFn>>,
{
    let mut connection_meta = ConnectionMeta::new();
    let mut write_stream = stream.try_clone()?;
    loop {
        connection_meta.increment();
        let keep_alive =
            handle_one_request(&mut stream, &mut write_stream, &config, &connection_meta)?;
        if !keep_alive {
            return Ok(());
        }
    }
}

// HACK: thread-local buffer to avoid allocs/zeroing per request.
// safe since each thread handles exactly one request at a time
fn with_uninit_buffer<'a, F>(max_size: usize, read_fn: F) -> io::Result<Option<&'a [u8]>>
where
    F: FnOnce(&mut [u8]) -> io::Result<usize>,
{
    const DEFAULT_REQUEST_BUFFER_SIZE: usize = 4096;
    thread_local! {
        static REQUEST_BUFFER: std::cell::RefCell<Vec<std::mem::MaybeUninit<u8>>> =
            std::cell::RefCell::new(Vec::with_capacity(DEFAULT_REQUEST_BUFFER_SIZE));
    }

    REQUEST_BUFFER.with(|cell| {
        let mut vec = cell.borrow_mut();

        if vec.len() != max_size {
            vec.resize_with(max_size, std::mem::MaybeUninit::uninit);
        }

        // SAFETY: We're only treating the slice as `[u8]` temporarily for writing.
        let buf = unsafe { std::slice::from_raw_parts_mut(vec.as_mut_ptr() as *mut u8, max_size) };

        let bytes_read = read_fn(buf)?;
        if bytes_read == 0 {
            return Ok(None); // EOF
        }

        // SAFETY: Only the first `bytes_read` bytes are guaranteed initialized
        let ret = unsafe { std::slice::from_raw_parts(vec.as_ptr() as *const u8, bytes_read) };

        Ok(Some(ret))
    })
}

/// returns whether to keep the stream alive for the next request
fn handle_one_request<R>(
    read_stream: &mut TcpStream,
    write_stream: &mut TcpStream,
    config: &HandlerConfig<R>,
    connection_meta: &ConnectionMeta,
) -> io::Result<bool>
where
    R: HttpRouter<Route = Box<RouteFn>>,
{
    let buf = match with_uninit_buffer(config.max_request_head, |buf| read_stream.read(buf))? {
        Some(b) => b,
        None => return Ok(false), // eof
    };
    let mut request = match Request::parse(buf) {
        Ok(x) => x,
        Err(_) => return Ok(false), // TODO: reply with 400?
    };

    let mut response = ResponseHandle {
        stream: write_stream,
        keep_alive: true,
    };

    if let Some(hook) = &config.pre_routing_hook {
        match (hook)(&mut request, &mut response, connection_meta) {
            PreRoutingAction::Proceed => {}
            PreRoutingAction::Drop => return Ok(response.keep_alive),
        }
    }

    let matched_route = config
        .router
        .match_route(&request.method, request.uri.path());

    let body = BodyReader::from_request(&buf[request.buf_offset..], read_stream, &request.headers);
    let ctx = RequestContext {
        method: request.method,
        headers: request.headers,
        uri: &request.uri,
        http_version: request.http_version,
        route_params: &matched_route.params,
        conn: connection_meta,
        body,
    };

    let client_requested_close = ctx.headers.is_connection_close();
    (matched_route.route)(ctx, &mut response)?;

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
            EPOLL_CTL_ADD, EPOLL_CTL_DEL, EPOLL_CTL_MOD, EPOLLET, EPOLLIN, EPOLLONESHOT,
            epoll_create1, epoll_ctl, epoll_event, epoll_wait,
        };
        use std::os::unix::io::{AsRawFd, RawFd};
        use std::ptr::null_mut;
        use std::sync::atomic::{AtomicPtr, Ordering};

        let listener = TcpListener::bind(&*self.bind_addrs)?;
        listener.set_nonblocking(true)?;

        let epfd = unsafe { epoll_create1(0) };
        if epfd == -1 {
            return Err(io::Error::last_os_error());
        }

        let listener_fd = listener.as_raw_fd();
        let mut ev = epoll_event {
            events: (EPOLLIN | EPOLLET) as u32,
            u64: listener_fd as u64,
        };
        if unsafe { epoll_ctl(epfd, EPOLL_CTL_ADD, listener_fd, &mut ev) } == -1 {
            return Err(io::Error::last_os_error());
        }

        struct Connection(TcpStream, TcpStream, ConnectionMeta);

        const MAX_FDS: usize = 1024; // TODO: should equal rlim_cur
        let connections: Arc<[AtomicPtr<Connection>]> = Arc::from(
            (0..MAX_FDS)
                .map(|_| AtomicPtr::new(null_mut()))
                .collect::<Vec<_>>(),
        );

        let pool = ThreadPool::new(self.thread_count);
        let mut events = vec![epoll_event { events: 0, u64: 0 }; 1024];

        loop {
            let n = unsafe { epoll_wait(epfd, events.as_mut_ptr(), 1024, -1) };
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
                                        StreamSetupAction::Proceed(s) => s,
                                        StreamSetupAction::Drop => continue,
                                        StreamSetupAction::StopAccepting => return Ok(()),
                                    }
                                }

                                let client_fd = stream.as_raw_fd();
                                if client_fd < 0 || client_fd as usize >= MAX_FDS {
                                    continue;
                                }

                                let mut ev = epoll_event {
                                    events: (EPOLLIN | EPOLLONESHOT | EPOLLET) as u32,
                                    u64: client_fd as u64,
                                };

                                if unsafe { epoll_ctl(epfd, EPOLL_CTL_ADD, client_fd, &mut ev) }
                                    == -1
                                {
                                    continue;
                                }
                                let write_stream = stream.try_clone().unwrap();

                                let conn = Box::new(Connection(
                                    stream,
                                    write_stream,
                                    ConnectionMeta::new(),
                                ));
                                let ptr = Box::into_raw(conn);
                                connections[client_fd as usize].store(ptr, Ordering::SeqCst);
                            }
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
                            Err(_) => break,
                        }
                    }
                } else {
                    if fd < 0 || fd as usize >= MAX_FDS {
                        continue;
                    }

                    let config = Arc::clone(&self.handler_config);
                    let connections = Arc::clone(&connections);

                    pool.execute(move || {
                        let conn_ptr = connections[fd as usize].swap(null_mut(), Ordering::SeqCst);
                        if conn_ptr.is_null() {
                            return;
                        }

                        // SAFETY: pointer is uniquely owned
                        let mut conn = unsafe { Box::from_raw(conn_ptr) };
                        conn.2.increment();

                        // SAFETY: (danger?)
                        // if handle_one_request panics, then EPOLL_CTL_DEL is not called -> test/document
                        let keep_alive =
                            handle_one_request(&mut conn.0, &mut conn.1, &config, &conn.2)
                                .unwrap_or(false);

                        if keep_alive {
                            let ptr = Box::into_raw(conn);
                            connections[fd as usize].store(ptr, Ordering::SeqCst);

                            let mut ev = epoll_event {
                                events: (EPOLLIN | EPOLLONESHOT | EPOLLET) as u32,
                                u64: fd as u64,
                            };
                            unsafe {
                                epoll_ctl(epfd, EPOLL_CTL_MOD, fd, &mut ev);
                            }
                        } else {
                            unsafe {
                                epoll_ctl(epfd, EPOLL_CTL_DEL, fd, null_mut());
                            }
                            // SAFETY: conn pointer is deallocated
                        }
                    });
                }
            }
        }
    }
}
