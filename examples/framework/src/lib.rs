use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::io;
use std::net::TcpStream;
use std::time::Duration;

pub use khttp;
use khttp::Request;
use khttp::{
    ConnectionMeta, PreRoutingAction, RequestContext, ResponseHandle, Server, ServerBuilder,
    StreamSetupAction,
};
use khttp::{Headers, Method, Status};

pub struct ServerConfig {
    pub port: u16,
    pub bind: String,
    pub thread_count: usize,
    pub verbose: bool,
    pub tcp_read_timeout: Option<u64>,
    pub tcp_write_timeout: Option<u64>,
    pub tcp_nodelay: bool,
}

fn get_stream_setup_fn(
    config: &ServerConfig,
) -> impl Fn(io::Result<TcpStream>) -> StreamSetupAction + use<> {
    let read_timeout = config.tcp_read_timeout;
    let write_timeout = config.tcp_write_timeout;
    let tcp_nodelay = config.tcp_nodelay;

    move |s| {
        let s = match s {
            Ok(s) => s,
            Err(_) => return StreamSetupAction::Skip,
        };
        if let Some(timeout) = read_timeout {
            match s.set_read_timeout(Some(Duration::from_millis(timeout))) {
                Ok(_) => (),
                Err(_) => return StreamSetupAction::Skip,
            };
        }
        if let Some(timeout) = write_timeout {
            match s.set_write_timeout(Some(Duration::from_millis(timeout))) {
                Ok(_) => (),
                Err(_) => return StreamSetupAction::Skip,
            }
        }
        match s.set_nodelay(tcp_nodelay) {
            Ok(_) => (),
            Err(_) => return StreamSetupAction::Skip,
        }
        StreamSetupAction::Accept(s)
    }
}
fn trailing_slash_redirect()
-> impl Fn(&mut Request<'_>, &ConnectionMeta, &mut ResponseHandle) -> PreRoutingAction
+ Send
+ Sync
+ 'static {
    move |req, _, response| {
        let original_path = req.uri.path();
        if original_path != "/" && original_path.ends_with('/') {
            let trimmed = original_path.trim_end_matches('/');
            let mut headers = Headers::new();
            headers.replace("Location", trimmed.as_bytes());
            let _ = response.send(&Status::of(301), &headers, &[][..]);
            return PreRoutingAction::Skip;
        }

        PreRoutingAction::Proceed
    }
}

pub struct FrameworkApp {
    server: ServerBuilder,
    config: ServerConfig,
}

impl FrameworkApp {
    pub fn new(config: ServerConfig) -> Self {
        let ip = &config.bind;
        let port = &config.port;
        let mut server = Server::builder(format!("{ip}:{port}")).expect("err: invalid addr");
        // server.set_pre_routing_hook(trailing_slash_redirect());
        server.thread_count(config.thread_count);
        server.stream_setup_hook(get_stream_setup_fn(&config));
        Self { server, config }
    }

    pub fn serve(self) -> io::Result<()> {
        print_startup_banner(&self.config);
        self.server.build().serve_epoll()
    }

    pub fn get(&mut self, path: &'static str) -> RouteBuilderWithMeta<'_> {
        self.route(Method::Get, path)
    }

    pub fn post(&mut self, path: &'static str) -> RouteBuilderWithMeta<'_> {
        self.route(Method::Post, path)
    }

    pub fn route(&mut self, method: Method, path: &'static str) -> RouteBuilderWithMeta<'_> {
        RouteBuilderWithMeta {
            app: self,
            method,
            path,
            builder: RouteBuilder {
                middleware: Vec::new(),
            },
        }
    }
}

fn print_startup_banner(config: &ServerConfig) {
    println!(
        r#"
 _  ___    _ _______ _______ _____
| |/ / |  | |__   __|__   __|  __ \
| ' /| |__| |  | |     | |  | |__) |
|  < |  __  |  | |     | |  |  ___/
| . \| |  | |  | |     | |  | |
|_|\_\_|  |_|  |_|     |_|  |_|

 KHTTP :: Minimal HTTP/1.1 Server Framework
 Running on http://{}:{}
 Threads: {}
────────────────────────────────────────────
"#,
        config.bind, config.port, config.thread_count,
    );
}

// ─────────────────────────────────────────────────────────────
// Middleware Framework

pub struct HandlerContext<'r> {
    pub request: RequestContext<'r>,
    pub extensions: HashMap<TypeId, Box<dyn Any + Send>>,
}

impl HandlerContext<'_> {
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.extensions
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
    }

    pub fn insert<T: 'static + Send>(&mut self, val: T) {
        self.extensions.insert(TypeId::of::<T>(), Box::new(val));
    }
}

pub type Handler = dyn Fn(HandlerContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync;

pub type MiddlewareFn = dyn Fn(Box<Handler>) -> Box<Handler> + Send + Sync;

pub struct RouteBuilder {
    middleware: Vec<Box<MiddlewareFn>>,
}

pub struct RouteBuilderWithMeta<'a> {
    app: &'a mut FrameworkApp,
    method: Method,
    path: &'static str,
    builder: RouteBuilder,
}

impl RouteBuilderWithMeta<'_> {
    pub fn middleware<F>(mut self, mw: F) -> Self
    where
        F: Fn(Box<Handler>) -> Box<Handler> + Send + Sync + 'static,
    {
        self.builder.middleware.push(Box::new(mw));
        self
    }

    pub fn inject<T>(mut self, val: T) -> Self
    where
        T: 'static + Send + Sync + Clone,
    {
        self.builder.middleware.push(Box::new(inject(val)));
        self
    }

    pub fn handle<F>(self, handler: F)
    where
        F: Fn(HandlerContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static,
    {
        self.app
            .server
            .route(self.method, self.path, self.builder.handle(handler));
    }
}

impl RouteBuilder {
    pub fn handle<F>(
        self,
        handler: F,
    ) -> impl Fn(RequestContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync
    where
        F: Fn(HandlerContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static,
    {
        let mut next: Box<Handler> = Box::new(handler);
        for mw in self.middleware.into_iter().rev() {
            next = mw(next);
        }

        move |ctx, res| {
            let ctx = HandlerContext {
                request: ctx,
                extensions: HashMap::new(),
            };
            next(ctx, res)
        }
    }
}

pub fn inject<T>(val: T) -> impl Fn(Box<Handler>) -> Box<Handler> + Send + Sync
where
    T: 'static + Send + Sync + Clone,
{
    move |next| {
        Box::new({
            let val = val.clone();
            move |mut ctx, res| {
                ctx.insert(val.clone());
                next(ctx, res)
            }
        })
    }
}
