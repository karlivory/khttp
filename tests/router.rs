use khttp::{HttpRouter, Method, Router, RouterBuilder};

// ---------------------------------------------------------------------
// TESTS
// ---------------------------------------------------------------------

#[test]
fn empty_route() {
    let mut r = new_router();
    add_routes(&mut r, &Method::Get, &[("/", 0), ("/route", 1)]);
    let r = r.build();
    assert_match(&r, &Method::Get, "/", 0);
}

#[test]
fn nested_routes() {
    let mut r = new_router();
    add_routes(&mut r, &Method::Get, &[("/route", 0), ("/route/foo", 1)]);
    let r = r.build();

    assert_match(&r, &Method::Get, "/route", 0);
    assert_match(&r, &Method::Get, "/route/foo", 1);
}

#[test]
fn wildcard() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &Method::Get,
        &[("/route1/*/foo", 0), ("/route2/*", 1), ("/route2/hey", 2)],
    );
    let r = r.build();

    assert_match(&r, &Method::Get, "/route1/abc/foo", 0);
    assert_match(&r, &Method::Get, "/route1/d/foo", 0);
    assert_match(&r, &Method::Get, "/route2/hello", 1);
    assert_match(&r, &Method::Get, "/route2/hey", 2);
}

#[test]
fn double_wildcard() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &Method::Get,
        &[
            ("/route1/**", 0),
            ("/route2/*", 1),
            ("/route2", 2),
            ("/**", 3),
        ],
    );
    let r = r.build();

    assert_match(&r, &Method::Get, "/route1/abc/def/hjk", 0);
    assert_match(&r, &Method::Get, "/route1/d/foo", 0);
    assert_match(&r, &Method::Get, "/route2/hello", 1);
    assert_match(&r, &Method::Get, "/route2", 2);
    assert_match(&r, &Method::Get, "/", 3);
    assert_match(&r, &Method::Get, "/route3", 3);
    assert_match(&r, &Method::Get, "/fallback/for/any/other/route", 3);
}

#[test]
fn precedence() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &Method::Get,
        &[("/route/foo", 0), ("/route/*", 1), ("/route/**", 2)],
    );
    let r = r.build();

    assert_match(&r, &Method::Get, "/route/foo", 0);
    assert_match(&r, &Method::Get, "/route/hello", 1);
    assert_match(&r, &Method::Get, "/route/foobar", 1);
    assert_match(&r, &Method::Get, "/route/foo/bar/index.html", 2);
}

#[test]
fn hashing_duplicate_routes() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &Method::Get,
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
    let r = r.build();

    // Last inserted 'wins' for identical routes
    assert_match(&r, &Method::Get, "/route/foo", 1);
    assert_match(&r, &Method::Get, "/route/foo2", 2);
    assert_match(&r, &Method::Get, "/route/hey", 4);
    assert_match(&r, &Method::Get, "/user/1234", 6);
}

#[test]
fn params_extraction() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &Method::Get,
        &[("/users/:id", 0), ("/users/:id/posts/:post_id", 1)],
    );
    let r = r.build();

    assert_match_params(&r, &Method::Get, "/users/12", 0, &[("id", "12")]);
    assert_match_params(
        &r,
        &Method::Get,
        "/users/42/posts/abc",
        1,
        &[("id", "42"), ("post_id", "abc")],
    );
}

// #[test]
// fn unmapping() {
//     let mut r = new_router();
//
//     assert_404(&r, &Method::Get, "/hello");
//
//     r.add_route(&Method::Get, "/hello", (10, "/hello"));
//     assert_match(&r, &Method::Get, "/hello", 10);
//
//     let removed = r.remove_route(&Method::Get, "/hello").unwrap();
//     assert_eq!(removed.0, 10);
//     assert_404(&r, &Method::Get, "/hello");
// }

#[test]
fn literal_beats_param() {
    let mut r = new_router();
    add_routes(&mut r, &Method::Get, &[("/users/me", 0), ("/users/:id", 1)]);
    let r = r.build();

    assert_match(&r, &Method::Get, "/users/me", 0);
    assert_match_params(&r, &Method::Get, "/users/42", 1, &[("id", "42")]);
}

#[test]
fn param_beats_wildcard() {
    let mut r = new_router();
    add_routes(
        &mut r,
        &Method::Get,
        &[("/files/:name", 0), ("/files/*", 1)],
    );
    let r = r.build();

    assert_match_params(&r, &Method::Get, "/files/readme", 0, &[("name", "readme")]);
}

#[test]
fn longest_literal_then_precedence() {
    // /a/b/*  -> LML=2
    // /a/*/c  -> LML=1 but ends with literal:
    //                  despite higher precedence it gets filtered out due to lower LML
    let mut r = new_router();
    add_routes(&mut r, &Method::Get, &[("/a/b/*", 0), ("/a/*/c", 1)]);
    let r = r.build();

    assert_match(&r, &Method::Get, "/a/b/c", 0);
}

