use khttp::RequestUri;

#[test]
fn test_absolute_uri_full_parts() {
    let uri = RequestUri::from("https://example.com:8080/path/to/resource?foo=bar&baz=qux#section");
    assert_eq!(uri.scheme(), Some("https"));
    assert_eq!(uri.authority(), Some("example.com:8080"));
    assert_eq!(uri.path(), "/path/to/resource");
    assert_eq!(uri.query(), Some("foo=bar&baz=qux"));
    assert_eq!(uri.fragment(), Some("section"));
}

#[test]
fn test_absolute_uri_no_query() {
    let uri = RequestUri::from("http://example.com/path#frag");
    assert_eq!(uri.scheme(), Some("http"));
    assert_eq!(uri.authority(), Some("example.com"));
    assert_eq!(uri.path(), "/path");
    assert_eq!(uri.query(), None);
    assert_eq!(uri.fragment(), Some("frag"));
}

#[test]
fn test_absolute_uri_no_fragment() {
    let uri = RequestUri::from("http://example.com/path?foo=bar");
    assert_eq!(uri.scheme(), Some("http"));
    assert_eq!(uri.authority(), Some("example.com"));
    assert_eq!(uri.path(), "/path");
    assert_eq!(uri.query(), Some("foo=bar"));
    assert_eq!(uri.fragment(), None);
}

#[test]
fn test_absolute_uri_only_authority() {
    let uri = RequestUri::from("https://example.com");
    assert_eq!(uri.scheme(), Some("https"));
    assert_eq!(uri.authority(), Some("example.com"));
    assert_eq!(uri.path(), "/");
    assert_eq!(uri.query(), None);
    assert_eq!(uri.fragment(), None);
}

#[test]
fn test_relative_uri_with_query_and_fragment() {
    let uri = RequestUri::from("/api/data?key=value#top");
    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.authority(), None);
    assert_eq!(uri.path(), "/api/data");
    assert_eq!(uri.query(), Some("key=value"));
    assert_eq!(uri.fragment(), Some("top"));
}

#[test]
fn test_relative_uri_only_path() {
    let uri = RequestUri::from("/just/path");
    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.authority(), None);
    assert_eq!(uri.path(), "/just/path");
    assert_eq!(uri.query(), None);
    assert_eq!(uri.fragment(), None);
}

#[test]
fn test_fragment_before_query_should_be_parsed_correctly() {
    let uri = RequestUri::from("/path#frag?these=are-not-params");
    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.authority(), None);
    assert_eq!(uri.path(), "/path");
    assert_eq!(uri.query(), None);
    assert_eq!(uri.fragment(), Some("frag?these=are-not-params"));
}

#[test]
fn test_authority_form() {
    let uri = RequestUri::from("example.com:443");
    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.authority(), None); // correctly interpreted as path
    assert_eq!(uri.path(), "example.com:443");
    assert_eq!(uri.query(), None);
    assert_eq!(uri.fragment(), None);
}

#[test]
fn test_asterisk_form() {
    let uri = RequestUri::from("*");
    assert_eq!(uri.scheme(), None);
    assert_eq!(uri.authority(), None);
    assert_eq!(uri.path(), "*");
    assert_eq!(uri.query(), None);
    assert_eq!(uri.fragment(), None);
}
