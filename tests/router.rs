use khttp::{Method, Method::*, Router, RouterBuilder};

// ---------------------------------------------------------------------
// TESTS
// ---------------------------------------------------------------------

#[test]
fn empty_route() {
    let r = new_router(&[(Get, "/", 0), (Get, "/route", 1)]);
    assert_match(&r, Get, "/", 0);
}

#[test]
fn nested_routes() {
    let r = new_router(&[(Get, "/route", 0), (Get, "/route/foo", 1)]);

    assert_match(&r, Get, "/route", 0);
    assert_match(&r, Get, "/route/foo", 1);
}

#[test]
fn wildcard() {
    let r = new_router(&[
        (Get, "/route1/*/foo", 0),
        (Get, "/route2/*", 1),
        (Get, "/route2/hey", 2),
    ]);

    assert_match(&r, Get, "/route1/abc/foo", 0);
    assert_match(&r, Get, "/route1/d/foo", 0);
    assert_match(&r, Get, "/route2/hello", 1);
    assert_match(&r, Get, "/route2/hey", 2);
}

#[test]
fn double_wildcard() {
    let r = new_router(&[
        (Get, "/route1/**", 0),
        (Get, "/route2/*", 1),
        (Get, "/route2", 2),
        (Get, "/**", 3),
    ]);

    assert_match(&r, Get, "/route1/abc/def/hjk", 0);
    assert_match(&r, Get, "/route1/d/foo", 0);
    assert_match(&r, Get, "/route2/hello", 1);
    assert_match(&r, Get, "/route2", 2);
    assert_match(&r, Get, "/", 3);
    assert_match(&r, Get, "/route3", 3);
    assert_match(&r, Get, "/fallback/for/any/other/route", 3);
}

#[test]
fn precedence() {
    let r = new_router(&[
        (Get, "/route/foo", 0),
        (Get, "/route/*", 1),
        (Get, "/route/**", 2),
    ]);

    assert_match(&r, Get, "/route/foo", 0);
    assert_match(&r, Get, "/route/hello", 1);
    assert_match(&r, Get, "/route/foobar", 1);
    assert_match(&r, Get, "/route/foo/bar/index.html", 2);
}

#[test]
fn hashing_duplicate_routes() {
    let r = new_router(&[
        (Get, "/route/foo", 0),
        (Get, "/route/foo", 1),
        (Get, "/route/foo2", 2),
        (Get, "/route/*", 3),
        (Get, "/route/*", 4),
        (Get, "/user/:id", 5),
        (Get, "/user/:slug", 6),
    ]);

    // Last inserted 'wins' for identical routes
    assert_match(&r, Get, "/route/foo", 1);
    assert_match(&r, Get, "/route/foo2", 2);
    assert_match(&r, Get, "/route/hey", 4);
    assert_match(&r, Get, "/user/1234", 6);
}

#[test]
fn params_extraction() {
    let r = new_router(&[
        (Get, "/users/:id", 0),
        (Get, "/users/:id/posts/:post_id", 1),
    ]);

    assert_match_params(&r, Get, "/users/12", 0, &[("id", "12")]);
    assert_match_params(
        &r,
        Get,
        "/users/42/posts/abc",
        1,
        &[("id", "42"), ("post_id", "abc")],
    );
}

#[test]
fn literal_beats_param() {
    let r = new_router(&[(Get, "/users/me", 0), (Get, "/users/:id", 1)]);

    assert_match(&r, Get, "/users/me", 0);
    assert_match_params(&r, Get, "/users/42", 1, &[("id", "42")]);
}

#[test]
fn param_beats_wildcard() {
    let r = new_router(&[(Get, "/files/:name", 0), (Get, "/files/*", 1)]);

    assert_match_params(&r, Get, "/files/readme", 0, &[("name", "readme")]);
}

