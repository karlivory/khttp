use khttp::Headers;

#[test]
fn test_add_and_get() {
    let mut headers = Headers::new();
    headers.add("Some-Header", b"Hello, World!");

    let value = std::str::from_utf8(headers.get("some-header").unwrap()).unwrap();
    assert_eq!(value, "Hello, World!");
}

#[test]
fn test_replace() {
    let mut headers = Headers::new();
    headers.add("Some-Header", b"old value");
    headers.replace("Some-Header", b"new value");

    let value = std::str::from_utf8(headers.get("some-header").unwrap()).unwrap();
    assert_eq!(value, "new value");
}

#[test]
fn test_remove() {
    let mut headers = Headers::new();
    headers.add("Some-Header", b"value 1");
    headers.add("Some-Header", b"value 2");
    assert!(headers.get("some-header").is_some());
    headers.remove("Some-Header");
    assert!(headers.get("some-header").is_none());
}

#[test]
fn test_transfer_encoding_is_set() {
    let mut headers = Headers::new();
    headers.set_transfer_encoding_chunked();
    let values = std::str::from_utf8(headers.get_transfer_encoding()).unwrap();

    assert!(headers.is_transfer_encoding_chunked());
    assert_eq!(values, "chunked");

    let mut headers = Headers::new();
    headers.add("Transfer-Encoding", b"chunked");
    let values = std::str::from_utf8(headers.get_transfer_encoding()).unwrap();

    assert!(headers.is_transfer_encoding_chunked());
    assert_eq!(values, "chunked");
}

#[test]
fn test_transfer_encoding_multiple_values() {
    let mut headers = Headers::new();
    headers.add("Transfer-Encoding", b"gzip, deflate");
    headers.add("Transfer-Encoding", b"other");
    let values = std::str::from_utf8(headers.get_transfer_encoding()).unwrap();

    assert!(!headers.is_transfer_encoding_chunked());
    assert_eq!(values, "gzip, deflate, other");
}

#[test]
fn test_connection_is_set() {
    let mut headers = Headers::new();
    headers.set_connection_close();
    let values = std::str::from_utf8(headers.get_connection_values()).unwrap();

    assert!(headers.is_connection_close());
    assert_eq!(values, "close");

    let mut headers = Headers::new();
    headers.add("Connection", b"close");
    let values = std::str::from_utf8(headers.get_connection_values()).unwrap();

    assert!(headers.is_connection_close());
    assert_eq!(values, "close");
}

#[test]
fn test_connection_multiple_values() {
    let mut headers = Headers::new();
    headers.add("Connection", b"keep-alive");
    headers.add("Connection", b"upgrade");
    let values = std::str::from_utf8(headers.get_connection_values()).unwrap();

    assert!(!headers.is_connection_close());
    assert_eq!(values, "keep-alive, upgrade");
}
