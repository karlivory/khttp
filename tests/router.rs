// tests/router.rs
//
// TODO: add some useful router tests
// e.g. /hello -> /hello?foo=bar should match
// (table-based test)

#[cfg(test)]
mod tests {
    use khttp::{
        common::{HttpHeaders, HttpMethod, HttpRequest, HttpResponse, HttpStatus},
        router::{DefaultRouter, RouteFn},
        server::{App, HttpServer},
    };

    fn request(uri: &str) -> HttpRequest {
        HttpRequest {
            body: None,
            headers: HttpHeaders::new(),
            method: HttpMethod::Get,
            uri: uri.to_string(),
        }
    }

    fn response(response_str: &str) -> HttpResponse {
        HttpResponse {
            body: Some(response_str.as_bytes().to_vec()),
            headers: HttpHeaders::new(),
            status: HttpStatus::of(200),
        }
    }

    fn get_app() -> HttpServer<DefaultRouter<Box<RouteFn>>> {
        App::with_default_router(8080, 5)
    }

    struct Route {
        route: &'static str,
        must_match: Vec<&'static str>,
    }

    fn run_route_test(routes: Vec<Route>) {
        let mut app = get_app();

        for (i, route) in routes.iter().enumerate() {
            let response_str = format!("route {}", i);
            app.map_route(HttpMethod::Get, route.route, move |_| {
                response(&response_str)
            });
        }

        for (i, route) in routes.iter().enumerate() {
            let expected_response_body = format!("route {}", i);
            for test_uri in route.must_match.iter() {
                let request = request(test_uri);
                let response = app.handle(request);
                assert_eq!(200, response.status.code);

                let response_body = response.body.unwrap();
                let response_body = String::from_utf8_lossy(&response_body);
                assert_eq!(expected_response_body, response_body);
            }
        }

        app.map_route(HttpMethod::Get, "/hello", |_| response(""));
    }

    #[test]
    fn test_similar_routes() {
        let r = vec![
            Route {
                route: "/route",
                must_match: vec!["/route"],
            },
            Route {
                route: "/route/foo",
                must_match: vec!["/route/foo"],
            },
        ];
        run_route_test(r);
    }

    #[test]
    fn test_http_parameters() {
        return; // TODO
        let r = vec![
            Route {
                route: "/route1",
                must_match: vec!["/route1", "/route1?foo=bar"],
            },
            Route {
                route: "/route2",
                must_match: vec!["/route2?foo=bar&fizz=buzz"],
            },
        ];
        run_route_test(r);
    }

    #[test]
    fn test_unmapping() {
        let mut app = get_app();

        app.map_route(HttpMethod::Get, "/hello", |_| response(""));

        let response = app.handle(request("/hello"));
        assert_eq!(200, response.status.code);

        app.unmap_route(HttpMethod::Get, "/hello");

        let response = app.handle(request("/hello"));
        assert_eq!(404, response.status.code);
    }
}
