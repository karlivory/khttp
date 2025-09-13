fn make(uri_str: &str) -> khttp::RequestUri<'static> {
    let request = format!("GET {uri_str} HTTP/1.1\r\nHost: foo\r\n\r\n");
    let request = Box::leak(request.into_boxed_str());
    let request = khttp::Request::parse(request.as_bytes()).expect("should parse");
    let uri = request.uri;
    assert_eq!(uri.as_str(), uri_str);
    uri
}

// ---------------------------------------------------------------------
// absolute form
// ---------------------------------------------------------------------

#[test]
fn absolute_form_with_query() {
    let uri_str = "http://example.com/foo/bar?x=1&y=2";
    let uri = make(uri_str);

    assert_eq!(uri.as_str(), uri_str);
    assert_eq!(uri.scheme(), Some("http"));
    assert_eq!(uri.authority(), Some("example.com"));
    assert_eq!(uri.path(), "/foo/bar");
    assert_eq!(uri.path_and_query(), "/foo/bar?x=1&y=2");
    assert_eq!(uri.query(), Some("x=1&y=2"));
}

#[test]
fn absolute_form_no_query() {
    let uri_str = "https://example.org/just/path";
    let uri = make(uri_str);

    assert_eq!(uri.scheme(), Some("https"));
    assert_eq!(uri.authority(), Some("example.org"));
    assert_eq!(uri.path(), "/just/path");
    assert_eq!(uri.path_and_query(), "/just/path");
    assert_eq!(uri.query(), None);
}

#[test]
fn absolute_form_with_port_and_query() {
    let uri_str = "http://example.com:8080/foo?bar=baz";
    let uri = make(uri_str);

    assert_eq!(uri.scheme(), Some("http"));
    assert_eq!(uri.authority(), Some("example.com:8080"));
    assert_eq!(uri.path(), "/foo");
    assert_eq!(uri.path_and_query(), "/foo?bar=baz");
    assert_eq!(uri.query(), Some("bar=baz"));
}

#[test]
fn absolute_form_no_path() {
    let uri_str = "http://example.com";
    let uri = make(uri_str);

    assert_eq!(uri.scheme(), Some("http"));
    assert_eq!(uri.authority(), Some("example.com"));
    assert_eq!(uri.path(), "");
    assert_eq!(uri.path_and_query(), "");
    assert_eq!(uri.query(), None);
}

#[test]
fn absolute_form_root_path() {
    let uri_str = "http://example.com/";
    let uri = make(uri_str);

    assert_eq!(uri.scheme(), Some("http"));
    assert_eq!(uri.authority(), Some("example.com"));
    assert_eq!(uri.path(), "/");
    assert_eq!(uri.path_and_query(), "/");
    assert_eq!(uri.query(), None);
}

#[test]
fn absolute_form_no_path_with_query() {
    let uri_str = "http://example.com?x=1";
    let uri = make(uri_str);

    assert_eq!(uri.scheme(), Some("http"));
    assert_eq!(uri.authority(), Some("example.com"));
    assert_eq!(uri.path(), "");
    assert_eq!(uri.path_and_query(), "?x=1");
    assert_eq!(uri.query(), Some("x=1"));
}

// ---------------------------------------------------------------------
// origin-form
// ---------------------------------------------------------------------

#[test]
fn origin_form_empty_query() {
    let uri_str = "/foo?";
    let uri = make(uri_str);

    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.authority(), None);
    assert_eq!(uri.path(), "/foo");
    assert_eq!(uri.path_and_query(), "/foo?");
    assert_eq!(uri.query(), Some("")); // empty query is present
}

#[test]
fn origin_form_with_query() {
    let uri_str = "/api/v1/resources?id=42&verbose=true";
    let uri = make(uri_str);

    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.authority(), None);
    assert_eq!(uri.path(), "/api/v1/resources");
    assert_eq!(uri.path_and_query(), "/api/v1/resources?id=42&verbose=true");
    assert_eq!(uri.query(), Some("id=42&verbose=true"));
}

#[test]
fn origin_form_no_query() {
    let uri_str = "/healthz";
    let uri = make(uri_str);

    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.authority(), None);
    assert_eq!(uri.path(), "/healthz");
    assert_eq!(uri.path_and_query(), "/healthz");
    assert_eq!(uri.query(), None);
}

#[test]
fn origin_form_root() {
    let uri_str = "/";
    let uri = make(uri_str);

    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.authority(), None);
    assert_eq!(uri.path(), "/");
    assert_eq!(uri.path_and_query(), "/");
    assert_eq!(uri.query(), None);
}

// ---------------------------------------------------------------------
// authority-form
// ---------------------------------------------------------------------

#[test]
fn authority_form_host_only() {
    let uri_str = "example.com";
    let uri = make(uri_str);

    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.authority(), Some("example.com"));
    assert_eq!(uri.path(), "");
    assert_eq!(uri.path_and_query(), "");
    assert_eq!(uri.query(), None);
}

#[test]
fn authority_form_host_port() {
    let uri_str = "example.com:443";
    let uri = make(uri_str);

    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.authority(), Some("example.com:443"));
    assert_eq!(uri.path(), "");
    assert_eq!(uri.path_and_query(), "");
    assert_eq!(uri.query(), None);
}

#[test]
fn authority_form_ipv6() {
    let uri_str = "[::1]:443";
    let uri = make(uri_str);

    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.authority(), Some("[::1]:443"));
    assert_eq!(uri.path(), "");
    assert_eq!(uri.path_and_query(), "");
    assert_eq!(uri.query(), None);
}

#[test]
fn test_display_trait() {
    let uri_str = "example.com:8080";
    let uri = make(uri_str);
    let formatted = format!("{uri}");

    assert_eq!(formatted, uri_str);
}