#[test]
fn many_params_extraction() {
    let mut r = new_router();
    add_routes(&mut r, &Method::Get, &[("/:a/:b/:c", 0)]);
    let r = r.build();

    assert_match_params(
        &r,
        &Method::Get,
        "/one/two/three",
        0,
        &[("a", "one"), ("b", "two"), ("c", "three")],
    );
}

#[test]
fn root_double_wildcard_fallback() {
    let mut r = new_router();
    add_routes(&mut r, &Method::Get, &[("/api/*", 0), ("/**", 1)]);
    let r = r.build();

    assert_match(&r, &Method::Get, "/whatever/else", 1);
    assert_match(&r, &Method::Get, "/api/foo", 0);
}

#[test]
fn method_isolation() {
    let mut r = new_router();
    add_routes(&mut r, &Method::Get, &[("/users/:id", 0)]);
    add_routes(&mut r, &Method::Post, &[("/users/:id", 1)]);
    let r = r.build();

    assert_match_params(&r, &Method::Get, "/users/10", 0, &[("id", "10")]);
    assert_match_params(&r, &Method::Post, "/users/10", 1, &[("id", "10")]);
}

// #[test]
// fn remove_param_route() {
//     let mut r = new_router();
//     r.add_route(&Method::Get, "/users/:id", (99, "/users/:id"));
//     let r = r.build();
//
//     assert_match_params(&r, &Method::Get, "/users/5", 99, &[("id", "5")]);
//     let removed = r.remove_route(&Method::Get, "/users/:id").unwrap();
//     assert_eq!(removed.0, 99);
//     assert_404(&r, &Method::Get, "/users/5");
// }

#[test]
fn overlap_param_and_double_wildcard() {
    let mut r = new_router();
    add_routes(&mut r, &Method::Get, &[("/blog/:slug", 0), ("/blog/**", 1)]);
    let r = r.build();

    // single segment -> param wins
    assert_match_params(&r, &Method::Get, "/blog/intro", 0, &[("slug", "intro")]);

    // deeper path -> ** wins
    assert_match(&r, &Method::Get, "/blog/2024/10/interesting", 1);
}

#[test]
fn test_trailing_slash() {
    // spec says: separate route
    let mut r = new_router();
    add_routes(&mut r, &Method::Get, &[("/hello", 0)]);
    add_routes(&mut r, &Method::Get, &[("/foo/bar", 1)]);
    add_routes(&mut r, &Method::Get, &[("/foo/bar/", 2)]);
    add_routes(&mut r, &Method::Get, &[("/wild/*", 3)]);
    add_routes(&mut r, &Method::Get, &[("/user/:id", 4)]);
    add_routes(&mut r, &Method::Get, &[("/any/**", 5)]);
    let r = r.build();

    assert_404(&r, &Method::Get, "/hello/");
    assert_match(&r, &Method::Get, "/foo/bar", 1);
    assert_match(&r, &Method::Get, "/foo/bar/", 2);
    assert_404(&r, &Method::Get, "/foo/bar//");
    assert_404(&r, &Method::Get, "/foo/bar//");
    assert_404(&r, &Method::Get, "/wild/abc/");
    assert_404(&r, &Method::Get, "/user/123/");
    assert_match(&r, &Method::Get, "/any////", 5);
}

#[test]
fn test_multiple_slashes() {
    // spec says: separate route
    let mut r = new_router();
    add_routes(&mut r, &Method::Get, &[("/hello", 0)]);
    add_routes(&mut r, &Method::Get, &[("/foo/bar", 1)]);
    let r = r.build();

    assert_404(&r, &Method::Get, "///hello");
    assert_404(&r, &Method::Get, "///hello///");
    assert_404(&r, &Method::Get, "/foo//bar");
    assert_404(&r, &Method::Get, "//////foo////bar");
}

// ---------------------------------------------------------------------
// UTILS
// ---------------------------------------------------------------------

type MockRouterBuilder = RouterBuilder<(usize, &'static str)>;
type MockRouter = Router<(usize, &'static str)>;

fn new_router() -> MockRouterBuilder {
    MockRouterBuilder::new((404, "/404"))
}

fn add_routes(router: &mut MockRouterBuilder, method: &Method, routes: &[(&'static str, usize)]) {
    for (pat, id) in routes {
        router.add_route(method, pat, (*id, *pat));
    }
}

fn assert_match(router: &MockRouter, method: &Method, uri: &str, expected_idx: usize) {
    let m = router.match_route(method, uri);
    assert_eq!(m.route.0, expected_idx, "URI: {}", uri);
}

fn assert_match_params(
    router: &MockRouter,
    method: &Method,
    uri: &str,
    expected_idx: usize,
    expected_params: &[(&str, &str)],
) {
    let m = router.match_route(method, uri);
    assert_eq!(m.route.0, expected_idx, "URI: {}", uri);
    for (k, v) in expected_params {
        assert_eq!(
            m.params.get(k).unwrap(),
            *v,
            "param '{}' mismatch for {}",
            k,
            uri
        );
    }
}

fn assert_404(router: &MockRouter, method: &Method, uri: &str) {
    assert!(
        router.match_route(method, uri).route.0 == 404,
        "expected 404 for URI {}",
        uri
    );
}
