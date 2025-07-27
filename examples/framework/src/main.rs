use khttp::common::{Headers, Status};
use khttp_framework::{FrameworkApp, ServerConfig};
use std::sync::Arc;

fn main() {
    let config = ServerConfig {
        port: 8080,
        bind: "0.0.0.0".to_string(),
        thread_count: 20,
        verbose: false,
        tcp_read_timeout: None,
        tcp_write_timeout: None,
        tcp_nodelay: false,
    };

    let mut app = FrameworkApp::new(config);
    add_routes(&mut app);
    app.serve().unwrap();
}

fn add_routes(app: &mut FrameworkApp) {
    let db_creds = Arc::new(DbCredentials {
        connection_string: "postgresql://user:pass@localhost/db".to_string(),
    });
    let logger = Arc::new(Logger);

    app.get("/api/db/call")
        .middleware(middlewares::panic_unwind())
        .inject(logger.clone())
        .middleware(middlewares::logger())
        .inject(db_creds.clone())
        .middleware(middlewares::auth("secret".to_string()))
        .handle(|ctx, res| {
            let db = ctx.get::<Arc<DbCredentials>>().unwrap();
            let log = ctx.get::<Arc<Logger>>().unwrap();
            log.info("Connecting to db...");
            res.ok(
                Headers::new(),
                format!("db = {}\n", db.connection_string).as_bytes(),
            )
        });

    app.get("/api/user/:id")
        .middleware(middlewares::panic_unwind())
        .inject(logger.clone())
        .middleware(middlewares::logger())
        .inject(db_creds.clone())
        .handle(|ctx, res| {
            let log = ctx.get::<Arc<Logger>>().unwrap();

            let user_id = match ctx
                .request
                .route_params
                .get("id")
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(id) => id,
                None => {
                    log.warn("Invalid or missing user id");
                    return res.send(
                        &Status::BAD_REQUEST,
                        Headers::new(),
                        "invalid user id".as_bytes(),
                    );
                }
            };

            if user_id == 0 {
                log.err("Something went very wrong!");
                panic!();
            }

            res.ok(Headers::new(), format!("user: {}", user_id).as_bytes())
        });
}

// ---------------------------------------------------------------------
// SERVICES
// ---------------------------------------------------------------------

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
        println!("[INFO] {msg}");
    }
    pub fn warn(&self, msg: &str) {
        println!("[WARN] {msg}");
    }
    pub fn err(&self, msg: &str) {
        println!("[ERROR] {msg}");
    }
}

// ---------------------------------------------------------------------
// MIDDLEWARES
// ---------------------------------------------------------------------

mod middlewares {
    use crate::Logger;
    use khttp::common::{Headers, Status};
    use khttp_framework::Handler;
    use std::sync::Arc;

    pub fn auth(secret: String) -> impl Fn(Box<Handler>) -> Box<Handler> + Send + Sync {
        move |next| {
            let secret = secret.clone();
            Box::new(move |ctx, res| {
                if ctx.request.headers.get("authorization") == Some(&secret) {
                    next(ctx, res)
                } else {
                    res.send(&Status::of(401), Headers::new(), &b"Unauthorized"[..])
                }
            })
        }
    }

    pub fn logger() -> impl Fn(Box<Handler>) -> Box<Handler> + Send + Sync {
        |next| {
            Box::new(move |ctx, res| {
                let ip = ctx
                    .request
                    .get_stream()
                    .peer_addr()
                    .map(|x| x.ip().to_string())
                    .unwrap_or("<unknown>".to_string());

                let log = ctx.get::<Arc<Logger>>().unwrap();
                log.info(&format!(
                    "[ip: {}] {} {}",
                    ip,
                    ctx.request.method,
                    ctx.request.uri.as_str()
                ));
                next(ctx, res)
            })
        }
    }

    pub fn panic_unwind() -> impl Fn(Box<Handler>) -> Box<Handler> + Send + Sync {
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
                    res.send(
                        &Status::of(500),
                        Headers::new(),
                        &b"Internal Server Error"[..],
                    )
                } else {
                    Ok(())
                }
            })
        }
    }
}
