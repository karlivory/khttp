#[cfg(test)]
mod tests {
    use khttp::{
        common::{HttpHeaders, HttpMethod, HttpRequest, HttpResponse, HttpStatus},
        http_parser::{HttpParser, HttpRequestParsingError},
        http_printer::HttpPrinter,
    };
    use std::collections::HashMap;

    struct HttpParserResponseTest {
        str: &'static str,
        expected: Result<HttpResponse, HttpRequestParsingError>,
    }

    struct HttpParserRequestTest {
        str: &'static str,
        expected: Result<HttpRequest, HttpRequestParsingError>,
    }

    fn get_request_tests() -> Vec<HttpParserRequestTest> {
        vec![
            // test1
            HttpParserRequestTest {
                str: "GET /hello HTTP/1.1\r\nheader1: foo\r\nheader2: bar\r\ncontent-length: 3\r\n\r\nabc",
                expected: Ok(HttpRequest {
                    body: Some("abc".as_bytes().to_vec()),
                    headers: HttpHeaders::from(HashMap::from([
                        ("header1", "foo"),
                        ("header2", "bar"),
                        ("content-length", "3"),
                    ])),
                    method: HttpMethod::Get,
                    uri: "/hello".to_string(),
                }),
            },
            // test2
            HttpParserRequestTest {
                str: "GET/\r\n\r\nheader1: foo\r\n\r\n",
                expected: Err(HttpRequestParsingError::MalformedStatusLine),
            },
        ]
    }

    fn get_response_tests() -> Vec<HttpParserResponseTest> {
        vec![
            // test1
            HttpParserResponseTest {
                str: "HTTP/1.1 200 OK\r\n\r\n",
                expected: Ok(HttpResponse {
                    body: None,
                    headers: HttpHeaders::new(),
                    status: HttpStatus::new(200, "OK".to_string()),
                }),
            },
            HttpParserResponseTest {
                str: "HTTP/1.1 500 Internal Server Foobar\r\n\r\n",
                expected: Ok(HttpResponse {
                    body: None,
                    headers: HttpHeaders::new(),
                    status: HttpStatus::new(500, "Internal Server Foobar".to_string()),
                }),
            },
            // test2
            HttpParserResponseTest {
                str: "HTTP/1.1 200 OK\r\nheader1: foobar\r\nheader2: 123\r\n\r\n",
                expected: Ok(HttpResponse {
                    body: None,
                    headers: HttpHeaders::from(HashMap::from([
                        ("header1", "foobar"),
                        ("header2", "123"),
                    ])),
                    status: HttpStatus::new(200, "OK".to_string()),
                }),
            },
            // test3
            HttpParserResponseTest {
                str: "HTTP/1.1 200 OK\r\nheader1: foobar\r\ncontent-length: 5\r\n\r\nabcde",
                expected: Ok(HttpResponse {
                    body: Some("abcde".as_bytes().to_vec()),
                    headers: HttpHeaders::from(HashMap::from([
                        ("header1", "foobar"),
                        ("content-length", "5"),
                    ])),
                    status: HttpStatus::new(200, "OK".to_string()),
                }),
            },
            // test4
            HttpParserResponseTest {
                str: "HTTP/1.1 200 OK\r\nheader1: foobar\r\ncontent-length: 4\r\n\r\nabc",
                expected: Err(HttpRequestParsingError::UnexpectedEOF),
            },
            HttpParserResponseTest {
                str: "HTTP/1.1 20000000000000 BAD\r\n\r\n",
                expected: Err(HttpRequestParsingError::MalformedStatusLine),
            },
            HttpParserResponseTest {
                str: "HTTP/1.1 200 OK\r\nheader:\r\n",
                expected: Err(HttpRequestParsingError::MalformedHeader),
            },
        ]
    }

    #[test]
    fn test_responses() {
        for test in get_response_tests().iter() {
            let bytes = test.str.as_bytes();
            let response = HttpParser::new(bytes).parse_response();
            assert_eq!(response, test.expected);

            // let's re-print using HttpPrinter and parse it again
            if let Ok(ref response) = response {
                let mut buf = Vec::new();
                HttpPrinter::new(&mut buf).write_response(response).unwrap();
                let new_response = HttpParser::new(buf.as_slice()).parse_response();
                assert_eq!(new_response, test.expected);
            }
        }
    }

    #[test]
    fn test_requests() {
        for test in get_request_tests().iter() {
            let bytes = test.str.as_bytes();
            let request = HttpParser::new(bytes).parse_request();
            assert_eq!(request, test.expected);

            // let's re-print using HttpPrinter and parse it again
            if let Ok(ref request) = request {
                let mut buf = Vec::new();
                HttpPrinter::new(&mut buf).write_request(request).unwrap();
                let new_request = HttpParser::new(buf.as_slice()).parse_request();
                assert_eq!(new_request, test.expected);
            }
        }
    }
}
