use khttp::{Headers, Method::*, Server};

static HELLO_WORLD: &[u8] = b"Hello, World!";

fn main() {
    let mut app = Server::builder("127.0.0.1:8080").unwrap();
    app.route(Get, "/", |_, res| res.ok(Headers::empty(), HELLO_WORLD));
    app.build().serve_epoll().unwrap();
}
