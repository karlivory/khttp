// cli/main.rs

use args_parser::ArgsParser;
use khttp::common::{HttpHeaders, HttpResponse};
use khttp::router::DefaultRouter;
use khttp::{common::HttpMethod, server::App};
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
    let mut app = App::<DefaultRouter>::new(8080, 3);
    app.router.map_route(HttpMethod::Post, "/", move |request| {
        HttpResponse::ok(HttpHeaders::new(), request.body)
    });
    app.serve_n(1);
}
