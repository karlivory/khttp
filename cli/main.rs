// cli/main.rs

use args_parser::ArgsParser;
use khttp::common::HttpMethod;
use khttp::common::{HttpRequest, HttpResponse};
use khttp::server::App;
use std::env::{self};

pub mod args_parser;

fn main() {
    let args = ArgsParser::parse(env::args());
    for argument in env::args() {
        println!("{argument}");
    }
    run_echo_server();
}

fn run_echo_server() {
    let mut app = App::with_default_router(8080, 5);
    // let foo = move |request: HttpRequest| HttpResponse::ok(HttpHeaders::new(), request.body);
    app.map_route(HttpMethod::Post, "/", foo);
    app.serve_n(1);
}

fn foo(request: HttpRequest) -> HttpResponse {
    todo!()
}
