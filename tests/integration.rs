// tests/integration.rs

#[cfg(test)]
mod tests {
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

    #[test]
    fn simple_multi_test() {
        // start server
        let h = thread::spawn(|| {
            let mut app = App::new("127.0.0.1", 8080);

            app.map_route(HttpMethod::Get, "/hello", move |_, res| {
                let hello = "Hello, World!".to_string();
                let mut headers = HttpHeaders::new();
                headers.set_content_length(hello.len());
                res.send(&HttpStatus::of(200), headers, Cursor::new(hello));
            });

            app.map_route(HttpMethod::Post, "/api/uppercase", move |mut ctx, res| {
                let mut headers = HttpHeaders::new();
                headers.set_content_length(ctx.headers.get_content_length().unwrap());

                let b = ctx.read_body_to_string().unwrap().to_ascii_uppercase();

                res.send(&HttpStatus::of(201), headers, Cursor::new(b));
            });

            app.serve_n(3);
        });
        // wait for server to be active
        thread::sleep(Duration::from_millis(10));

        // init client
        let client = Client::new("localhost:8080");

        // test: GET /hello
        let mut response = client.get("/hello", &HttpHeaders::new()).unwrap();
        assert_eq!(response.status.code, 200);
        assert_eq!(response.read_body_to_string(), "Hello, World!");

        // test: POST /api/uppercase
        let mut response = client
            .post(
                "/api/uppercase",
                &HttpHeaders::from(vec![(HttpHeaders::CONTENT_LENGTH, "7")]),
                Cursor::new("test123"),
            )
            .unwrap();
        assert_eq!(response.status.code, 201);
        assert_eq!(response.read_body_to_string(), "TEST123");

        // test for 404
        let mut response = client
            .post("/not-routed", &HttpHeaders::new(), Cursor::new(""))
            .unwrap();
        let _ = response.read_body(); // close socket
        assert_eq!(response.status.code, 404);

        // wait for server thread to finish
        let _ = h.join();
    }
}
