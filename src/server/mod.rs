use crate::parser::Request;
use crate::router::RouteParams;
use crate::threadpool::{Task, ThreadPool};
use crate::{
    BodyReader, Headers, HttpParsingError, HttpPrinter, Method, RequestUri, Router, Status,
};
use std::cell::RefCell;
use std::io::{self, Read};
use std::mem::MaybeUninit;
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

struct HandlerConfig {
    router: Router<Box<RouteFn>>,
    pre_routing_hook: Option<Box<PreRoutingHookFn>>,
    max_request_head: usize,
}

pub struct Server {
    bind_addrs: Vec<SocketAddr>,
    thread_count: usize,
    stream_setup_hook: Option<Box<StreamSetupFn>>,
    handler_config: Arc<HandlerConfig>,
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

impl Server {
    pub fn builder<A: ToSocketAddrs>(addr: A) -> io::Result<ServerBuilder> {
        ServerBuilder::new(addr)
    }
}

impl Server {
    pub fn bind_addrs(&self) -> &Vec<SocketAddr> {
        &self.bind_addrs
    }

    pub fn threads(&self) -> usize {
        self.thread_count
    }

    pub fn serve(self) -> io::Result<()> {
        struct PoolJob(TcpStream, Arc<HandlerConfig>);

        impl Task for PoolJob {
            #[inline]
            fn run(self) {
                let _ = handle_connection(self.0, self.1);
            }
        }
        let listener = TcpListener::bind(&*self.bind_addrs)?;
        let pool: ThreadPool<PoolJob> = ThreadPool::new(self.thread_count);

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

            pool.execute(PoolJob(stream, Arc::clone(&self.handler_config)));
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

    pub fn ok<R: Read>(&mut self, headers: &Headers, body: R) -> io::Result<()> {
        self.send(&Status::OK, headers, body)
    }

    pub fn ok0(&mut self, headers: &Headers) -> io::Result<()> {
        self.send0(&Status::OK, headers)
    }

    pub fn send<R: Read>(&mut self, status: &Status, headers: &Headers, body: R) -> io::Result<()> {
        if headers.is_connection_close() {
            self.keep_alive = false;
        }
        HttpPrinter::new(&mut self.stream).write_response(status, headers, body)
    }

    pub fn send0(&mut self, status: &Status, headers: &Headers) -> io::Result<()> {
        if headers.is_connection_close() {
            self.keep_alive = false;
        }
        HttpPrinter::new(&mut self.stream).write_response0(status, headers)
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
    pub params: &'r RouteParams<'r, 'r>,
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
            self.params,
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

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn conn_start(&self) -> &Instant {
        &self.conn_start
    }
}

fn handle_connection(mut stream: TcpStream, config: Arc<HandlerConfig>) -> io::Result<()> {
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

const DEFAULT_REQUEST_BUFFER_SIZE: usize = 4096;
thread_local! {
    static REQUEST_BUFFER: RefCell<Vec<MaybeUninit<u8>>> =
        RefCell::new(Vec::with_capacity(DEFAULT_REQUEST_BUFFER_SIZE));
}

/// Read request head into a thread-local uninitialized buffer and parse it.
/// Thread-local storage is used since each thread handles exactly one request at once.
fn read_request<'a>(
    stream: &mut TcpStream,
    max_size: usize,
) -> Result<(&'a [u8], Request<'a>), ReadRequestError> {
    use std::slice::{from_raw_parts, from_raw_parts_mut};
    use ReadRequestError::*;

    REQUEST_BUFFER.with(|cell| {
        let mut vec = cell.borrow_mut();

        if vec.len() != max_size {
            vec.resize_with(max_size, MaybeUninit::uninit);
        }

        let ptr = vec.as_mut_ptr() as *mut u8;
        let mut filled = 0;

        loop {
            if filled == max_size {
                return Err(RequestHeadTooLarge);
            }

            // SAFETY: ptr.add(filled) is within bounds; read() will init this tail region
            let tail = unsafe { from_raw_parts_mut(ptr.add(filled), max_size - filled) };

            let n = match stream.read(tail) {
                Ok(0) => return Err(ReadEof),
                Ok(n) => n,
                Err(_) => return Err(IOError),
            };
            filled += n;

            // SAFETY: only the prefix [..filled] has been written (initialized) by read()
            let buf = unsafe { from_raw_parts(ptr as *const u8, filled) };

            match Request::parse(buf) {
                Ok(req) => return Ok((buf, req)),
                Err(HttpParsingError::UnexpectedEof) => continue, // need more bytes, keep reading
                Err(_) => return Err(InvalidRequestHead),         // malformed request head
            }
        }
    })
}

enum ReadRequestError {
    RequestHeadTooLarge,
    InvalidRequestHead,
    ReadEof,
    IOError,
}

/// Returns "keep-alive" (whether to keep the connection alive for the next request).
fn handle_one_request(
    read_stream: &mut TcpStream,
    response: &mut ResponseHandle,
    config: &HandlerConfig,
    connection_meta: &ConnectionMeta,
) -> io::Result<bool> {
    let (buf, mut request) = match read_request(read_stream, config.max_request_head) {
        Ok((buf, req)) => (buf, req),
        Err(ReadRequestError::InvalidRequestHead) => {
            response.send(&Status::BAD_REQUEST, Headers::close(), std::io::empty())?;
            return Ok(false);
        }
        Err(ReadRequestError::RequestHeadTooLarge) => {
            response.send(&Status::of(431), Headers::close(), std::io::empty())?;
            return Ok(false);
        }
        Err(_) => return Ok(false), // silently drop connection on eof / io-error
    };

    if let Some(hook) = &config.pre_routing_hook {
        match (hook)(&mut request, response, connection_meta) {
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
        params: &matched_route.params,
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
