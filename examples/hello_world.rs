use khttp::{Headers, Method::*, Server};

fn main() {
    let mut app = Server::builder("127.0.0.1:8080").unwrap();
    app.route(Get, "/", |_, res| res.ok(Headers::empty(), "Hello, World!"));
    app.build().serve_epoll().unwrap();
}
