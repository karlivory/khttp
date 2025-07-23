// tests/router.rs

use khttp::{
    common::HttpMethod,
    router::{AppRouter, DefaultRouter},
};

// ------------------------------- types & utils -------------------------------

type RouteTy = (usize, &'static str);

type Router = DefaultRouter<RouteTy>;

fn new_router() -> Router {
    DefaultRouter::new()
}

fn add_routes(router: &mut Router, method: &HttpMethod, routes: &[(&'static str, usize)]) {
    for (pat, id) in routes {
        router.add_route(method, pat, (*id, *pat));
    }
}

/// Assert that `uri` matches and the route index equals `expected_idx`.
fn assert_match(router: &Router, method: &HttpMethod, uri: &str, expected_idx: usize) {
    let m = router
        .match_route_params(method, uri)
        .unwrap_or_else(|| panic!("expected match for URI {}", uri));
    assert_eq!(m.route.0, expected_idx, "URI: {}", uri);
}

/// Assert that `uri` matches, the route index equals `expected_idx`,
/// and that all expected params are present and equal.
fn assert_match_params(
    router: &Router,
    method: &HttpMethod,
    uri: &str,
    expected_idx: usize,
    expected_params: &[(&str, &str)],
) {
    let m = router
        .match_route_params(method, uri)
        .unwrap_or_else(|| panic!("expected match for URI {}", uri));
    assert_eq!(m.route.0, expected_idx, "URI: {}", uri);
    for (k, v) in expected_params {
        assert_eq!(
            m.params.get(*k).unwrap(),
            *v,
            "param '{}' mismatch for {}",
            k,
            uri
        );
    }
}

fn assert_404(router: &Router, method: &HttpMethod, uri: &str) {
    assert!(
        router.match_route_params(method, uri).is_none(),
        "expected 404 for URI {}",
        uri
    );
}

// ------------------------------- tests --------------------------------------

#[test]
fn nested_routes() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &HttpMethod::Get,
        &[("/route", 0), ("/route/foo", 1)],
    );

    assert_match(&r, &HttpMethod::Get, "/route", 0);
    assert_match(&r, &HttpMethod::Get, "/route/foo", 1);
}

#[test]
fn http_parameters_ignored_for_path_match() {
    let mut r = new_router();
    add_routes(&mut r, &HttpMethod::Get, &[("/route1", 0), ("/route2", 1)]);

    assert_match(&r, &HttpMethod::Get, "/route1", 0);
    assert_match(&r, &HttpMethod::Get, "/route1?foo=bar", 0);
    assert_match(&r, &HttpMethod::Get, "/route2?foo=bar&fizz=buzz", 1);
}

#[test]
fn wildcard() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &HttpMethod::Get,
        &[("/route1/*/foo", 0), ("/route2/*", 1), ("/route2/hey", 2)],
    );

    assert_match(&r, &HttpMethod::Get, "/route1/abc/foo", 0);
    assert_match(&r, &HttpMethod::Get, "/route1/d/foo", 0);
    assert_match(&r, &HttpMethod::Get, "/route2/hello?foo=bar&fizz=buzz", 1);
    assert_match(&r, &HttpMethod::Get, "/route2/hey?foo=bar&fizz=buzz", 2);
}

#[test]
fn double_wildcard() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &HttpMethod::Get,
        &[
            ("/route1/**", 0),
            ("/route2/*", 1),
            ("/route2", 2),
            ("/**", 3),
        ],
    );

    assert_match(&r, &HttpMethod::Get, "/route1/abc/def/hjk", 0);
    assert_match(&r, &HttpMethod::Get, "/route1/d/foo", 0);
    assert_match(&r, &HttpMethod::Get, "/route2/hello?foo=bar&fizz=buzz", 1);
    assert_match(&r, &HttpMethod::Get, "/route2?foo=bar&fizz=buzz", 2);
    assert_match(&r, &HttpMethod::Get, "/", 3);
    assert_match(&r, &HttpMethod::Get, "/route3", 3);
    assert_match(&r, &HttpMethod::Get, "/fallback/for/any/other/route", 3);
}

#[test]
fn precedence_literal_wildcards() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &HttpMethod::Get,
        &[("/route/foo", 0), ("/route/*", 1), ("/route/**", 2)],
    );

    assert_match(&r, &HttpMethod::Get, "/route/foo", 0);
    assert_match(&r, &HttpMethod::Get, "/route/foo?fizz=buzz", 0);

    assert_match(&r, &HttpMethod::Get, "/route/hello", 1);
    assert_match(&r, &HttpMethod::Get, "/route/foobar", 1);

    assert_match(&r, &HttpMethod::Get, "/route/foo/bar?foo=bar&fizz=buzz", 2);
}

