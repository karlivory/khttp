#[cfg(test)]
mod tests {
    use khttp::{
        common::{HttpHeaders, HttpMethod, HttpStatus},
        http_parser::{
            HttpParsingError, HttpRequestParser, HttpRequestParts, HttpResponseParser,
            HttpResponseParts,
        },
        http_printer::HttpPrinter,
    };
    use std::{
        collections::HashMap,
        io::{BufReader, Read},
    };

    struct HttpParserResponseTest {
        str: &'static str,
        expected: Result<HttpResponseParts<MockReader>, HttpParsingError>,
    }

    struct HttpParserRequestTest {
        str: &'static str,
        expected: Result<HttpRequestParts<MockReader>, HttpParsingError>,
    }

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

    fn get_reader(s: &str) -> BufReader<MockReader> {
        let reader = MockReader {
            body: s.to_string(),
            read: false,
        };
        BufReader::new(reader)
    }

    fn get_request_tests() -> Vec<HttpParserRequestTest> {
        vec![
            // test1
            HttpParserRequestTest {
                str: "GET /hello HTTP/1.1\r\nheader1: foo\r\nheader2: bar\r\ncontent-length: 3\r\n\r\nabc",
                expected: Ok(HttpRequestParts {
                    method: HttpMethod::Get,
                    uri: "/hello".to_string(),
                    headers: HttpHeaders::from(HashMap::from([
                        ("header1", "foo"),
                        ("header2", "bar"),
                        ("content-length", "3"),
                    ])),
                    reader: get_reader("abc"),
                }),
            },
            // test2
            HttpParserRequestTest {
                str: "GET/\r\n\r\nheader1: foo\r\n\r\n",
                expected: Err(HttpParsingError::MalformedStatusLine),
            },
        ]
    }

    fn get_response_tests() -> Vec<HttpParserResponseTest> {
        vec![
            // test1
            HttpParserResponseTest {
                str: "HTTP/1.1 200 OK\r\n\r\n",
                expected: Ok(HttpResponseParts {
                    headers: HttpHeaders::new(),
                    status: HttpStatus::new(200, "OK".to_string()),
                    reader: get_reader(""),
                }),
            },
            HttpParserResponseTest {
                str: "HTTP/1.1 500 Internal Server Foobar\r\n\r\n",
                expected: Ok(HttpResponseParts {
                    headers: HttpHeaders::new(),
                    status: HttpStatus::new(500, "Internal Server Foobar".to_string()),
                    reader: get_reader(""),
                }),
            },
            // test2
            HttpParserResponseTest {
                str: "HTTP/1.1 200 OK\r\nheader1: foobar\r\nheader2: 123\r\n\r\n",
                expected: Ok(HttpResponseParts {
                    headers: HttpHeaders::from(HashMap::from([
                        ("header1", "foobar"),
                        ("header2", "123"),
                    ])),
                    status: HttpStatus::new(200, "OK".to_string()),
                    reader: get_reader(""),
                }),
            },
            // test3
            HttpParserResponseTest {
                str: "HTTP/1.1 200 OK\r\nheader1: foobar\r\ncontent-length: 5\r\n\r\nabcde",
                expected: Ok(HttpResponseParts {
                    headers: HttpHeaders::from(HashMap::from([
                        ("header1", "foobar"),
                        ("content-length", "5"),
                    ])),
                    status: HttpStatus::new(200, "OK".to_string()),
                    reader: get_reader("abcde"),
                }),
            },
            HttpParserResponseTest {
                str: "HTTP/1.1 20000000000000 BAD\r\n\r\n",
                expected: Err(HttpParsingError::MalformedStatusLine),
            },
            HttpParserResponseTest {
                str: "HTTP/1.1 200 OK\r\nheader:\r\n",
                expected: Err(HttpParsingError::MalformedHeader),
            },
        ]
    }

