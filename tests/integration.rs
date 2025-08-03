#![cfg(feature = "client")]
use khttp::{Client, ClientResponseHandle, Headers, Method, Server, Status, StreamSetupAction};
use std::io;
use std::{
    io::Cursor,
    net::TcpStream,
    sync::{Arc, atomic::AtomicU64},
    thread::{self},
    time::Duration,
};

const TEST_PORT: u16 = 32734;

#[test]
fn simple_multi_test() {
    // start server, wait for it to be active
    let h = start_server(4);
    thread::sleep(Duration::from_millis(10));

    let mut client = Client::new(&format!("localhost:{}", TEST_PORT));

    let response = client.get("/hello", Headers::empty()).unwrap();
    assert_status_and_body(response, 200, "Hello, World!");

    let response = client
        .post("/api/uppercase", Headers::empty(), Cursor::new("test123"))
        .unwrap();
    assert_status_and_body(response, 201, "TEST123");

    let response = client
        .post("/not-routed", Headers::empty(), Cursor::new(""))
        .unwrap();
    assert_status_and_body(response, 404, "");

    let response = client
        .delete("/user/123", Headers::empty(), Cursor::new(""))
        .unwrap();
    assert_status_and_body(response, 400, "no user: 123");

    // wait for server thread to finish
    let _ = std::net::TcpStream::connect(("127.0.0.1", TEST_PORT));
    let _ = h.join();
}

// ---------------------------------------------------------------------
// UTILS
// ---------------------------------------------------------------------

fn start_server(n: u64) -> std::thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut app = Server::builder(format!("127.0.0.1:{TEST_PORT}")).unwrap();

        app.route(Method::Get, "/hello", |_, res| {
            res.ok(Headers::empty(), &b"Hello, World!"[..])
        });

        app.route(Method::Post, "/api/uppercase", |mut ctx, res| {
            let mut body = ctx.body().vec().unwrap();
            body.make_ascii_uppercase();
            res.send(&Status::of(201), Headers::empty(), &body[..])
        });

        app.route(Method::Delete, "/user/:id", |ctx, res| {
            let body = format!("no user: {}", ctx.route_params.get("id").unwrap());
            res.send(&Status::of(400), Headers::empty(), body.as_bytes())
        });

        let counter = Arc::new(AtomicU64::new(0));
        app.stream_setup_hook(request_limiter(counter, n));
        app.build().serve().ok();
    })
}

fn request_limiter(
    counter: Arc<AtomicU64>,
    n: u64,
) -> impl Fn(io::Result<TcpStream>) -> StreamSetupAction {
    let counter = counter.clone();
    move |stream| match stream {
        Ok(stream) => {
            let seen = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if seen < n {
                StreamSetupAction::Accept(stream)
            } else {
                StreamSetupAction::StopAccepting
            }
        }
        Err(_) => StreamSetupAction::StopAccepting,
    }
}

fn assert_status_and_body(
    mut res: ClientResponseHandle,
    expected_status: u16,
    expected_body: &str,
) {
    assert_eq!(res.status.code, expected_status);
    let body = res.body().string().unwrap();
    assert_eq!(body, expected_body);
}