#[test]
fn hashing_duplicate_routes() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &HttpMethod::Get,
        &[
            ("/route/foo", 0),
            ("/route/foo", 1),
            ("/route/foo2", 2),
            ("/route/*", 3),
            ("/route/*", 4),
            ("/user/:id", 5),
            ("/user/:slug", 6),
        ],
    );

    // Last inserted 'wins' for identical routes
    assert_match(&r, &HttpMethod::Get, "/route/foo", 1);
    assert_match(&r, &HttpMethod::Get, "/route/foo2", 2);
    assert_match(&r, &HttpMethod::Get, "/route/hey", 4);
    assert_match(&r, &HttpMethod::Get, "/user/1234", 6);
}

#[test]
fn params_extraction() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &HttpMethod::Get,
        &[("/users/:id", 0), ("/users/:id/posts/:post_id", 1)],
    );

    assert_match_params(&r, &HttpMethod::Get, "/users/12", 0, &[("id", "12")]);
    assert_match_params(
        &r,
        &HttpMethod::Get,
        "/users/42/posts/abc",
        1,
        &[("id", "42"), ("post_id", "abc")],
    );
}

#[test]
fn unmapping() {
    let mut r = new_router();

    assert_404(&r, &HttpMethod::Get, "/hello");

    r.add_route(&HttpMethod::Get, "/hello", (10, "/hello"));
    assert_match(&r, &HttpMethod::Get, "/hello", 10);

    let removed = r.remove_route(&HttpMethod::Get, "/hello").unwrap();
    assert_eq!(removed.0, 10);
    assert_404(&r, &HttpMethod::Get, "/hello");
}

#[test]
fn literal_beats_param() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &HttpMethod::Get,
        &[("/users/me", 0), ("/users/:id", 1)],
    );

    assert_match(&r, &HttpMethod::Get, "/users/me", 0);
    assert_match_params(&r, &HttpMethod::Get, "/users/42", 1, &[("id", "42")]);
}

#[test]
fn param_beats_wildcard() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &HttpMethod::Get,
        &[("/files/:name", 0), ("/files/*", 1)],
    );

    assert_match_params(
        &r,
        &HttpMethod::Get,
        "/files/readme",
        0,
        &[("name", "readme")],
    );
}

#[test]
fn longest_literal_then_precedence() {
    // /a/b/*  -> LML=2
    // /a/*/c  -> LML=1 but ends with literal:
    //                  despite higher precedence it gets filtered out due to lower LML
    let mut r = new_router();
    add_routes(&mut r, &HttpMethod::Get, &[("/a/b/*", 0), ("/a/*/c", 1)]);

    assert_match(&r, &HttpMethod::Get, "/a/b/c", 0);
}

#[test]
fn many_params_extraction() {
    let mut r = new_router();
    add_routes(&mut r, &HttpMethod::Get, &[("/:a/:b/:c", 0)]);

    assert_match_params(
        &r,
        &HttpMethod::Get,
        "/one/two/three",
        0,
        &[("a", "one"), ("b", "two"), ("c", "three")],
    );
}

#[test]
fn root_double_wildcard_fallback() {
    let mut r = new_router();
    add_routes(&mut r, &HttpMethod::Get, &[("/api/*", 0), ("/**", 1)]);

    assert_match(&r, &HttpMethod::Get, "/whatever/else", 1);
    assert_match(&r, &HttpMethod::Get, "/api/foo", 0);
}

#[test]
fn method_isolation() {
    let mut r = new_router();
    add_routes(&mut r, &HttpMethod::Get, &[("/users/:id", 0)]);
    add_routes(&mut r, &HttpMethod::Post, &[("/users/:id", 1)]);

    assert_match_params(&r, &HttpMethod::Get, "/users/10", 0, &[("id", "10")]);
    assert_match_params(&r, &HttpMethod::Post, "/users/10", 1, &[("id", "10")]);
}

#[test]
fn remove_param_route() {
    let mut r = new_router();
    r.add_route(&HttpMethod::Get, "/users/:id", (99, "/users/:id"));

    assert_match_params(&r, &HttpMethod::Get, "/users/5", 99, &[("id", "5")]);
    let removed = r.remove_route(&HttpMethod::Get, "/users/:id").unwrap();
    assert_eq!(removed.0, 99);
    assert_404(&r, &HttpMethod::Get, "/users/5");
}

#[test]
fn query_string_does_not_affect_params() {
    let mut r = new_router();
    add_routes(&mut r, &HttpMethod::Get, &[("/search/:term", 0)]);

    assert_match_params(
        &r,
        &HttpMethod::Get,
        "/search/rust?lang=en&sort=asc",
        0,
        &[("term", "rust")],
    );
}

#[test]
fn overlap_param_and_double_wildcard() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &HttpMethod::Get,
        &[("/blog/:slug", 0), ("/blog/**", 1)],
    );

    // single segment -> param wins
    assert_match_params(&r, &HttpMethod::Get, "/blog/intro", 0, &[("slug", "intro")]);

    // deeper path -> ** wins
    assert_match(&r, &HttpMethod::Get, "/blog/2024/10/interesting", 1);
}
