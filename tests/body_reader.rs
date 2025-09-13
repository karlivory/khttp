use khttp::BodyReader;

#[test]
fn test_chunked_simple() {
    let body = read_chunked(
        "\
        5\r\n\
        Hello\r\n\
        6\r\n\
        , Worl\r\n\
        1\r\n\
        d\r\n\
        0\r\n\
        \r\n",
    );
    assert_eq!(body, "Hello, World");
}

#[test]
fn test_chunked_empty_body() {
    assert_eq!(read_chunked("0\r\n\r\n"), "");
}

#[test]
fn test_chunked_with_trailers() {
    // trailers are ignored (TODO?)
    let body = read_chunked(
        "\
        5\r\nHello\r\n\
        7\r\n world!\r\n\
        0\r\n\
        X-Trailer: yes\r\n\
        \r\n",
    );
    assert_eq!(body, "Hello world!");
}

#[test]
fn test_reader_fixed() {
    let leftover = b"Hello, w";
    let stream = b"orld";
    let mut body = BodyReader::new_fixed(leftover, &stream[..], 12);

    assert_eq!(body.string().unwrap(), "Hello, world");
}

#[test]
fn test_reader_chunked() {
    let leftover = b"5\r\nHe";
    let stream = b"llo\r\n6\r\n, worl\r\n1\r\nd\r\n0\r\n\r\n";
    let mut body = BodyReader::new_chunked(leftover, &stream[..]);

    assert_eq!(body.string().unwrap(), "Hello, world");
}

#[test]
fn test_reader_empty() {
    let stream = b"Hello, world"; // should not read body!
    let mut body = BodyReader::new_empty(&stream[..]);

    assert_eq!(body.string().unwrap(), "");
}

#[test]
fn test_reader_eof() {
    let leftover = b"Hello, w";
    let stream = b"orld"; // should not read body!
    let mut body = BodyReader::new_eof(leftover, &stream[..]);

    assert_eq!(body.string().unwrap(), "Hello, world");
}

// ---------------------------------------------------------------------
// UTILS
// ---------------------------------------------------------------------

fn read_chunked(input: &'static str) -> String {
    let mut body = BodyReader::new_chunked(&[], input.as_bytes());
    body.string().expect("should read")
}
