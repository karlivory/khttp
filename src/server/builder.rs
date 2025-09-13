use super::{
    ConnectionMeta, HandlerConfig, PreRoutingAction, PreRoutingHookFn, RequestContext,
    ResponseHandle, RouteFn, Server, StreamSetupAction, StreamSetupFn,
};
use crate::parser::Request;
use crate::router::RouterBuilder;
use crate::{Headers, Method, Status};
use std::io::{self};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;

const DEFAULT_MAX_REQUEST_HEAD: usize = 4096; // should be plenty, this is what nginx uses by default
const DEFAULT_EPOLL_QUEUE_MAXEVENTS: usize = 512;

pub struct ServerBuilder {
    bind_addrs: Vec<SocketAddr>,
    router: RouterBuilder<Box<RouteFn>>,
    stream_setup_hook: Option<Box<StreamSetupFn>>,
    pre_routing_hook: Option<Box<PreRoutingHookFn>>,
    thread_count: usize,
    max_request_head_size: usize,
    epoll_queue_max_events: usize,
}

impl ServerBuilder {
    pub fn new<A: ToSocketAddrs>(addr: A) -> io::Result<ServerBuilder> {
        let bind_addrs: Vec<SocketAddr> = addr.to_socket_addrs()?.collect();

        if bind_addrs.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid address",
            ));
        }

        Ok(ServerBuilder {
            bind_addrs,
            router: RouterBuilder::new(Box::new(|_, r| {
                r.send0(&Status::NOT_FOUND, Headers::empty())
            })),
            stream_setup_hook: None,
            pre_routing_hook: None,
            thread_count: get_default_thread_count(),
            max_request_head_size: DEFAULT_MAX_REQUEST_HEAD,
            epoll_queue_max_events: DEFAULT_EPOLL_QUEUE_MAXEVENTS,
        })
    }

    pub fn build(self) -> Server {
        Server {
            bind_addrs: self.bind_addrs,
            thread_count: self.thread_count,
            stream_setup_hook: self.stream_setup_hook,
            handler_config: Arc::new(HandlerConfig {
                router: self.router.build(),
                pre_routing_hook: self.pre_routing_hook,
                max_request_head: self.max_request_head_size,
            }),
            epoll_queue_max_events: self.epoll_queue_max_events,
        }
    }

    pub fn route<F>(&mut self, method: Method, path: &str, route_fn: F) -> &mut Self
    where
        F: Fn(RequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static,
    {
        self.router.add_route(&method, path, Box::new(route_fn));
        self
    }

    pub fn thread_count(&mut self, thread_count: usize) -> &mut Self {
        self.thread_count = thread_count;
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

    pub fn max_request_head_size(&mut self, value: usize) -> &mut Self {
        self.max_request_head_size = value;
        self
    }

    pub fn epoll_queue_max_events(&mut self, value: usize) -> &mut Self {
        self.epoll_queue_max_events = value;
        self
    }
}

fn get_default_thread_count() -> usize {
    const FALLBACK_THREAD_COUNT: usize = 16;
    match std::thread::available_parallelism() {
        Ok(x) => 10.max(x.get() * 2),
        Err(_) => FALLBACK_THREAD_COUNT,
    }
}
