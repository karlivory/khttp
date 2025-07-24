#[cfg(test)]
mod tests {
    use khttp::{
        common::{HttpHeaders, HttpMethod, HttpStatus},
        http_printer::HttpPrinter,
    };
    use std::io::{Cursor, Read, Write};

    // ---------------------------------------------------------------------
    // RESPONSES
    // ---------------------------------------------------------------------

    #[test]
    fn test_response_with_content_length() {
        let mut headers = HttpHeaders::new();
        headers.set_content_length(5);
        assert_print_response(
            b"HTTP/1.1 200 OK\r\ncontent-length: 5\r\n\r\nhello",
            HttpStatus::OK,
            headers,
            "hello",
        );
    }

    #[test]
    fn test_response_auto_content_length_small_body() {
        assert_print_response(
            b"HTTP/1.1 200 OK\r\ncontent-length: 4\r\n\r\ntiny",
            HttpStatus::OK,
            HttpHeaders::new(),
            "tiny",
        );
    }

    #[test]
    fn test_response_chunked_explicit_te() {
        let mut headers = HttpHeaders::new();
        headers.set_transfer_encoding_chunked();

        assert_print_response(
            b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n4\r\ndata\r\n0\r\n\r\n",
            HttpStatus::OK,
            headers,
            "data",
        );
    }

    #[test]
    fn test_large_response_te_overrides_ce() {
        let headers = HttpHeaders::from(vec![
            ("content-length", "5"),
            ("transfer-encoding", "chunked"),
        ]);
        assert_print_response(
            b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n",
            HttpStatus::OK,
            headers,
            "hello",
        );
    }

    #[test]
    fn test_large_response_auto_te() {
        let body = b"hello".repeat(3000);
        let w = capture_response(HttpStatus::OK, HttpHeaders::new(), &body[..]);
        assert!(w.contains("transfer-encoding: chunked"));
        assert!(!w.contains("content-length"));
    }

    #[test]
    fn test_large_response_cl_no_auto_te() {
        let body = b"hello".repeat(3000);
        let mut headers = HttpHeaders::new();
        let cl = body.len() as u64;
        headers.set_content_length(cl);
        let w = capture_response(HttpStatus::OK, headers, &body[..]);
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
            HttpMethod::Post,
            "/api",
            headers,
            "test",
        );
    }

    #[test]
    fn test_request_with_te() {
        let mut headers = HttpHeaders::new();
        headers.set_transfer_encoding_chunked();
        assert_print_request(
            b"POST /api HTTP/1.1\r\ntransfer-encoding: chunked\r\n\r\n4\r\ntest\r\n0\r\n\r\n",
            HttpMethod::Post,
            "/api",
            headers,
            "test",
        );
    }

    // ---------------------------------------------------------------------
    // UTILS
    // ---------------------------------------------------------------------

    fn headers_with_content_length(len: u64) -> HttpHeaders {
        let mut h = HttpHeaders::new();
        h.set_content_length(len);
        h
    }

    fn assert_print_response(
        expected: &[u8],
        status: HttpStatus,
        headers: HttpHeaders,
        body: &str,
    ) {
        let got = capture_response(status, headers, Cursor::new(body));
        let expected = String::from_utf8_lossy(expected);
        assert_eq!(got, expected);
    }

    fn assert_print_request(
        expected: &[u8],
        method: HttpMethod,
        uri: &str,
        headers: HttpHeaders,
        body: &str,
    ) {
        let got = capture_request(method, uri, headers, Cursor::new(body));
        let expected = String::from_utf8_lossy(expected);
        assert_eq!(got, expected);
    }

    fn capture_request(
        method: HttpMethod,
        uri: &str,
        headers: HttpHeaders,
        body: impl Read,
    ) -> String {
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

    fn capture_response(status: HttpStatus, headers: HttpHeaders, body: impl Read) -> String {
        let mut w = MockWriter::new();
        {
            let mut printer = HttpPrinter::new(&mut w);
            printer.write_response(&status, headers, body).unwrap();
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
}
