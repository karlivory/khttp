use khttp::{Headers, Method::*, Server};

fn main() {
    println!("Starting server...");
    let mut app = Server::builder("127.0.0.1:8080").unwrap();
    app.set_thread_count(20);

    app.route(Get, "/", |_, r| r.ok(Headers::empty(), &b"Hey!"[..]));
    app.route(Post, "/", |mut c, r| r.ok(&c.headers.clone(), c.body()));

    app.build().serve_epoll().unwrap();
}
