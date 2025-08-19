use khttp::{Headers, HttpPrinter, Status};
use std::io::{Cursor, Read, Write};

// ---------------------------------------------------------------------
// RESPONSES
// ---------------------------------------------------------------------

#[test]
fn test_response_with_content_length() {
    let mut headers = Headers::new_nodate();
    headers.set_content_length(Some(5));
    assert_print_response(
        b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\n\r\nhello",
        Status::OK,
        headers,
        "hello",
    );
}

#[test]
fn test_response_auto_content_length_small_body() {
    assert_print_response(
        b"HTTP/1.1 200 OK\r\ncontent-length: 4\r\n\r\ntiny",
        Status::OK,
        Headers::new_nodate(),
        "tiny",
    );
}

#[test]
fn test_response_chunked_explicit_te() {
    let mut headers = Headers::new_nodate();
    headers.set_transfer_encoding_chunked();

    assert_print_response(
        b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n4\r\ndata\r\n0\r\n\r\n",
        Status::OK,
        headers,
        "data",
    );
}

#[test]
fn test_large_response_auto_te() {
    let body = b"hello".repeat(3000);
    let w = capture_response(Status::OK, Headers::new_nodate(), &body[..]);
    assert!(w.contains("transfer-encoding: chunked"));
    assert!(!w.contains("content-length"));
}

#[test]
fn test_large_response_cl_no_auto_te() {
    let body = b"hello".repeat(3000);
    let mut headers = Headers::new_nodate();
    let cl = body.len() as u64;
    headers.set_content_length(Some(cl));
    let w = capture_response(Status::OK, headers, &body[..]);
    assert!(!w.contains("transfer-encoding"));
    assert!(w.contains(&format!("content-length: {cl}")));
}

#[test]
fn test_100_continue() {
    let mut w = MockWriter::new();
    {
        HttpPrinter::write_100_continue(&mut w).expect("should print");
    }

    assert_eq!(w.as_str(), "HTTP/1.1 100 Continue\r\n\r\n");
}

// ---------------------------------------------------------------------
// REQUESTS
// ---------------------------------------------------------------------

#[cfg(feature = "client")]
#[test]
fn test_request_with_content_length() {
    use khttp::Method;
    let mut headers = Headers::new_nodate();
    headers.set_content_length(Some(4));
    assert_print_request(
        b"POST /api HTTP/1.1\r\ncontent-length: 4\r\n\r\ntest",
        Method::Post,
        "/api",
        headers,
        "test",
    );
}

#[cfg(feature = "client")]
#[test]
fn test_request_with_te() {
    use khttp::Method;
    let mut headers = Headers::new_nodate();
    headers.set_transfer_encoding_chunked();
    assert_print_request(
        b"POST /api HTTP/1.1\r\ntransfer-encoding: chunked\r\n\r\n4\r\ntest\r\n0\r\n\r\n",
        Method::Post,
        "/api",
        headers,
        "test",
    );
}

#[test]
fn test_write_response0() {
    let mut headers = Headers::new_nodate();
    headers.add("foo", b"bar");
    let mut w = MockWriter::new();
    {
        HttpPrinter::write_response0(&mut w, &Status::OK, &headers).unwrap();
    }
    assert_eq!(
        w.as_str(),
        "HTTP/1.1 200 OK\r\nfoo: bar\r\ncontent-length: 0\r\n\r\n"
    );
}

// ---------------------------------------------------------------------
// UTILS
// ---------------------------------------------------------------------

fn assert_print_response(expected: &[u8], status: Status, headers: Headers, body: &str) {
    let got = capture_response(status, headers, Cursor::new(body));
    let expected = String::from_utf8_lossy(expected);
    assert_eq!(got, expected);
}

#[cfg(feature = "client")]
fn assert_print_request(
    expected: &[u8],
    method: khttp::Method,
    uri: &str,
    headers: Headers,
    body: &str,
) {
    let got = capture_request(method, uri, &headers, Cursor::new(body));
    let expected = String::from_utf8_lossy(expected);
    assert_eq!(got, expected);
}

#[cfg(feature = "client")]
fn capture_request(method: khttp::Method, uri: &str, headers: &Headers, body: impl Read) -> String {
    let mut w = MockWriter::new();
    {
        HttpPrinter::write_request(&mut w, &method, uri, headers, body).expect("should print");
    }
    w.into_string()
}

fn capture_response(status: Status, headers: Headers, body: impl Read) -> String {
    let mut w = MockWriter::new();
    {
        HttpPrinter::write_response(&mut w, &status, &headers, body).unwrap();
    }
    w.into_string()
}

struct MockWriter {
    buf: Vec<u8>,
}

impl MockWriter {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf).unwrap()
    }

    fn into_string(self) -> String {
        String::from_utf8(self.buf).unwrap()
    }
}

impl Write for MockWriter {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
