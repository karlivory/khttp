use khttp::{Headers, HttpPrinter, Method, Status};
use std::io::{Cursor, Read, Write};

// ---------------------------------------------------------------------
// RESPONSES
// ---------------------------------------------------------------------

#[test]
fn test_response_with_content_length() {
    let mut headers = Headers::new();
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
        Headers::new(),
        "tiny",
    );
}

#[test]
fn test_response_chunked_explicit_te() {
    let mut headers = Headers::new();
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
    let w = capture_response(Status::OK, Headers::new(), &body[..]);
    assert!(w.contains("transfer-encoding: chunked"));
    assert!(!w.contains("content-length"));
}

#[test]
fn test_large_response_cl_no_auto_te() {
    let body = b"hello".repeat(3000);
    let mut headers = Headers::new();
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
        let mut printer = HttpPrinter::new(&mut w);
        printer.write_100_continue().unwrap();
        printer.flush().unwrap();
    }

    assert_eq!(w.as_str(), "HTTP/1.1 100 Continue\r\n\r\n");
}

// ---------------------------------------------------------------------
// REQUESTS
// ---------------------------------------------------------------------

#[test]
fn test_request_with_content_length() {
    let headers = headers_with_content_length(4);
    assert_print_request(
        b"POST /api HTTP/1.1\r\ncontent-length: 4\r\n\r\ntest",
        Method::Post,
        "/api",
        headers,
        "test",
    );
}

#[test]
fn test_request_with_te() {
    let mut headers = Headers::new();
    headers.set_transfer_encoding_chunked();
    assert_print_request(
        b"POST /api HTTP/1.1\r\ntransfer-encoding: chunked\r\n\r\n4\r\ntest\r\n0\r\n\r\n",
        Method::Post,
        "/api",
        headers,
        "test",
    );
}

// ---------------------------------------------------------------------
// UTILS
// ---------------------------------------------------------------------

fn headers_with_content_length(len: u64) -> Headers<'static> {
    let mut h = Headers::new();
    h.set_content_length(Some(len));
    h
}

fn assert_print_response(expected: &[u8], status: Status, headers: Headers, body: &str) {
    let got = capture_response(status, headers, Cursor::new(body));
    let expected = String::from_utf8_lossy(expected);
    assert_eq!(got, expected);
}

fn assert_print_request(expected: &[u8], method: Method, uri: &str, headers: Headers, body: &str) {
    let got = capture_request(method, uri, &headers, Cursor::new(body));
    let expected = String::from_utf8_lossy(expected);
    assert_eq!(got, expected);
}

fn capture_request(method: Method, uri: &str, headers: &Headers, body: impl Read) -> String {
    let mut w = MockWriter::new();
    {
        let mut printer = HttpPrinter::new(&mut w);
        printer
            .write_request(&method, uri, headers, body)
            .expect("should print");
        printer.flush().unwrap();
    }
    w.into_string()
}

fn capture_response(status: Status, headers: Headers, body: impl Read) -> String {
    let mut w = MockWriter::new();
    {
        let mut printer = HttpPrinter::new(&mut w);
        printer.write_response(&status, &headers, body).unwrap();
        printer.flush().unwrap();
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
