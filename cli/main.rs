// cli/main.rs

use args_parser::ArgsParser;
use khttp::common::{HttpHeaders, HttpMethod};
use khttp::server::{App, HttpRequestContext};

pub mod args_parser;

fn main() {
    // let args = ArgsParser::parse(env::args());
    // for argument in env::args() {
    //     println!("{argument}");
    // }
    run_echo_server();
}

fn run_echo_server() {
    let mut app = App::new(8080, 5);
    app.map_route(HttpMethod::Post, "/foo", |mut ctx, res| {
        let mut headers = HttpHeaders::new();
        if let Some(len) = ctx.headers.get_content_length() {
            headers.set_content_length(len);
        }
        res.ok(&headers, ctx.read_body().to_ascii_uppercase().as_slice());
    });
    app.serve_n(1);
}

fn foo(mut context: HttpRequestContext) {}
