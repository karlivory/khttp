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

mod builder;
mod epoll;
pub use builder::ServerBuilder;

pub type RouteFn = dyn for<'req, 's> Fn(RequestContext<'req>, &mut ResponseHandle<'s>) -> io::Result<()>
    + Send
    + Sync;

pub type StreamSetupFn = dyn Fn(io::Result<TcpStream>) -> StreamSetupAction + Send + Sync;

pub type PreRoutingHookFn = dyn for<'req, 's> Fn(&mut Request<'req>, &mut ResponseHandle<'s>) -> PreRoutingAction
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

    pub fn serve_threaded(self) -> io::Result<()> {
        let listener = TcpListener::bind(&*self.bind_addrs)?;

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
            let config = Arc::clone(&self.handler_config);

            std::thread::spawn(|| {
                let _ = handle_connection(stream, config);
            });
        }
        Ok(())
    }

    pub fn handle(&self, stream: TcpStream) -> io::Result<()> {
        handle_connection(stream, self.handler_config.clone())
    }
}

pub struct ResponseHandle<'s> {
    stream: &'s TcpStream,
    keep_alive: bool,
}

impl<'s> ResponseHandle<'s> {
    fn new(stream: &'s TcpStream) -> Self {
        ResponseHandle {
            stream,
            keep_alive: true,
        }
    }

    pub fn ok<B: AsRef<[u8]>>(&mut self, headers: &Headers, body: B) -> io::Result<()> {
        self.send(&Status::OK, headers, body)
    }

    pub fn send<B: AsRef<[u8]>>(
        &mut self,
        status: &Status,
        headers: &Headers,
        body: B,
    ) -> io::Result<()> {
        if headers.is_connection_close() {
            self.keep_alive = false;
        }
        HttpPrinter::write_response_bytes(self.stream, status, headers, body.as_ref())
    }

    pub fn ok0(&mut self, headers: &Headers) -> io::Result<()> {
        self.send0(&Status::OK, headers)
    }

    pub fn send0(&mut self, status: &Status, headers: &Headers) -> io::Result<()> {
        if headers.is_connection_close() {
            self.keep_alive = false;
        }
        HttpPrinter::write_response_empty(self.stream, status, headers)
    }

    pub fn okr<R: Read>(&mut self, headers: &Headers, body: R) -> io::Result<()> {
        self.sendr(&Status::OK, headers, body)
    }

    pub fn sendr<R: Read>(
        &mut self,
        status: &Status,
        headers: &Headers,
        body: R,
    ) -> io::Result<()> {
        if headers.is_connection_close() {
            self.keep_alive = false;
        }
        HttpPrinter::write_response(self.stream, status, headers, body)
    }

    pub fn send_100_continue(&mut self) -> io::Result<()> {
        HttpPrinter::write_100_continue(self.stream)
    }

    pub fn send_417_expectation_failed(&mut self) -> io::Result<()> {
        HttpPrinter::write_417_expectation_failed(self.stream)
    }

    pub fn get_stream(&self) -> &TcpStream {
        self.stream
    }
}

pub struct RequestContext<'r> {
    pub method: Method,
    pub uri: &'r RequestUri<'r>,
    pub headers: Headers<'r>,
    pub params: &'r RouteParams<'r, 'r>,
    pub http_version: u8,
    body: BodyReader<'r, &'r TcpStream>,
}

impl<'r> RequestContext<'r> {
    pub fn body(&mut self) -> &mut BodyReader<'r, &'r TcpStream> {
        &mut self.body
    }

    pub fn get_stream(&self) -> &TcpStream {
        self.body.inner()
    }

    pub fn into_parts(
        self,
    ) -> (
        Method,
        &'r RequestUri<'r>,
        Headers<'r>,
        &'r RouteParams<'r, 'r>,
        u8,
        BodyReader<'r, &'r TcpStream>,
    ) {
        (
            self.method,
            self.uri,
            self.headers,
            self.params,
            self.http_version,
            self.body,
        )
    }
}

fn handle_connection(stream: TcpStream, config: Arc<HandlerConfig>) -> io::Result<()> {
    let mut response = ResponseHandle::new(&stream);

    loop {
        let keep_alive = handle_one_request(&stream, &mut response, &config)?;
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
    mut stream: &TcpStream,
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
fn handle_one_request<'s>(
    stream: &TcpStream,
    response: &mut ResponseHandle<'s>,
    config: &HandlerConfig,
) -> io::Result<bool> {
    let (buf, mut request) = match read_request(stream, config.max_request_head) {
        Ok((buf, req)) => (buf, req),
        Err(ReadRequestError::InvalidRequestHead) => {
            response.send0(&Status::BAD_REQUEST, Headers::close())?;
            return Ok(false);
        }
        Err(ReadRequestError::RequestHeadTooLarge) => {
            response.send0(&Status::of(431), Headers::close())?;
            return Ok(false);
        }
        Err(_) => return Ok(false), // silently drop connection on eof / io-error
    };

    if let Some(hook) = &config.pre_routing_hook {
        match (hook)(&mut request, response) {
            PreRoutingAction::Proceed => {}
            PreRoutingAction::Drop => return Ok(response.keep_alive),
        }
    }

    let matched_route = config
        .router
        .match_route(&request.method, request.uri.path());

    let body = BodyReader::from_request(&buf[request.buf_offset..], stream, &request.headers);
    let ctx = RequestContext {
        method: request.method,
        headers: request.headers,
        uri: &request.uri,
        http_version: request.http_version,
        params: &matched_route.params,
        body,
    };

    let client_requested_close = ctx.headers.is_connection_close();
    (matched_route.route)(ctx, response)?;
    if client_requested_close {
        return Ok(false);
    }
    Ok(response.keep_alive)
}
