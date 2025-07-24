use std::panic::UnwindSafe;
use std::thread;
use std::time::Duration;

use crate::args_parser::{ServerConfig, ServerOp};
use khttp::common::{HttpHeaders, HttpMethod, HttpStatus};
use khttp::router::DefaultRouter;
use khttp::server::{App, HttpRequestContext, HttpServer, ResponseHandle, RouteFn};

pub fn run(op: ServerOp) {
    match op {
        ServerOp::Echo(config) => run_echo_server(config),
        ServerOp::Sleep(config) => run_sleep_server(config),
    }
}

fn run_echo_server(config: ServerConfig) {
    let mut app = get_app(config);
    app.map_route(
        HttpMethod::Post,
        "/**",
        recover(|mut ctx, res| {
            res.ok(ctx.headers.clone(), ctx.get_body_reader());
        }),
    );
    app.serve().unwrap();
}

fn run_sleep_server(config: ServerConfig) {
    let mut app = get_app(config);
    app.map_route(
        HttpMethod::Get,
        "/sleep",
        recover(|ctx, res| {
            thread::sleep(Duration::from_secs(3));
            res.ok(ctx.headers, &[][..]);
        }),
    );
    app.serve().unwrap();
}

fn get_app(config: ServerConfig) -> HttpServer<DefaultRouter<Box<RouteFn>>> {
    let mut app = App::new(&config.bind, config.port);
    if let Some(n) = config.thread_count {
        app.set_thread_count(n);
    }
    app
}

fn recover<F>(f: F) -> impl Fn(HttpRequestContext, &mut ResponseHandle)
where
    F: Fn(HttpRequestContext, &mut ResponseHandle) + UnwindSafe,
{
    move |ctx, res| {
        if let Err(panic_info) =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(ctx, res)))
        {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s
            } else {
                ""
            };
            eprintln!("handler panicked: {msg}");
            res.send(
                &HttpStatus::of(500),
                HttpHeaders::new(),
                &b"Internal Server Error"[..],
            );
        }
    }
}
