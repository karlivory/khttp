use khttp::{Headers, HttpPrinter, Status};
#[cfg(feature = "client")]
use khttp::{Method, Method::*};
use std::io::{Read, Write};

// ---------------------------------------------------------------------
// RESPONSES
// ---------------------------------------------------------------------

#[test]
fn test_response_with_content_length() {
    let mut headers = Headers::new_nodate();
    headers.set_content_length(Some(5));

    assert_eq!(
        print_response(Status::OK, headers, &b"hello"[..]),
        "HTTP/1.1 200 OK\r\ncontent-length: 5\r\n\r\nhello",
    );
}

#[test]
fn test_response_auto_content_length_small_body() {
    assert_eq!(
        print_response(Status::OK, Headers::new_nodate(), &b"tiny"[..]),
        "HTTP/1.1 200 OK\r\ncontent-length: 4\r\n\r\ntiny",
    );
}

#[test]
fn test_response_chunked_explicit_te() {
    let mut headers = Headers::new_nodate();
    headers.set_transfer_encoding_chunked();

    assert_eq!(
        print_response(Status::OK, headers, &b"data"[..]),
        "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n4\r\ndata\r\n0\r\n\r\n",
    );
}

#[test]
fn test_large_response_auto_te() {
    let body = b"hello".repeat(3000);
    let w = print_response(Status::OK, Headers::new_nodate(), &body[..]);

    assert!(w.contains("transfer-encoding: chunked"));
    assert!(!w.contains("content-length"));
}

#[test]
fn test_large_response_cl_no_auto_te() {
    let body = b"hello".repeat(3000);
    let mut headers = Headers::new_nodate();
    let cl = body.len() as u64;
    headers.set_content_length(Some(cl));
    let response = print_response(Status::OK, headers, &body[..]);

    assert!(!response.contains("transfer-encoding"));
    assert!(response.contains(&format!("content-length: {cl}")));
}

#[test]
fn test_write_response_empty() {
    let mut headers = Headers::new_nodate();
    headers.add("foo", b"bar");
    let mut w = MockWriter::new();
    HttpPrinter::write_response_empty(&mut w, &Status::OK, &headers).unwrap();

    assert_eq!(
        w.as_str(),
        "HTTP/1.1 200 OK\r\nfoo: bar\r\ncontent-length: 0\r\n\r\n"
    );
}

#[test]
fn test_write_response_empty_chunked() {
    let mut headers = Headers::new_nodate();
    headers.add("foo", b"bar");
    headers.set_transfer_encoding_chunked();
    let mut w = MockWriter::new();
    HttpPrinter::write_response_empty(&mut w, &Status::OK, &headers).unwrap();

    assert_eq!(
        w.as_str(),
        "HTTP/1.1 200 OK\r\nfoo: bar\r\ntransfer-encoding: chunked\r\n\r\n0\r\n\r\n"
    );
}

#[test]
fn test_write_response_bytes() {
    let mut headers = Headers::new_nodate();
    headers.add("foo", b"bar");
    let mut w = MockWriter::new();
    HttpPrinter::write_response_bytes(&mut w, &Status::CREATED, &headers, b"hello123").unwrap();

    assert_eq!(
        w.as_str(),
        "HTTP/1.1 201 CREATED\r\nfoo: bar\r\ncontent-length: 8\r\n\r\nhello123"
    );
}

#[test]
fn test_write_response_bytes_chunked() {
    let mut headers = Headers::new_nodate();
    headers.add("foo", b"bar");
    headers.set_transfer_encoding_chunked();
    let mut w = MockWriter::new();
    HttpPrinter::write_response_bytes(&mut w, &Status::CREATED, &headers, b"hello123").unwrap();

    assert_eq!(
        w.as_str(),
        "HTTP/1.1 201 CREATED\r\nfoo: bar\r\ntransfer-encoding: chunked\r\n\r\n8\r\nhello123\r\n0\r\n\r\n"
    );
}

#[test]
fn test_100_continue() {
    let mut w = MockWriter::new();
    HttpPrinter::write_100_continue(&mut w).expect("should print");
    assert_eq!(w.as_str(), "HTTP/1.1 100 Continue\r\n\r\n");
}

// ---------------------------------------------------------------------
// REQUESTS
// ---------------------------------------------------------------------

#[cfg(feature = "client")]
#[test]
fn test_request_with_content_length() {
    let mut headers = Headers::new_nodate();
    headers.set_content_length(Some(4));

    assert_eq!(
        print_request(Post, "/api", &headers, &b"test"[..]),
        "POST /api HTTP/1.1\r\ncontent-length: 4\r\n\r\ntest"
    );
}

#[cfg(feature = "client")]
#[test]
fn test_request_with_te() {
    let mut headers = Headers::new_nodate();
    headers.set_transfer_encoding_chunked();

    assert_eq!(
        print_request(Post, "/api", &headers, &b"test"[..]),
        "POST /api HTTP/1.1\r\ntransfer-encoding: chunked\r\n\r\n4\r\ntest\r\n0\r\n\r\n",
    );
}

// ---------------------------------------------------------------------
// UTILS
// ---------------------------------------------------------------------

#[cfg(feature = "client")]
fn print_request(method: Method, uri: &str, headers: &Headers, body: impl Read) -> String {
    let mut w = MockWriter::new();
    HttpPrinter::write_request(&mut w, &method, uri, headers, body).expect("should print");
    w.into_string()
}

fn print_response(status: Status, headers: Headers, body: impl Read) -> String {
    let mut w = MockWriter::new();
    HttpPrinter::write_response(&mut w, &status, &headers, body).expect("should print");
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
