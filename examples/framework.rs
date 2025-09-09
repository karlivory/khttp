use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use khttp::{
    ConnectionMeta, Headers, Method, PreRoutingAction, Request, RequestContext, ResponseHandle,
    Server, ServerBuilder, Status,
};

fn main() {
    let mut app = Server::builder("0.0.0.0:8080").unwrap();

    // Custom configuration
    app.with_trailing_slash_redirect();

    // Sample "services"
    let db = Arc::new(DbCredentials {
        connection_string: "postgresql://user:pass@localhost/db".into(),
    });
    let logger = Arc::new(Logger);

    // Reusable middleware chain used by multiple routes
    let base_layer = Chain::new()
        .middleware(middlewares::panic_unwind())
        .inject(logger.clone())
        .middleware(middlewares::logger());

    app.get("/health")
        .with(&base_layer)
        .handle(|_, r| r.ok(Headers::empty(), "ok"));

    app.get("/api/user/:id")
        .with(&base_layer)
        .middleware(middlewares::auth("user-secret"))
        .handle(|ctx, res| {
            let log = ctx.get::<Arc<Logger>>().unwrap();
            let user_id = match ctx
                .request
                .params
                .get("id")
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(id) => id,
                None => {
                    log.warn("Invalid user id");
                    return res.send(&Status::BAD_REQUEST, Headers::empty(), "bad id");
                }
            };

            if user_id == 0 {
                log.err("Simulated panic for id=0");
                panic!("boom");
            }
            res.ok(Headers::empty(), format!("user: {}\n", user_id).as_bytes())
        });

    app.post("/api/db/call")
        .with(&base_layer)
        .middleware(middlewares::auth("db-secret"))
        .inject(db.clone())
        .handle(|ctx, res| {
            let db = ctx.get::<Arc<DbCredentials>>().unwrap();
            let log = ctx.get::<Arc<Logger>>().unwrap();

            log.info("Connecting to DB...");
            let result = format!("db = {}\n", db.connection_string);
            res.ok(Headers::empty(), result.as_bytes())
        });

    app.serve().unwrap();
}

// -------------------------------------------------------------------------
// framework lib
// -------------------------------------------------------------------------

pub trait ServerBuilderExt {
    fn route(&mut self, method: Method, path: &'static str) -> RouteBuilder<'_>;
    fn get(&mut self, path: &'static str) -> RouteBuilder<'_> {
        self.route(Method::Get, path)
    }
    fn post(&mut self, path: &'static str) -> RouteBuilder<'_> {
        self.route(Method::Post, path)
    }
    fn with_trailing_slash_redirect(&mut self) -> &mut Self;
    fn serve(self) -> io::Result<()>;
}

impl ServerBuilderExt for ServerBuilder {
    fn route(&mut self, method: Method, path: &'static str) -> RouteBuilder<'_> {
        RouteBuilder {
            app: self,
            method,
            path,
            middleware: Vec::new(),
        }
    }

    /// Redirects "/foo/" -> "/foo"
    fn with_trailing_slash_redirect(&mut self) -> &mut Self {
        self.pre_routing_hook(trailing_slash_redirect());
        self
    }

    fn serve(self) -> io::Result<()> {
        let app = self.build();
        print_startup_banner(
            &app.bind_addrs().first().unwrap().to_string(),
            app.threads(),
        );
        app.serve_epoll()
    }
}

pub struct HandlerContext<'r> {
    pub request: RequestContext<'r>,
    pub extensions: HashMap<TypeId, Box<dyn Any>>,
}

impl<'r> HandlerContext<'r> {
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.extensions
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
    }

    pub fn insert<T: 'static>(&mut self, val: T) {
        self.extensions.insert(TypeId::of::<T>(), Box::new(val));
    }
}

pub type Handler = dyn Fn(HandlerContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync;
pub type MiddlewareFn = dyn Fn(Box<Handler>) -> Box<Handler>;

#[derive(Default)]
pub struct Chain {
    mws: Vec<Arc<MiddlewareFn>>,
}

impl Chain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn middleware<F>(mut self, mw: F) -> Self
    where
        F: Fn(Box<Handler>) -> Box<Handler> + 'static,
    {
        self.mws.push(Arc::new(mw));
        self
    }

    pub fn inject<T>(self, val: T) -> Self
    where
        T: 'static + Send + Sync + Clone,
    {
        self.middleware(inject(val))
    }

    fn extend_into(&self, v: &mut Vec<Arc<MiddlewareFn>>) {
        v.extend(self.mws.iter().cloned());
    }
}

pub struct RouteBuilder<'a> {
    app: &'a mut ServerBuilder,
    method: Method,
    path: &'static str,
    middleware: Vec<Arc<MiddlewareFn>>,
}

