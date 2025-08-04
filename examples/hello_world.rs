use khttp::{Headers, Method::*, Server, Status};

fn main() {
    let mut app = Server::builder("127.0.0.1:8080").unwrap();
    app.thread_count(20);

    // custom fallback_route
    app.fallback_route(|_, r| {
        r.send(
            &Status::NOT_FOUND,
            Headers::empty(),
            &b"404 - not found"[..],
        )
    });

    // GET http://localhost:8080 should respond with "Hello, World!"
    app.route(Get, "/", |_, r| {
        let mut headers = Headers::new();
        headers.add("Server", b"khttp");
        r.ok(&headers, &b"Hello, World!"[..])
    });

    app.build().serve_epoll().unwrap();
}
