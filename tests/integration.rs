#![cfg(feature = "client")]
use khttp::{Client, ClientResponseHandle, Headers, Method, Server, Status, StreamSetupAction};
use std::io;
use std::{
    io::Cursor,
    net::TcpStream,
    sync::{atomic::AtomicU64, Arc},
    thread::{self},
    time::Duration,
};

const TEST_PORT: u16 = 32734;

#[test]
fn simple_multi_test() {
    // start server, wait for it to be active
    let h = start_server(6);
    thread::sleep(Duration::from_millis(10));

    let mut client = Client::new(format!("localhost:{}", TEST_PORT));

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

    // wait for server thread to finish
    let _ = TcpStream::connect(("127.0.0.1", TEST_PORT));
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
                // TODO: if read/write times out, then test should fail
                let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
                let _ = stream.set_write_timeout(Some(Duration::from_millis(500)));
                StreamSetupAction::Proceed(stream)
            } else {
                StreamSetupAction::StopAccepting
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