    fn assert_eq_request(
        req1: &mut Result<HttpRequestParts<MockReader>, HttpParsingError>,
        req2: &mut Result<HttpRequestParts<MockReader>, HttpParsingError>,
    ) -> (String, String) {
        match (req1, req2) {
            (Ok(req1), Ok(req2)) => assert_eq_request_parts(req1, req2),
            (Err(e1), Err(e2)) => {
                assert_eq!(e1, e2);
                ("".to_string(), "".to_string())
            }
            (Ok(_), Err(_)) => panic!("req1 != req2"),
            (Err(_), Ok(_)) => todo!(),
        }
    }

    fn assert_eq_request_parts(
        req1: &mut HttpRequestParts<MockReader>,
        req2: &mut HttpRequestParts<MockReader>,
    ) -> (String, String) {
        assert_eq!(req1.method, req2.method);
        assert_eq!(req1.uri, req2.uri);
        assert_eq!(req1.headers, req2.headers);

        let mut body1_buf = String::new();
        let mut body2_buf = String::new();
        _ = req1.reader.read_to_string(&mut body1_buf);
        _ = req2.reader.read_to_string(&mut body2_buf);
        assert_eq!(body1_buf, body2_buf);

        (body1_buf, body2_buf)
    }

    fn assert_eq_response(
        res1: &mut Result<HttpResponseParts<MockReader>, HttpParsingError>,
        res2: &mut Result<HttpResponseParts<MockReader>, HttpParsingError>,
    ) -> (String, String) {
        match (res1, res2) {
            (Ok(res1), Ok(res2)) => assert_eq_response_parts(res1, res2),
            (Err(e1), Err(e2)) => {
                assert_eq!(e1, e2);
                ("".to_string(), "".to_string())
            }
            (Ok(_), Err(_)) => panic!("res1 != res2"),
            (Err(_), Ok(_)) => todo!(),
        }
    }

    fn assert_eq_response_parts(
        res1: &mut HttpResponseParts<MockReader>,
        res2: &mut HttpResponseParts<MockReader>,
    ) -> (String, String) {
        assert_eq!(res1.status, res2.status);
        assert_eq!(res1.headers, res2.headers);

        let mut body1_buf = String::new();
        let mut body2_buf = String::new();
        _ = res1.reader.read_to_string(&mut body1_buf);
        _ = res2.reader.read_to_string(&mut body2_buf);
        assert_eq!(body1_buf, body2_buf);

        (body1_buf, body2_buf)
    }

    #[test]
    fn test_responses() {
        for mut test in get_response_tests().into_iter() {
            let stream = MockReader {
                body: test.str.to_string(),
                read: false,
            };

            let mut response = HttpResponseParser::new(stream).parse();
            let (body1, _) = assert_eq_response(&mut response, &mut test.expected);

            // let's re-print using HttpPrinter and parse it again
            if let Ok(ref mut res) = response {
                let mut buf = Vec::new();
                HttpPrinter::new(&mut buf)
                    .write_response(&res.status, &res.headers, get_reader(&body1))
                    .unwrap();

                res.reader = get_reader(&body1);

                let stream = MockReader {
                    body: String::from_utf8_lossy(&buf).to_string(),
                    read: false,
                };
                let mut new_response = HttpResponseParser::new(stream).parse();
                assert_eq_response(&mut new_response, &mut response); // now succeeds
            }
        }
    }

    #[test]
    fn test_requests() {
        for mut test in get_request_tests().into_iter() {
            let stream = MockReader {
                body: test.str.to_string(),
                read: false,
            };

            let mut request = HttpRequestParser::new(stream).parse();
            let (body1, _) = assert_eq_request(&mut request, &mut test.expected);

            // let's re-print using HttpPrinter and parse it again
            if let Ok(ref mut req) = request {
                let mut buf = Vec::new();
                HttpPrinter::new(&mut buf)
                    .write_request(&req.method, &req.uri, &req.headers, get_reader(&body1))
                    .unwrap();

                req.reader = get_reader(&body1);

                let stream = MockReader {
                    body: String::from_utf8_lossy(&buf).to_string(),
                    read: false,
                };

                let mut new_request = HttpRequestParser::new(stream).parse();
                assert_eq_request(&mut new_request, &mut request);
            }
        }
    }
}
