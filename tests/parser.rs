use khttp::{Headers, HttpParsingError, Method, Request, Response};
use std::io::Read;

// ---------------------------------------------------------------------
// REQUEST OK
// ---------------------------------------------------------------------

#[test]
fn test_request_get_no_headers() {
    assert_parse_request_ok("GET /ab HTTP/1.1\r\n\r\n", Method::Get, "/ab", &[], "");
}

#[test]
fn test_request_get_simple() {
    assert_parse_request_ok(
        "GET /foo HTTP/1.1\r\nhost: localhost\r\n\r\n",
        Method::Get,
        "/foo",
        &[("host", b"localhost")],
        "",
    );
}

#[test]
fn test_request_post_with_body() {
    assert_parse_request_ok(
        "POST /data HTTP/1.1\r\nfoobar: 5\r\n\r\nhello",
        Method::Post,
        "/data",
        &[("foobar", b"5")],
        "hello",
    );
}

// header fields: https://datatracker.ietf.org/doc/html/rfc7230#section-3.2.4

#[test]
fn test_request_header_empty_value() {
    assert_parse_request_ok(
        "GET /foo HTTP/1.1\r\nX-Test:\r\n\r\n",
        Method::Get,
        "/foo",
        &[("X-Test", b"")],
        "",
    );
}

#[test]
fn test_request_header_value_leading_whitespace_is_removed() {
    assert_parse_request_ok(
        "GET / HTTP/1.1\r\nFoo:\t    bar\r\n\r\n",
        Method::Get,
        "/",
        &[("Foo", b"bar")],
        "",
    );
}

#[test]
fn test_request_header_value_trailing_whitespace_is_kept() {
    assert_parse_request_ok(
        "GET / HTTP/1.1\r\nFoo: bar  \t \r\n\r\n",
        Method::Get,
        "/",
        &[("Foo", b"bar  \t ")],
        "",
    );
}

// URI characters: https://datatracker.ietf.org/doc/html/rfc3986#section-2

#[test]
fn test_request_valid_uri_chars() {
    assert_parse_request_ok(
        "GET http://host:8080/-._~:/?#%[]@!$&'()*+,;= HTTP/1.1\r\n\r\n",
        Method::Get,
        "http://host:8080/-._~:/?#%[]@!$&'()*+,;=",
        &[],
        "",
    );
}

// // ---------------------------------------------------------------------
// // REQUEST ERRORS
// // ---------------------------------------------------------------------

#[test]
fn test_request_missing_headers_eof() {
    assert_parse_request_err("GET / HTTP/1.1", HttpParsingError::UnexpectedEof);
}

#[test]
fn test_request_missing_http_version() {
    assert_parse_request_err(
        "GET /hello\r\nheader: value\r\n\r\n",
        HttpParsingError::MalformedStatusLine,
    );
}

#[test]
fn test_request_header_without_colon() {
    assert_parse_request_err(
        "GET / HTTP/1.1\r\nbadheader\r\n\r\n",
        HttpParsingError::MalformedHeader,
    );
}

#[test]
fn test_request_header_with_invalid_characters() {
    assert_parse_request_err(
        "GET / HTTP/1.1\r\nbad\x01header: val\r\n\r\n",
        HttpParsingError::MalformedHeader,
    );
}

#[test]
fn test_request_unsupported_http_version() {
    assert_parse_request_err(
        "GET / HTTP/2\r\n\r\n",
        HttpParsingError::UnsupportedHttpVersion,
    );
    assert_parse_request_err(
        "GET / HTTP/3\r\n\r\n",
        HttpParsingError::UnsupportedHttpVersion,
    );
    assert_parse_request_err(
        "GET / HTTP/F\r\n\r\n",
        HttpParsingError::UnsupportedHttpVersion,
    );
}

// ---------------------------------------------------------------------
// RESPONSE OK
// ---------------------------------------------------------------------

#[test]
fn test_response_crlf_only_headers() {
    assert_parse_response_ok(
        "HTTP/1.1 204 No Content\r\n\r\n",
        204,
        "No Content",
        &[],
        "",
    );
}

#[test]
fn test_response_simple_ok() {
    assert_parse_response_ok(
        "HTTP/1.1 200 OK\r\nfoobar: 5\r\n\r\nhello",
        200,
        "OK",
        &[("foobar", b"5")],
        "hello",
    );
}

#[test]
fn test_response_not_found() {
    assert_parse_response_ok("HTTP/1.1 404 Not Found\r\n\r\n", 404, "Not Found", &[], "");
}

#[test]
fn test_response_empty_reason_phrase() {
    assert_parse_response_ok("HTTP/1.1 204 \r\n\r\n", 204, "", &[], "");
}

