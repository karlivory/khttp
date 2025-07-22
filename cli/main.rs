// cli/main.rs

use std::env;
use std::io::Cursor;

use args_parser::{ArgsParser, ClientOp, ClientOpArg, MainOp};
use khttp::client::Client;
use khttp::common::{HttpHeaders, HttpMethod};
use khttp::server::App;

pub mod args_parser;

fn main() {
    let arg_vec: Vec<String> = env::args().collect();
    dbg!(arg_vec);
    let args = ArgsParser::parse(env::args());
    if args.is_err() {
        print_help();
    } else {
        dbg!(&args);
        handle_op(args.unwrap());
    }
}

fn handle_op(op: MainOp) {
    match op {
        MainOp::Server(op) => todo!(),
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
    let mut response = client
        .exchange(&op.method, &op.uri, &headers, Cursor::new(body))
        .expect("TODO");
    let response_body = response.read_body_to_string();
    if verbose {
        println!("{} {}", response.status.code, response.status.reason);
        for (h, v) in response.headers.get_header_map() {
            println!("{}: {}", h, v);
        }
    }
    print!("{}", response_body);
}

fn print_help() {
    println!("-- khttp client");
    println!();
    println!("HELP: how to use and stuff");
    println!("example: khttp get foo");
}

fn run_echo_server() {
    let mut app = App::new(8080, 5);
    app.map_route(HttpMethod::Post, "/echo", |mut ctx, res| {
        let body = ctx.read_body().to_ascii_uppercase();
        res.ok(&ctx.headers, body.as_slice());
    });
    app.serve();
}
