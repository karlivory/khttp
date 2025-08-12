use crate::parser::Request;
use crate::router::RouteParams;
use crate::threadpool::ThreadPool;
use crate::{BodyReader, Headers, HttpPrinter, HttpRouter, Method, RequestUri, Router, Status};
use std::io::{self, Read};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Instant;

mod builder;
mod epoll;
pub use builder::ServerBuilder;

pub type RouteFn = dyn Fn(RequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync;
pub type StreamSetupFn = dyn Fn(io::Result<TcpStream>) -> StreamSetupAction + Send + Sync;
pub type PreRoutingHookFn = dyn Fn(&mut Request<'_>, &mut ResponseHandle, &ConnectionMeta) -> PreRoutingAction
    + Send
    + Sync;

struct HandlerConfig<R> {
    router: R,
    pre_routing_hook: Option<Box<PreRoutingHookFn>>,
    max_request_head: usize,
}

pub struct Server<R> {
    bind_addrs: Vec<SocketAddr>,
    thread_count: usize,
    stream_setup_hook: Option<Box<StreamSetupFn>>,
    handler_config: Arc<HandlerConfig<R>>,
    epoll_queue_max_events: usize,
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

impl Server<Router<Box<RouteFn>>> {
    pub fn builder<A: ToSocketAddrs>(addr: A) -> io::Result<ServerBuilder> {
        ServerBuilder::new(addr)
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

pub struct ResponseHandle {
    stream: TcpStream,
    keep_alive: bool,
}

impl ResponseHandle {
    fn new(stream: TcpStream) -> Self {
        ResponseHandle {
            stream,
            keep_alive: true,
        }
    }

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
        &self.stream
    }

    pub fn get_stream_mut(&mut self) -> &mut TcpStream {
        &mut self.stream
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
    let mut conn_meta = ConnectionMeta::new();
    let write_stream = stream.try_clone()?;
    let mut response = ResponseHandle {
        stream: write_stream,
        keep_alive: true,
    };

    loop {
        conn_meta.increment();
        let keep_alive = handle_one_request(&mut stream, &mut response, &config, &conn_meta)?;
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

/// Returns "keep-alive" (whether to keep the connection alive for the next request).
fn handle_one_request<R>(
    read_stream: &mut TcpStream,
    response: &mut ResponseHandle,
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
    (matched_route.route)(ctx, response)?;
    if client_requested_close {
        return Ok(false);
    }
    Ok(response.keep_alive)
}
