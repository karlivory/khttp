#[cfg(test)]
mod tests {
    use khttp::{
        common::{HttpHeaders, HttpMethod},
        http_parser::{HttpParsingError, HttpRequestParser, HttpResponseParser},
    };
    use std::io::Read;

    // ---------------------------------------------------------------------
    // REQUEST OK
    // ---------------------------------------------------------------------

    #[test]
    fn test_request_get_simple() {
        assert_parse_request_ok(
            "GET /foo HTTP/1.1\r\nhost: localhost\r\n\r\n",
            HttpMethod::Get,
            "/foo",
            &[("host", &["localhost"])],
            "",
        );
    }

    #[test]
    fn test_request_post_with_body() {
        assert_parse_request_ok(
            "POST /data HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello",
            HttpMethod::Post,
            "/data",
            &[("Content-Length", &["5"])],
            "hello",
        );
    }

    #[test]
    fn test_request_extra_whitespace() {
        assert_parse_request_ok(
            "GET    /abc     HTTP/1.1\r\nhost: x\r\n\r\n",
            HttpMethod::Get,
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
            HttpMethod::Get,
            "/foo",
            &[("X-Test", &[""])],
            "",
        );
    }

    #[test]
    fn test_request_header_with_tabs() {
        assert_parse_request_ok(
            "GET / HTTP/1.1\r\nFoo:\t bar \t\r\n\r\n",
            HttpMethod::Get,
            "/",
            &[("Foo", &["bar"])],
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
    // UTILS
    // ---------------------------------------------------------------------

    #[derive(Debug, PartialEq)]
    struct MockReader {
        pub body: String,
        read: bool,
    }

    impl Read for MockReader {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.read {
                return Ok(0);
            }

            let bytes = self.body.bytes();
            let n = bytes.len();
            for (i, byte) in bytes.enumerate() {
                buf[i] = byte;
            }
            self.read = true;
            Ok(n)
        }
    }

    fn assert_parse_request_ok(
        input: &str,
        method: HttpMethod,
        uri: &str,
        headers: &[(&str, &[&str])],
        body: &str,
    ) {
        let reader = MockReader {
            body: input.to_string(),
            read: false,
        };
        let mut parsed = HttpRequestParser::new(reader)
            .parse()
            .expect("should parse");

        assert_eq!(parsed.method, method);
        assert_eq!(parsed.full_uri, uri);
        assert_eq!(parsed.headers, HttpHeaders::from(headers));

        let mut buf = String::new();
        _ = parsed.reader.read_to_string(&mut buf);
        assert_eq!(buf, body);
    }

    fn assert_parse_request_err(input: &str, expected: HttpParsingError) {
        let reader = MockReader {
            body: input.to_string(),
            read: false,
        };
        let parsed = HttpRequestParser::new(reader).parse();
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
            body: input.to_string(),
            read: false,
        };
        let mut parsed = HttpResponseParser::new(reader)
            .parse()
            .expect("should parse");

        assert_eq!(parsed.status.code, code);
        assert_eq!(parsed.status.reason, reason);
        assert_eq!(parsed.headers, HttpHeaders::from(headers));

        let mut buf = String::new();
        _ = parsed.reader.read_to_string(&mut buf);
        assert_eq!(buf, body);
    }

    fn assert_parse_response_err(input: &str, expected: HttpParsingError) {
        let reader = MockReader {
            body: input.to_string(),
            read: false,
        };
        let parsed = HttpResponseParser::new(reader).parse();
        assert_eq!(parsed.unwrap_err(), expected);
    }
}