impl<'a> RouteBuilder<'a> {
    pub fn middleware<F>(mut self, mw: F) -> Self
    where
        F: Fn(Box<Handler>) -> Box<Handler> + Send + Sync + 'static,
    {
        self.middleware.push(Arc::new(mw));
        self
    }

    pub fn inject<T>(self, val: T) -> Self
    where
        T: 'static + Send + Sync + Clone,
    {
        self.middleware(inject(val))
    }

    pub fn with(mut self, chain: &Chain) -> Self {
        chain.extend_into(&mut self.middleware);
        self
    }

    pub fn handle<F>(self, handler: F)
    where
        F: Fn(HandlerContext, &mut ResponseHandle) -> io::Result<()> + Send + Sync + 'static,
    {
        let mut next: Box<Handler> = Box::new(handler);
        for mw in self.middleware.into_iter().rev() {
            next = (mw)(next);
        }

        let route = move |ctx: RequestContext, res: &mut ResponseHandle| {
            let ctx = HandlerContext {
                request: ctx,
                extensions: HashMap::new(),
            };
            next(ctx, res)
        };

        self.app.route(self.method, self.path, route);
    }
}

// Helper middleware: inject a value into the HandlerContext
fn inject<T>(val: T) -> impl Fn(Box<Handler>) -> Box<Handler>
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

mod middlewares {
    use super::*;

    pub fn auth(secret: &'static str) -> impl Fn(Box<Handler>) -> Box<Handler> {
        move |next| {
            Box::new(move |ctx, res| {
                if ctx.request.headers.get("authorization") == Some(secret.as_bytes()) {
                    next(ctx, res)
                } else {
                    if let Some(log) = ctx.get::<Arc<Logger>>() {
                        log.warn("blocked unauthorized request");
                    }
                    res.send(&Status::of(401), Headers::empty(), b"unauthorized")
                }
            })
        }
    }

    pub fn logger() -> impl Fn(Box<Handler>) -> Box<Handler> {
        |next| {
            Box::new(move |ctx, res| {
                let ip = ctx
                    .request
                    .get_stream()
                    .peer_addr()
                    .map(|x| x.ip().to_string())
                    .unwrap_or_else(|_| "<unknown>".into());

                if let Some(log) = ctx.get::<Arc<Logger>>() {
                    log.info(&format!(
                        "[ip: {}] {} {}",
                        ip,
                        ctx.request.method,
                        ctx.request.uri.as_str()
                    ));
                }
                next(ctx, res)
            })
        }
    }

    pub fn panic_unwind() -> impl Fn(Box<Handler>) -> Box<Handler> {
        |next| {
            Box::new(move |ctx, res| {
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| next(ctx, res)));

                if let Err(panic_info) = result {
                    let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                        *s
                    } else if let Some(s) = panic_info.downcast_ref::<String>() {
                        s.as_str()
                    } else {
                        "unknown panic"
                    };

                    eprintln!("[panic] handler panicked: {msg}");
                    res.send(&Status::of(500), Headers::empty(), b"internal error")
                } else {
                    Ok(())
                }
            })
        }
    }
}

fn trailing_slash_redirect(
) -> impl Fn(&mut Request<'_>, &mut ResponseHandle, &ConnectionMeta) -> PreRoutingAction {
    move |request, response, _| {
        let original_path = request.uri.path();
        if original_path != "/" && original_path.ends_with('/') {
            let trimmed = original_path.trim_end_matches('/');
            let mut headers = Headers::new();
            headers.replace("Location", trimmed.as_bytes());
            let _ = response.send0(&Status::of(301), &headers);
            return PreRoutingAction::Drop;
        }
        PreRoutingAction::Proceed
    }
}

fn print_startup_banner(addr: &str, threads: usize) {
    println!(
        r#"
   _  ___    _ _______ _______ _____
  | |/ / |  | |__   __|__   __|  __ \
  | ' /| |__| |  | |     | |  | |__) |
  |  < |  __  |  | |     | |  |  ___/
  | . \| |  | |  | |     | |  | |
  |_|\_\_|  |_|  |_|     |_|  |_|

 KHTTP: minimal HTTP/1.1 server framework
 Worker threads: {threads}
 Listening on:   http://{addr}
────────────────────────────────────────────
"#
    );
}

pub struct DbCredentials {
    pub connection_string: String,
}

pub struct Logger;
impl Logger {
    pub fn trace(&self, msg: &str) {
        println!("[TRACE] {msg}");
    }
    pub fn debug(&self, msg: &str) {
        println!("[DEBUG] {msg}");
    }
    pub fn info(&self, msg: &str) {
        println!("[INFO]  {msg}");
    }
    pub fn warn(&self, msg: &str) {
        println!("[WARN]  {msg}");
    }
    pub fn err(&self, msg: &str) {
        eprintln!("[ERROR] {msg}");
    }
}
