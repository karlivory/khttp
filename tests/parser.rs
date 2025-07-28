use khttp::{BodyReader, Headers, HttpParsingError, Method, Parser, ResponseParts};
use std::io::Read;

// ---------------------------------------------------------------------
// REQUEST OK
// ---------------------------------------------------------------------

#[test]
fn test_request_get_simple() {
    assert_parse_request_ok(
        "GET /foo HTTP/1.1\r\nhost: localhost\r\n\r\n",
        Method::Get,
        "/foo",
        &[("host", &["localhost"])],
        "",
    );
}

#[test]
fn test_request_post_with_body() {
    assert_parse_request_ok(
        "POST /data HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello",
        Method::Post,
        "/data",
        &[("Content-Length", &["5"])],
        "hello",
    );
}

#[test]
fn test_request_extra_whitespace() {
    assert_parse_request_ok(
        "GET    /abc     HTTP/1.1\r\nhost: x\r\n\r\n",
        Method::Get,
        "/abc",
        &[("host", &["x"])],
        "",
    );
}

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
fn test_request_header_empty_value() {
    assert_parse_request_ok(
        "GET /foo HTTP/1.1\r\nX-Test:\r\n\r\n",
        Method::Get,
        "/foo",
        &[("X-Test", &[""])],
        "",
    );
}

#[test]
fn test_request_header_value_leading_whitespace_is_removed() {
    assert_parse_request_ok(
        "GET / HTTP/1.1\r\nFoo:\t    bar\r\n\r\n",
        Method::Get,
        "/",
        &[("Foo", &["bar"])],
        "",
    );
}

#[test]
fn test_request_header_value_trailing_whitespace_is_kept() {
    assert_parse_request_ok(
        "GET / HTTP/1.1\r\nFoo: bar  \t \r\n\r\n",
        Method::Get,
        "/",
        &[("Foo", &["bar  \t "])],
        "",
    );
}

// ---------------------------------------------------------------------
// REQUEST ERRORS
// ---------------------------------------------------------------------

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
fn test_response_simple_ok() {
    assert_parse_response_ok(
        "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello",
        200,
        "OK",
        &[("Content-Length", &["5"])],
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
        &[("Set-Cookie", &["a=1", "b=2"])],
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
        &[("Big", &[&big])],
        "",
    );
}

#[test]
fn test_response_extra_crlf_after_headers() {
    assert_parse_response_ok(
        "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n\r\nhello",
        200,
        "OK",
        &[("Content-Length", &["5"])],
        "\r\nhello", // \r\n included in body
    );
}

// ---------------------------------------------------------------------
// RESPONSE ERRORS
// ---------------------------------------------------------------------

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
// CHUNKED ENCODING
// ---------------------------------------------------------------------

#[test]
fn test_chunked_response_parsing() {
    let raw = b"\
HTTP/1.1 200 OK\r\n\
Transfer-Encoding: chunked\r\n\
\r\n\
5\r\n\
Hello\r\n\
6\r\n\
, worl\r\n\
1\r\n\
d\r\n\
0\r\n\
\r\n";

    let parsed = must_parse_response(raw);
    assert_eq!(parsed.status.code, 200);
    assert_eq!(parsed.status.reason, "OK");
    assert!(parsed.headers.is_transfer_encoding_chunked());

    let mut body_reader = BodyReader::from(&parsed.headers, parsed.reader);
    let mut buf = String::new();
    body_reader.read_to_string(&mut buf).unwrap();

    assert_eq!(buf, "Hello, world");
}

#[test]
fn test_chunked_response_invalid_chunk_size() {
    let raw = b"\
HTTP/1.1 200 OK\r\n\
Transfer-Encoding: chunked\r\n\
\r\n\
ZZ\r\n\
Hello\r\n\
0\r\n\
\r\n";

    let parsed = must_parse_response(raw);
    let mut body = BodyReader::from(&parsed.headers, parsed.reader);
    let mut out = String::new();

    let err = body.read_to_string(&mut out).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn test_transfer_encoding_overrides_content_length() {
    let raw = b"\
HTTP/1.1 200 OK\r\n\
Content-Length: 100\r\n\
Transfer-Encoding: chunked\r\n\
\r\n\
3\r\n\
Hel\r\n\
2\r\n\
lo\r\n\
0\r\n\
\r\n";

    let parsed = must_parse_response(raw);
    assert!(parsed.headers.is_transfer_encoding_chunked());
    assert_eq!(parsed.headers.get("content-length"), Some("100"));

    let mut body = BodyReader::from(&parsed.headers, parsed.reader);
    let mut buf = String::new();
    body.read_to_string(&mut buf).unwrap();

    assert_eq!(buf, "Hello");
}

#[test]
fn test_chunked_response_with_trailers() {
    let raw = b"\
HTTP/1.1 200 OK\r\n\
Transfer-Encoding: chunked\r\n\
\r\n\
5\r\n\
Hello\r\n\
7\r\n\
, World\r\n\
0\r\n\
X-Foo: trailer\r\n\
X-Bar: more\r\n\
\r\n";

    let parsed = must_parse_response(raw);
    let mut body = BodyReader::from(&parsed.headers, parsed.reader);
    let mut buf = String::new();
    body.read_to_string(&mut buf).unwrap();

    assert_eq!(buf, "Hello, World");
}

// ---------------------------------------------------------------------
// UTILS
// ---------------------------------------------------------------------

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
        for (i, byte) in self.body.iter().enumerate() {
            buf[i] = *byte;
        }
        self.read = true;
        Ok(n)
    }
}

fn must_parse_response(body: &[u8]) -> ResponseParts<MockReader> {
    let test_reader = MockReader { body, read: false };

    Parser::new(test_reader)
        .parse_response(&None, &None, &None)
        .expect("parse headers")
}

fn assert_parse_request_ok(
    input: &str,
    method: Method,
    uri: &str,
    headers: &[(&str, &[&str])],
    body: &str,
) {
    let reader = MockReader {
        body: input.as_bytes(),
        read: false,
    };
    let mut parsed = Parser::new(reader)
        .parse_request(&None, &None, &None)
        .expect("should parse");

    assert_eq!(parsed.method, method);
    assert_eq!(parsed.uri.full(), uri);
    assert_eq!(parsed.headers, Headers::from(headers));

    let mut buf = String::new();
    _ = parsed.reader.read_to_string(&mut buf);
    assert_eq!(buf, body);
}

fn assert_parse_request_err(input: &str, expected: HttpParsingError) {
    let reader = MockReader {
        body: input.as_bytes(),
        read: false,
    };
    let parsed = Parser::new(reader).parse_request(&None, &None, &None);
    assert_eq!(parsed.unwrap_err(), expected);
}

fn assert_parse_response_ok(
    input: &str,
    code: u16,
    reason: &str,
    headers: &[(&str, &[&str])],
    body: &str,
) {
    let reader = MockReader {
        body: input.as_bytes(),
        read: false,
    };
    let mut parsed = Parser::new(reader)
        .parse_response(&None, &None, &None)
        .expect("should parse");

    assert_eq!(parsed.status.code, code);
    assert_eq!(parsed.status.reason, reason);
    assert_eq!(parsed.headers, Headers::from(headers));

    let mut buf = String::new();
    _ = parsed.reader.read_to_string(&mut buf);
    assert_eq!(buf, body);
}

fn assert_parse_response_err(input: &str, expected: HttpParsingError) {
    let reader = MockReader {
        body: input.as_bytes(),
        read: false,
    };
    let parsed = Parser::new(reader).parse_response(&None, &None, &None);
    assert_eq!(parsed.unwrap_err(), expected);
}
