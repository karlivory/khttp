use std::{net::TcpListener, sync::Arc, thread};

use khttp::{Headers, Method::*, Server};

fn main() {
    println!("Starting server...");
    let mut app = Server::builder("127.0.0.1:8080").unwrap();
    app.route(Get, "/", |_, r| r.ok(Headers::new(), &b"hello"[..]));

    let arc = Arc::new(app.build());

    let listener = TcpListener::bind("0.0.0.0:8080").unwrap();
    for stream in listener.incoming() {
        let app = arc.clone();
        thread::spawn(move || {
            app.handle(stream.unwrap()).ok();
        });
    }
}
