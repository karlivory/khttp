// src/http_parser.rs
//
// responsibility: parsing HttpResponse or HttpRequest from T: std::io::Read

use crate::common::{HttpHeaders, HttpMethod, HttpRequest, HttpResponse, HttpStatus};
use std::io::{BufReader, Bytes, Read};
use std::iter::Peekable;

pub struct HttpParser<T>
where
    T: std::io::Read,
{
    reader: Peekable<Bytes<BufReader<T>>>,
}

pub struct HttpRequestStatusLine {
    pub method: HttpMethod,
    pub uri: String,
}

impl<T> HttpParser<T>
where
    T: std::io::Read,
{
    pub fn new(stream: T) -> Self {
        Self {
            reader: BufReader::new(stream).bytes().peekable(),
        }
    }

    pub fn parse_response(&mut self) -> Result<HttpResponse, HttpRequestParsingError> {
        let status = self.parse_response_status_line()?;
        let headers = self.parse_headers()?;

        let mut body = None;
        if let Some(content_len) = headers.get_content_length() {
            body = Some(self.get_body_bytes(content_len)?);
        }
        Ok(HttpResponse {
            body,
            status,
            headers,
        })
    }

    pub fn parse_request(&mut self) -> Result<HttpRequest, HttpRequestParsingError> {
        let status_line = self.parse_request_status_line()?;
        let headers = self.parse_headers()?;

        let mut body = None;
        if let Some(content_len) = headers.get_content_length() {
            body = Some(self.get_body_bytes(content_len)?);
        }
        Ok(HttpRequest {
            method: status_line.method,
            uri: status_line.uri,
            body,
            headers,
        })
    }

    fn parse_request_status_line(
        &mut self,
    ) -> Result<HttpRequestStatusLine, HttpRequestParsingError> {
        let byte_iter = self.reader.by_ref();

        let mut status_line: Vec<u8> = Vec::new();
        loop {
            let byte = byte_iter.next();
            if byte.is_none() {
                break;
            }
            let byte = byte.unwrap().unwrap();

            if byte == b'\r' && byte_iter.peek().unwrap().as_ref().unwrap() == &b'\n' {
                byte_iter.next();
                break;
            }
            status_line.push(byte);
        }
        let status_line = String::from_utf8_lossy(&status_line).to_string();

        let parts = status_line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 2 {
            return Err(HttpRequestParsingError::MalformedStatusLine);
        }

        let method = try_parse_http_method(parts[0]);
        if method.is_none() {
            return Err(HttpRequestParsingError::UnknownHttpRequestMethod);
        }
        Ok(HttpRequestStatusLine {
            method: method.unwrap(),
            uri: parts[1].to_string(),
        })
    }

    fn parse_response_status_line(&mut self) -> Result<HttpStatus, HttpRequestParsingError> {
        let byte_iter = self.reader.by_ref();

        let mut status_line: Vec<u8> = Vec::new();
        loop {
            let byte = byte_iter.next();
            if byte.is_none() {
                break;
            }
            let byte = byte.unwrap().unwrap();

            if byte == b'\r' && byte_iter.peek().unwrap().as_ref().unwrap() == &b'\n' {
                byte_iter.next();
                break;
            }
            status_line.push(byte);
        }
        let status_line = String::from_utf8_lossy(&status_line).to_string();

        let parts = status_line.splitn(3, " ").collect::<Vec<_>>();
        if parts.len() < 3 {
            return Err(HttpRequestParsingError::MalformedStatusLine);
        }

        let code = parts[1]
            .parse::<u16>()
            .map_err(|_| HttpRequestParsingError::MalformedStatusLine)?;
        let reason = parts[2].to_string();

        Ok(HttpStatus::new(code, reason))
    }

    fn parse_headers(&mut self) -> Result<HttpHeaders, HttpRequestParsingError> {
        let byte_iter = self.reader.by_ref();
        let mut headers = HttpHeaders::new();
        let mut header: Vec<u8> = Vec::new();
        loop {
            let byte = byte_iter.next();
            if byte.is_none() {
                break;
            }
            let byte = byte.unwrap().unwrap();

            if byte == b'\r' && byte_iter.peek().unwrap().as_ref().unwrap() == &b'\n' {
                byte_iter.next();
                if header.is_empty() {
                    break;
                }
                let header_line = String::from_utf8_lossy(&header).to_string().to_lowercase();
                header = Vec::new();

                let (header, value) = parse_header_line(header_line)?;
                headers.add_header(&header, &value);
                continue;
            }
            header.push(byte);
        }

        Ok(headers)
    }

    fn get_body_bytes(&mut self, content_len: usize) -> Result<Vec<u8>, HttpRequestParsingError> {
        let byte_iter = self.reader.by_ref();
        let mut body_bytes = Vec::with_capacity(content_len);
        if content_len > 0 {
            println!("reading body len {}", content_len);
            for _ in 0..content_len {
                let byte = byte_iter.next();
                if byte.is_none() {
                    return Err(HttpRequestParsingError::UnexpectedEOF); // TODO: is this correct?
                }
                let byte = byte.unwrap().unwrap();
                body_bytes.push(byte);
            }
        }
        Ok(body_bytes)
    }
}

#[derive(Debug, PartialEq)]
pub enum HttpRequestParsingError {
    MalformedStatusLine,
    MalformedHeader,
    UnknownHttpRequestMethod,
    UnexpectedEOF,
}

fn try_parse_http_method(s: &str) -> Option<HttpMethod> {
    Some(match s.to_uppercase().as_str() {
        "POST" => HttpMethod::Post,
        "GET" => HttpMethod::Get,
        _ => return None,
    })
}

fn parse_header_line(line: String) -> Result<(String, String), HttpRequestParsingError> {
    match line.split_once(": ") {
        Some((header, value)) => Ok((header.trim().to_string(), value.trim().to_string())),
        None => Err(HttpRequestParsingError::MalformedHeader),
    }
}
