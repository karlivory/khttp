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

struct Upper<R: Read> {
    inner: R,
}

impl<R: Read> Read for Upper<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        for byte in &mut buf[..n] {
            *byte = byte.to_ascii_uppercase();
        }
        Ok(n)
    }
}

fn run_echo_server() {
    let mut app = App::with_default_router(8080, 5);
    app.map_route(HttpMethod::Post, "/foo", |mut ctx, res| {
        let mut headers = HttpHeaders::new();
        headers.set_content_length(ctx.headers.get_content_length().unwrap_or(0));
        let body = Upper {
            inner: ctx.get_body_reader(),
        };
        res.ok(&headers, body);
    });
    app.serve_n(1);
}

fn foo(mut context: HttpRequestContext) {}
