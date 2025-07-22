// benches/benches.rs
use std::io::Cursor;

use khttp::{
    common::{HttpHeaders, HttpMethod},
    server::App,
};

static HELLO_WORLD: &str = "Hello, World!";

fn main() {
    let mut app = App::new("127.0.0.1", 3000);

    app.map_route(HttpMethod::Get, "/", |_, res| {
        let mut headers = HttpHeaders::new();
        headers.set_content_length(HELLO_WORLD.len());
        res.ok(headers, Cursor::new(HELLO_WORLD))
    });
    app.set_thread_count(100);
    app.serve();
}