#[test]
fn test_response_multiple_headers_same_name() {
    assert_parse_response_ok(
        "HTTP/1.1 200 OK\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n\r\n",
        200,
        "OK",
        &[("Set-Cookie", b"a=1"), ("Set-Cookie", b"b=2")],
        "",
    );
}

#[test]
fn test_response_large_header_value() {
    let big = "a".repeat(1024);
    assert_parse_response_ok(
        &format!("HTTP/1.1 200 OK\r\nBig: {}\r\n\r\n", big),
        200,
        "OK",
        &[("Big", big.as_bytes())],
        "",
    );
}

#[test]
fn test_response_extra_crlf_after_headers() {
    assert_parse_response_ok(
        "HTTP/1.1 200 OK\r\nfoobar: 5\r\n\r\n\r\nhello",
        200,
        "OK",
        &[("foobar", b"5")],
        "\r\nhello",
    );
}

// // ---------------------------------------------------------------------
// // RESPONSE ERRORS
// // ---------------------------------------------------------------------

#[test]
fn test_response_invalid_status_code_4_digits() {
    assert_parse_response_err(
        "HTTP/1.1 2000 OK\r\n\r\n",
        HttpParsingError::MalformedStatusLine,
    );
}

#[test]
fn test_response_header_eof_before_complete() {
    assert_parse_response_err(
        "HTTP/1.1 200 OK\r\nheader:\r\n",
        HttpParsingError::UnexpectedEof,
    );
}

#[test]
fn test_response_header_without_colon() {
    assert_parse_response_err(
        "HTTP/1.1 200 OK\r\ninvalidheader\r\n\r\n",
        HttpParsingError::MalformedHeader,
    );
}

#[test]
fn test_response_header_invalid_name() {
    assert_parse_response_err(
        "HTTP/1.1 200 OK\r\nX-\x01Bad: foo\r\n\r\n",
        HttpParsingError::MalformedHeader,
    );
}

#[test]
fn test_response_status_code_two_digits() {
    assert_parse_response_err(
        "HTTP/1.1 99 Weird\r\n\r\n",
        HttpParsingError::MalformedStatusLine,
    );
}

#[test]
fn test_response_status_code_non_numeric() {
    assert_parse_response_err(
        "HTTP/1.1 abc OK\r\n\r\n",
        HttpParsingError::MalformedStatusLine,
    );
}

// ---------------------------------------------------------------------
// UTILS
// ---------------------------------------------------------------------

fn assert_parse_request_ok(
    input: &str,
    method: Method,
    uri: &str,
    headers: &[(&str, &[u8])],
    body: &str,
) {
    let buf = input.as_bytes();
    let mut req = Request::new();
    let n = req.parse(buf).expect("should parse");
    assert_eq!(req.method.unwrap(), method);
    assert_eq!(req.uri.unwrap().full(), uri);
    assert_eq!(req.headers, Headers::from(headers));

    let mut body_reader = MockReader {
        body: &buf[n..],
        read: false,
    };
    let mut actual = String::new();
    body_reader.read_to_string(&mut actual).unwrap();
    assert_eq!(actual, body);
}

fn assert_parse_request_err(input: &str, expected: HttpParsingError) {
    let buf = input.as_bytes();
    let mut req = Request::new();
    let result = req.parse(buf);
    assert_eq!(result.unwrap_err(), expected);
}

fn assert_parse_response_ok(
    input: &str,
    code: u16,
    reason: &str,
    headers: &[(&str, &[u8])],
    body: &str,
) {
    let buf = input.as_bytes();
    let mut res = Response::new();
    let n = res.parse(buf).expect("should parse");
    let status = res.status.as_ref().unwrap();
    assert_eq!(status.code, code);
    assert_eq!(status.reason, reason);
    assert_eq!(res.headers, Headers::from(headers));

    let mut reader = MockReader {
        body: &buf[n..],
        read: false,
    };
    let mut out = String::new();
    reader.read_to_string(&mut out).unwrap();
    assert_eq!(out, body);
}

fn assert_parse_response_err(input: &str, expected: HttpParsingError) {
    let buf = input.as_bytes();
    let mut res = Response::new();
    let result = res.parse(buf);
    assert_eq!(result.unwrap_err(), expected);
}

#[derive(Debug, PartialEq)]
struct MockReader<'a> {
    pub body: &'a [u8],
    read: bool,
}

impl Read for MockReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.read {
            return Ok(0);
        }

        let n = self.body.len();
        buf[..n].copy_from_slice(self.body);
        self.read = true;
        Ok(n)
    }
}
