// cli/main.rs

use std::io::Cursor;
use std::time::Duration;
use std::{env, thread};

use args_parser::{ArgsParser, ClientOp, ClientOpArg, MainOp, ServerOpArg};
use khttp::client::{Client, HttpClientError};
use khttp::common::{HttpHeaders, HttpMethod};
use khttp::router::DefaultRouter;
use khttp::server::{App, HttpServer, RouteFn};

pub mod args_parser;

fn main() {
    let args = ArgsParser::parse(env::args());
    match args {
        Err(_) => print_help(),
        Ok(op) => handle_op(op),
    }
}

fn handle_op(op: MainOp) {
    match op {
        MainOp::Server(op) => match op {
            args_parser::ServerOp::Echo(args) => run_echo_server(args),
            args_parser::ServerOp::Sleep(args) => run_sleep_server(args),
        },
        MainOp::Client(op) => handle_client_op(op),
    }
}

fn handle_client_op(op: ClientOp) {
    let address = format!("{}:{}", op.host, op.port);
    let client = Client::new(&address);
    let mut headers = HttpHeaders::new();
    let mut body = String::new();
    let mut verbose = false;
    for opt_arg in op.opt_args {
        match opt_arg {
            ClientOpArg::Header((h, v)) => headers.add_header(&h, &v),
            ClientOpArg::Body(b) => body = b,
            ClientOpArg::Verbose => verbose = true,
        };
    }
    let response = client.exchange(&op.method, &op.uri, &headers, Cursor::new(body));
    if let Err(e) = response {
        handle_client_error(e);
        return;
    }
    let mut response = response.unwrap();
    let response_body = response.read_body_to_string();
    if verbose {
        println!("{} {}", response.status.code, response.status.reason);
        for (h, v) in response.headers.get_header_map() {
            println!("{}: {}", h, v);
        }
    }
    print!("{}", response_body);
}

fn handle_client_error(err: HttpClientError) {
    print!("ERROR: ");
    match err {
        HttpClientError::ConnectionFailure(e) => println!("failed to connect: {}", e),
        HttpClientError::WriteFailure(e) => println!("failed to write to tcp socket: {}", e),
        HttpClientError::ReadFailure(e) => println!("failed to read from tcp socket: {}", e),
        HttpClientError::ParsingFailure => println!("failed to parse response"),
    }
}

fn print_help() {
    println!("-- khttp client");
    println!();
    println!("HELP: how to use and stuff");
    println!("example: khttp get foo");
}

fn get_app(args: Vec<ServerOpArg>) -> HttpServer<DefaultRouter<Box<RouteFn>>> {
    let mut address = "127.0.0.1".to_string();
    let mut port = 8080;
    let mut thread_count = 10;
    let mut _verbose = false;
    for opt_arg in args {
        match opt_arg {
            ServerOpArg::Port(p) => port = p,
            ServerOpArg::BindAddress(a) => address = a.clone(),
            ServerOpArg::ThreadCount(x) => thread_count = x,
            ServerOpArg::Verbose => _verbose = true,
        };
    }
    App::new(address.as_str(), port, thread_count)
}

fn run_echo_server(args: Vec<ServerOpArg>) {
    let mut app = get_app(args);
    app.map_route(HttpMethod::Post, "/**", |mut ctx, res| {
        res.ok(&ctx.headers.clone(), ctx.get_body_reader());
    });
    app.serve();
}

fn run_sleep_server(args: Vec<ServerOpArg>) {
    let mut app = get_app(args);
    app.map_route(HttpMethod::Get, "/sleep", |ctx, res| {
        thread::sleep(Duration::from_secs(3));
        res.ok(&ctx.headers, &[][..]);
    });
    app.serve();
}