#[test]
fn longest_literal_then_precedence() {
    // /a/b/*  -> LML=2
    // /a/*/c  -> LML=1 but ends with literal:
    //                  despite higher precedence it gets filtered out due to lower LML
    let r = new_router(&[(Get, "/a/b/*", 0), (Get, "/a/*/c", 1)]);

    assert_match(&r, Get, "/a/b/c", 0);
}

#[test]
fn many_params_extraction() {
    let r = new_router(&[(Get, "/:a/:b/:c", 0)]);

    assert_match_params(
        &r,
        Get,
        "/one/two/three",
        0,
        &[("a", "one"), ("b", "two"), ("c", "three")],
    );
}

#[test]
fn root_double_wildcard_fallback() {
    let r = new_router(&[(Get, "/api/*", 0), (Get, "/**", 1)]);

    assert_match(&r, Get, "/whatever/else", 1);
    assert_match(&r, Get, "/api/foo", 0);
}

#[test]
fn method_isolation() {
    let r = new_router(&[(Get, "/users/:id", 0), (Post, "/users/:id", 1)]);

    assert_match_params(&r, Get, "/users/10", 0, &[("id", "10")]);
    assert_match_params(&r, Post, "/users/10", 1, &[("id", "10")]);
}

#[test]
fn overlap_param_and_double_wildcard() {
    let r = new_router(&[(Get, "/blog/:slug", 0), (Get, "/blog/**", 1)]);

    // single segment -> param wins
    assert_match_params(&r, Get, "/blog/intro", 0, &[("slug", "intro")]);

    // deeper path -> ** wins
    assert_match(&r, Get, "/blog/2024/10/interesting", 1);
}

#[test]
fn test_trailing_slash() {
    let r = new_router(&[
        (Get, "/hello", 0),
        (Get, "/foo/bar", 1),
        (Get, "/foo/bar/", 2),
        (Get, "/wild/*", 3),
        (Get, "/user/:id", 4),
        (Get, "/any/**", 5),
    ]);

    assert_404(&r, Get, "/hello/");
    assert_match(&r, Get, "/foo/bar", 1);
    assert_match(&r, Get, "/foo/bar/", 2);
    assert_404(&r, Get, "/foo/bar//");
    assert_404(&r, Get, "/foo/bar//");
    assert_404(&r, Get, "/wild/abc/");
    assert_404(&r, Get, "/user/123/");
    assert_match(&r, Get, "/any////", 5);
}

#[test]
fn test_multiple_slashes() {
    let r = new_router(&[(Get, "/hello", 0), (Get, "/foo/bar", 1)]);

    assert_404(&r, Get, "///hello");
    assert_404(&r, Get, "///hello///");
    assert_404(&r, Get, "/foo//bar");
    assert_404(&r, Get, "//////foo////bar");
}

// ---------------------------------------------------------------------
// UTILS
// ---------------------------------------------------------------------

type MockRouter = Router<(usize, &'static str)>;
type RouteSpec = (Method, &'static str, usize);

fn new_router(routes: &[RouteSpec]) -> MockRouter {
    let mut b = RouterBuilder::new((404, "/404"));
    for (m, pat, id) in routes {
        b.add_route(m, pat, (*id, *pat));
    }
    b.build()
}

fn assert_match(router: &MockRouter, method: Method, uri: &str, expected_idx: usize) {
    let m = router.match_route(&method, uri);
    assert_eq!(m.route.0, expected_idx, "URI: {uri}");
}

fn assert_match_params(
    router: &MockRouter,
    method: Method,
    uri: &str,
    expected_idx: usize,
    expected_params: &[(&str, &str)],
) {
    let m = router.match_route(&method, uri);
    assert_eq!(m.route.0, expected_idx, "URI: {}", uri);
    for (k, v) in expected_params {
        assert_eq!(
            m.params.get(k).unwrap(),
            *v,
            "param '{k}' mismatch for {uri}",
        );
    }
}

fn assert_404(router: &MockRouter, method: Method, uri: &str) {
    assert!(
        router.match_route(&method, uri).route.0 == 404,
        "expected 404 for URI {uri}",
    );
}
