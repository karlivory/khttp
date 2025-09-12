use std::net::{SocketAddr, TcpStream};
use std::panic::UnwindSafe;
use std::time::Duration;
use std::{io, thread};

use crate::args_parser::{ServerConfig, ServerOp};
use khttp::{ConnectionSetupAction, RequestContext, ResponseHandle, Server, ServerBuilder};
use khttp::{Headers, Method, Status};

pub fn run(op: ServerOp) {
    match op {
        ServerOp::Echo(config) => run_echo_server(config),
        ServerOp::Sleep(config) => run_sleep_server(config),
    }
}

fn run_echo_server(config: ServerConfig) {
    let mut app = get_app(config);

    app.route(
        Method::Post,
        "/**",
        recover(|mut ctx, res| res.okr(&ctx.headers.clone(), ctx.body())),
    );
    app.build().serve().unwrap();
}

fn run_sleep_server(config: ServerConfig) {
    let mut app = get_app(config);

    app.route(
        Method::Get,
        "/sleep",
        recover(|_ctx, res| {
            thread::sleep(Duration::from_secs(3));
            res.ok0(Headers::empty())
        }),
    );
    app.build().serve().unwrap();
}

fn get_connection_setup_fn(
    config: ServerConfig,
) -> impl Fn(io::Result<(TcpStream, SocketAddr)>) -> ConnectionSetupAction {
    let read_timeout = config.tcp_read_timeout;
    let write_timeout = config.tcp_write_timeout;
    let tcp_nodelay = config.tcp_nodelay;

    move |connection| {
        let stream = match connection {
            Ok(conn) => conn.0,
            Err(_) => return ConnectionSetupAction::Drop,
        };
        if let Some(timeout) = read_timeout {
            match stream.set_read_timeout(Some(Duration::from_millis(timeout))) {
                Ok(_) => (),
                Err(_) => return ConnectionSetupAction::Drop,
            };
        }
        if let Some(timeout) = write_timeout {
            match stream.set_write_timeout(Some(Duration::from_millis(timeout))) {
                Ok(_) => (),
                Err(_) => return ConnectionSetupAction::Drop,
            }
        }
        match stream.set_nodelay(tcp_nodelay) {
            Ok(_) => (),
            Err(_) => return ConnectionSetupAction::Drop,
        }
        ConnectionSetupAction::Proceed(stream)
    }
}

fn get_app(config: ServerConfig) -> ServerBuilder {
    let mut app = Server::builder("0.0.0.0:8080").unwrap();
    if let Some(n) = config.thread_count {
        app.thread_count(n);
    }
    app.connection_setup_hook(get_connection_setup_fn(config));
    app
}

fn recover<F>(f: F) -> impl Fn(RequestContext, &mut ResponseHandle) -> io::Result<()>
where
    F: Fn(RequestContext, &mut ResponseHandle) -> io::Result<()> + UnwindSafe,
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
            res.send(&Status::of(500), Headers::empty(), "Internal Server Error")
        } else {
            Ok(())
        }
    }
}
