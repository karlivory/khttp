// tests/integration.rs

#[cfg(test)]
mod tests {
    use khttp::{
        client::Client,
        common::{HttpHeaders, HttpMethod, HttpStatus},
        router::DefaultRouter,
        server::{HttpServer, RouteFn},
    };
    use std::{
        io::{Cursor, Read},
        thread,
        time::Duration,
    };

    #[test]
    fn simple_multi_test() {
        // start server
        let h = thread::spawn(|| {
            let mut app = HttpServer::<DefaultRouter<Box<RouteFn>>>::new(8080, 3);
            app.map_route(HttpMethod::Post, "/to-upper", move |mut ctx, res| {
                let mut headers = HttpHeaders::new();
                headers.set_content_length(ctx.headers.get_content_length().unwrap());

                let b = ctx.read_body_to_string().to_ascii_uppercase();

                res.send(&HttpStatus::of(200), &headers, Cursor::new(b));
            });
            app.serve_n(2);
        });
        // wait for server to be active
        thread::sleep(Duration::from_millis(10));

        // init client
        let client = Client::new("localhost:8080");

        // test 1 : echo
        let response = client
            .post(
                "/to-upper".to_string(),
                HttpHeaders::new(),
                Some("test123".bytes().collect::<Vec<u8>>()),
            )
            .unwrap();
        let binding = response.body.unwrap();
        let response_body_str = String::from_utf8_lossy(binding.as_slice());
        assert_eq!(response_body_str, "TEST123");

        // test 2 : check for 404
        let response = client
            .post("/not-routed".to_string(), HttpHeaders::new(), None)
            .unwrap();
        assert_eq!(response.status.code, 404);

        // test 3 : check for 500
        // let response = client
        //     .exchange(HttpRequest {
        //         body: None,
        //         headers: Default::default(),
        //         method: HttpMethod::Custom("FOOBAR".to_string()),
        //         uri: "/".to_string(),
        //     })
        //     .unwrap();
        // assert_eq!(response.status.code, 500);

        // wait for server thread to finish
        let _ = h.join();
    }
}
