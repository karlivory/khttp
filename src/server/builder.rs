use crate::parser::Request;
use crate::router::RouterBuilder;
use crate::{Headers, HttpRouter, Method, Router, Status};
use std::io::{self};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;

use super::{
    ConnectionMeta, HandlerConfig, PreRoutingAction, PreRoutingHookFn, RequestContext,
    ResponseHandle, RouteFn, Server, StreamSetupAction, StreamSetupFn,
};

const DEFAULT_THREAD_COUNT: usize = 20;
const DEFAULT_MAX_REQUEST_HEAD: usize = 8192;
const DEFAULT_EPOLL_QUEUE_MAXEVENTS: usize = 1024;

pub struct ServerBuilder {
    bind_addrs: Vec<SocketAddr>,
    router: RouterBuilder<Box<RouteFn>>,
    stream_setup_hook: Option<Box<StreamSetupFn>>,
    pre_routing_hook: Option<Box<PreRoutingHookFn>>,
    thread_count: Option<usize>,
    max_request_head_size: Option<usize>,
    max_request_header_count: Option<usize>,
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
                r.send(&Status::NOT_FOUND, Headers::empty(), std::io::empty())
            })),
            thread_count: None,
            stream_setup_hook: None,
            pre_routing_hook: None,
            max_request_head_size: None,
            max_request_header_count: None,
            epoll_queue_max_events: DEFAULT_EPOLL_QUEUE_MAXEVENTS,
        })
    }

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

    pub fn epoll_queue_max_events(&mut self, value: usize) -> &mut Self {
        self.epoll_queue_max_events = value;
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
            epoll_queue_max_events: self.epoll_queue_max_events,
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
            epoll_queue_max_events: self.epoll_queue_max_events,
        }
    }
}
