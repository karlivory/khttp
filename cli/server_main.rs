use std::net::TcpStream;
use std::panic::UnwindSafe;
use std::time::Duration;
use std::{io, thread};

use crate::args_parser::{ServerConfig, ServerOp};
use khttp::common::{HttpHeaders, HttpMethod, HttpStatus};
use khttp::router::DefaultRouter;
use khttp::server::{
    App, HttpRequestContext, HttpServerBuilder, ResponseHandle, RouteFn, StreamSetupAction,
};

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
        recover(|mut ctx, res| res.ok(ctx.headers.clone(), ctx.get_body_reader())),
    );
    app.build().serve().unwrap();
}

fn run_sleep_server(config: ServerConfig) {
    let mut app = get_app(config);

    app.map_route(
        HttpMethod::Get,
        "/sleep",
        recover(|ctx, res| {
            thread::sleep(Duration::from_secs(3));
            res.ok(ctx.headers, &[][..])
        }),
    );
    app.build().serve().unwrap();
}

fn get_stream_setup_fn(
    config: ServerConfig,
) -> impl Fn(io::Result<TcpStream>) -> StreamSetupAction {
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

fn get_app(config: ServerConfig) -> HttpServerBuilder<DefaultRouter<Box<RouteFn>>> {
    let mut app = App::new(&config.bind, config.port);
    if let Some(n) = config.thread_count {
        app.set_thread_count(n);
    }
    app.set_stream_setup_fn(get_stream_setup_fn(config));
    app
}

fn recover<F>(f: F) -> impl Fn(HttpRequestContext, &mut ResponseHandle) -> io::Result<()>
where
    F: Fn(HttpRequestContext, &mut ResponseHandle) -> io::Result<()> + UnwindSafe,
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
            )
        } else {
            Ok(())
        }
    }
}
