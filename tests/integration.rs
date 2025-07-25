#![cfg(feature = "client")]
use khttp::{
    client::Client,
    common::{HttpHeaders, HttpMethod, HttpStatus},
    server::App,
};
use std::{
    io::Cursor,
    thread::{self},
    time::Duration,
};

const TEST_PORT: u16 = 32734;

#[test]
fn simple_multi_test() {
    // start server, wait for it to be active
    let h = start_server(4);
    thread::sleep(Duration::from_millis(10));

    let client = Client::new(&format!("localhost:{}", TEST_PORT));

    let response = client.get("/hello", HttpHeaders::new()).unwrap();
    assert_status_and_body(response, 200, "Hello, World!");

    let response = client
        .post("/api/uppercase", HttpHeaders::new(), Cursor::new("test123"))
        .unwrap();
    assert_status_and_body(response, 201, "TEST123");

    let response = client
        .post("/not-routed", HttpHeaders::new(), Cursor::new(""))
        .unwrap();
    assert_status_and_body(response, 404, "");

    let response = client
        .delete("/user/123", HttpHeaders::new(), Cursor::new(""))
        .unwrap();
    assert_status_and_body(response, 400, "no user: 123");

    // wait for server thread to finish
    let _ = h.join();
}

// ---------------------------------------------------------------------
// UTILS
// ---------------------------------------------------------------------

fn start_server(n: u64) -> std::thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut app = App::new("127.0.0.1", TEST_PORT);

        app.map_route(HttpMethod::Get, "/hello", |_, res| {
            res.send(&HttpStatus::OK, HttpHeaders::new(), &b"Hello, World!"[..]);
        });

        app.map_route(HttpMethod::Post, "/api/uppercase", |mut ctx, res| {
            let mut body = ctx.read_body().unwrap();
            body.make_ascii_uppercase();
            res.send(&HttpStatus::of(201), HttpHeaders::new(), &body[..]);
        });

        app.map_route(HttpMethod::Delete, "/user/:id", |ctx, res| {
            let body = format!("no user: {}", ctx.route_params.get("id").unwrap());
            res.send(&HttpStatus::of(400), HttpHeaders::new(), body.as_bytes());
        });

        app.build().serve_n(n).ok();
    })
}

fn assert_status_and_body(
    mut res: khttp::client::HttpResponse,
    expected_status: u16,
    expected_body: &str,
) {
    assert_eq!(res.status.code, expected_status);
    let body = res.read_body_to_string().unwrap();
    assert_eq!(body, expected_body);
}
