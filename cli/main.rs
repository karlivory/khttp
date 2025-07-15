// cli/main.rs

use args_parser::ArgsParser;
use khttp::common::{HttpHeaders, HttpMethod, HttpStatus};
use khttp::common::{HttpRequest, HttpResponse};
use khttp::server::{App, HttpRequestContext};
use std::env::{self};
use std::fs::File;
use std::io::{BufReader, Cursor, Read};

pub mod args_parser;

fn main() {
    // let args = ArgsParser::parse(env::args());
    // for argument in env::args() {
    //     println!("{argument}");
    // }
    run_echo_server();
}

fn run_echo_server() {
    let mut app = App::with_default_router(8080, 5);
    // let foo = move |request: HttpRequest| HttpResponse::ok(HttpHeaders::new(), request.body);
    // app.map_route(HttpMethod::Post, "/", foo);
    app.map_route(HttpMethod::Post, "/foo", |mut ctx| {
        let mut buf = String::new();
        ctx.get_body_reader().read_to_string(&mut buf).unwrap();

        let mut headers = HttpHeaders::new();
        headers.set_content_length(buf.len());

        ctx.send(&HttpStatus::of(200), &headers, Cursor::new(buf));
    });
    app.serve_n(1);
}

fn foo(mut context: HttpRequestContext) {}
