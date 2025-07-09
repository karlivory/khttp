// tests/router.rs
//
// tests for DefaultRouter (AppRouter impl)

#[cfg(test)]
mod tests {
    use khttp::{
        common::HttpMethod,
        router::{AppRouter, DefaultRouter},
    };

    fn get_router() -> DefaultRouter<(usize, Route)> {
        DefaultRouter::<(usize, Route)>::new()
    }

    #[derive(Debug, PartialEq, Clone)]
    struct Route {
        route: &'static str,
        must_match: Vec<&'static str>,
    }

    fn run_route_test(routes: Vec<Route>) {
        let mut router = get_router();

        for (i, route) in routes.iter().enumerate() {
            router.add_route(&HttpMethod::Get, route.route, (i, route.clone()));
        }

        for (i, route) in routes.iter().enumerate() {
            for test_uri in route.must_match.iter() {
                let response = router.match_route(&HttpMethod::Get, test_uri);
                debug_assert!(
                    response.is_some(),
                    "Unexpected route 404!\n|i = {}|\nroute: {}\nuri:   {}",
                    i,
                    route.route,
                    test_uri
                );
                debug_assert!(
                    response.is_some() && response.unwrap().0 == i,
                    "Expected route did not match!\n|i = {}|\nroute:   {}\nuri:   {}\nmatched: {}",
                    i,
                    route.route,
                    test_uri,
                    response.unwrap().1.route,
                );
            }
        }
    }

    #[test]
    fn test_nested_routes() {
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
    fn test_wildcard() {
        let r = vec![
            Route {
                route: "/route1/*/foo",
                must_match: vec!["/route1/abc/foo", "/route1/d/foo"],
            },
            Route {
                route: "/route2/*",
                must_match: vec!["/route2/hello?foo=bar&fizz=buzz"],
            },
            Route {
                route: "/route2/hey",
                must_match: vec!["/route2/hey?foo=bar&fizz=buzz"],
            },
        ];
        run_route_test(r);
    }

    #[test]
    fn test_double_wildcard() {
        let r = vec![
            Route {
                route: "/route1/**",
                must_match: vec!["/route1/abc/def/hjk", "/route1/d/foo"],
            },
            Route {
                route: "/route2/*",
                must_match: vec!["/route2/hello?foo=bar&fizz=buzz"],
            },
            Route {
                route: "/route2",
                must_match: vec!["/route2?foo=bar&fizz=buzz"],
            },
        ];
        run_route_test(r);
    }

    #[test]
    fn test_precedence() {
        let r = vec![
            Route {
                route: "/route/foo",
                must_match: vec!["/route/foo", "/route/foo?fizz=buzz"],
            },
            Route {
                route: "/route/*",
                must_match: vec!["/route/hello", "/route/foobar"],
            },
            Route {
                route: "/route/**",
                must_match: vec!["/route/foo/bar?foo=bar&fizz=buzz"],
            },
        ];
        run_route_test(r);
    }

    #[test]
    fn test_hashing() {
        let r = vec![
            Route {
                route: "/route/foo",
                must_match: vec![],
            },
            Route {
                route: "/route/foo",
                must_match: vec!["/route/foo"],
            },
            Route {
                route: "/route/foo2",
                must_match: vec!["/route/foo2"],
            },
            Route {
                route: "/route/*",
                must_match: vec![],
            },
            Route {
                route: "/route/*",
                must_match: vec!["/route/hey"],
            },
        ];
        run_route_test(r);
    }

    // TODO
    // #[test]
    // fn test_unmapping() {
    //     let mut app = get_app();
    //
    //     app.map_route(HttpMethod::Get, "/hello", |_| response(""));
    //
    //     let response = app.handle(request("/hello"));
    //     assert_eq!(200, response.status.code);
    //
    //     app.unmap_route(HttpMethod::Get, "/hello");
    //
    //     let response = app.handle(request("/hello"));
    //     assert_eq!(404, response.status.code);
    // }
}
