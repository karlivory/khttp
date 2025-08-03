use khttp::BodyReader;
use std::io::Read;

#[test]
fn test_chunked_simple() {
    assert_chunked_body(
        b"\
        5\r\n\
        Hello\r\n\
        6\r\n\
        , worl\r\n\
        1\r\n\
        d\r\n\
        0\r\n\
        \r\n",
        "Hello, world",
    );
}

#[test]
fn test_chunked_empty_body() {
    assert_chunked_body(b"0\r\n\r\n", "");
}

#[test]
fn test_chunked_with_trailers() {
    // trailers are ignored (TODO?)
    assert_chunked_body(
        b"\
        5\r\nHello\r\n\
        6\r\n World\r\n\
        0\r\n\
        X-Trailer: yes\r\n\
        \r\n",
        "Hello World",
    );
}

#[test]
fn test_prefix() {
    let prefix = b"5\r\nHe";
    let stream = b"llo\r\n6\r\n, worl\r\n1\r\nd\r\n0\r\n\r\n";
    assert_chunked_body_with_prefix(prefix, stream, "Hello, world");
}

// ---------------------------------------------------------------------
// UTILS
// ---------------------------------------------------------------------

fn assert_chunked_body(input: &'static [u8], expected: &str) {
    let mut cursor = std::io::Cursor::new(input);
    let mut body = BodyReader::new_chunked(&[], &mut cursor);
    let mut buf = String::new();

    match body.read_to_string(&mut buf) {
        Ok(_) => assert_eq!(
            buf, expected,
            "\n--- MISMATCH ---\nExpected: {:?}\nActual:   {:?}\n",
            expected, buf
        ),
        Err(e) => panic!("Failed to read chunked body: {e}"),
    }
}

fn assert_chunked_body_with_prefix(prefix: &[u8], stream: &[u8], expected: &str) {
    let mut cursor = std::io::Cursor::new(stream);
    let mut body = BodyReader::new_chunked(prefix, &mut cursor);
    let mut buf = String::new();
    body.read_to_string(&mut buf).unwrap();
    assert_eq!(buf, expected);
}
