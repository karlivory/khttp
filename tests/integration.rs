#![cfg(feature = "client")]
use khttp::{Client, ClientResponseHandle, ConnectionSetupAction, Headers, Method, Server, Status};
use std::io;
use std::net::SocketAddr;
use std::{
    io::Cursor,
    net::TcpStream,
    sync::{atomic::AtomicU64, Arc},
    thread::{self},
    time::Duration,
};

#[test]
fn test_serve_default() {
    const TEST_PORT: u16 = 32734;
    let handle = thread::spawn(|| build_server(TEST_PORT).serve().unwrap());
    thread::sleep(Duration::from_millis(10));

    run_test_requests(TEST_PORT);
    TcpStream::connect(("127.0.0.1", TEST_PORT)).expect("should close server");
    handle.join().unwrap();
}

#[test]
fn test_serve_threaded() {
    const TEST_PORT: u16 = 32735;
    let handle = thread::spawn(|| build_server(TEST_PORT).serve_threaded().unwrap());
    thread::sleep(Duration::from_millis(10));

    run_test_requests(TEST_PORT);
    TcpStream::connect(("127.0.0.1", TEST_PORT)).expect("should close server");
    handle.join().unwrap();
}

#[cfg(feature = "epoll")]
#[test]
fn test_serve_epoll() {
    const TEST_PORT: u16 = 32736;
    let handle = thread::spawn(|| build_server(TEST_PORT).serve_epoll().unwrap());
    thread::sleep(Duration::from_millis(10));

    run_test_requests(TEST_PORT);
    TcpStream::connect(("127.0.0.1", TEST_PORT)).expect("should close server");
    handle.join().unwrap();
}

// ---------------------------------------------------------------------
// server & client
// ---------------------------------------------------------------------

fn build_server(port: u16) -> khttp::Server {
    let mut app = Server::builder(format!("127.0.0.1:{port}")).unwrap();

    app.route(Method::Get, "/hello", |_, res| {
        res.ok(Headers::empty(), &b"Hello, World!"[..])
    });

    app.route(Method::Post, "/api/uppercase", |mut ctx, res| {
        let mut body = ctx.body().vec().unwrap();
        body.make_ascii_uppercase();
        res.send(&Status::of(201), Headers::empty(), &body[..])
    });

    app.route(Method::Delete, "/user/:id", |ctx, res| {
        let body = format!("no user: {}", ctx.params.get("id").unwrap());
        res.send(&Status::of(400), Headers::empty(), body.as_bytes())
    });

    app.route(Method::Get, "/chunked", |_, res| {
        let body = "Chunked Response 123";
        let mut headers = Headers::new();
        headers.set_transfer_encoding_chunked();
        res.send(&Status::of(200), &headers, body.as_bytes())
    });

    app.route(Method::Post, "/upload/chunked", |mut ctx, res| {
        let body = ctx.body().string().unwrap();
        let body = format!("got: {body}");
        res.send(&Status::of(200), Headers::empty(), body.as_bytes())
    });

    let counter = Arc::new(AtomicU64::new(0));
    app.connection_setup_hook(request_limiter(counter, 6));
    app.build()
}

fn run_test_requests(port: u16) {
    let mut client = Client::new(format!("localhost:{port}"));

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

    let response = client.get("/chunked", Headers::empty()).unwrap();
    assert_status_and_body(response, 200, "Chunked Response 123");

    let response = client
        .post("/upload/chunked", Headers::empty(), "hello123".as_bytes())
        .unwrap();
    assert_status_and_body(response, 200, "got: hello123");
}

// ---------------------------------------------------------------------
// UTILS
// ---------------------------------------------------------------------

fn request_limiter(
    counter: Arc<AtomicU64>,
    n: u64,
) -> impl Fn(io::Result<(TcpStream, SocketAddr)>) -> ConnectionSetupAction {
    let counter = counter.clone();
    move |stream| match stream {
        Ok((stream, _peer_addr)) => {
            let seen = counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if seen < n {
                // TODO: if read/write times out, then test should fail
                let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
                ConnectionSetupAction::Proceed(stream)
            } else {
                ConnectionSetupAction::StopAccepting
            }
        }
        Err(e) => panic!("socket error: {e}"),
    }
}

fn assert_status_and_body(
    mut res: ClientResponseHandle,
    expected_status: u16,
    expected_body: &str,
) {
    assert_eq!(res.status.code, expected_status);
    assert_eq!(res.body().string().unwrap(), expected_body);
    // ClientResponseHandle is dropped -> stream is closed
}
