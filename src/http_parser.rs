// src/http_parser.rs
//
// responsibility: parsing HttpResponse or HttpRequest from T: std::io::Read

// TODO: I'm not entirely sure that a single \r (no \n) is handled correctly in headers
// gotta add more edge-case tests

use crate::common::{HttpHeaders, HttpMethod, HttpRequest, HttpResponse, HttpStatus};
use std::io::{BufReader, Bytes, Read};

pub struct HttpParser<R: Read> {
    reader: BufReader<R>,
}

pub struct HttpRequestStatusLine {
    pub method: HttpMethod,
    pub uri: String,
}

impl<R: Read> HttpParser<R> {
    pub fn new(stream: R) -> Self {
        Self {
            reader: BufReader::new(stream),
        }
    }

    pub fn parse_response(&mut self) -> Result<HttpResponse, HttpRequestParsingError> {
        let mut peekable = self.reader.by_ref().bytes();
        let byte_iter = peekable.by_ref();

        let status = parse_response_status_line(byte_iter)?;
        let headers = parse_headers(byte_iter)?;

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
        let mut peekable = self.reader.by_ref().bytes();
        let byte_iter = peekable.by_ref();

        let status_line = parse_request_status_line(byte_iter)?;
        let headers = parse_headers(byte_iter)?;

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

    fn get_body_bytes(&mut self, content_len: usize) -> Result<Vec<u8>, HttpRequestParsingError> {
        let mut peekable = self.reader.by_ref().bytes();
        let byte_iter = peekable.by_ref();
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

pub struct HttpRequestParts<R: Read> {
    pub headers: HttpHeaders,
    pub method: HttpMethod,
    pub uri: String,
    pub reader: BufReader<R>,
}

pub fn parse_request_parts<R: Read>(
    reader: R,
) -> Result<HttpRequestParts<R>, HttpRequestParsingError> {
    let mut reader = BufReader::new(reader);
    let mut peekable = reader.by_ref().bytes();
    let byte_iter = peekable.by_ref();

    let status_line = parse_request_status_line(byte_iter)?;
    let headers = parse_headers(byte_iter)?;

    Ok(HttpRequestParts {
        method: status_line.method,
        uri: status_line.uri,
        headers,
        reader,
    })
}

pub fn parse_response_status_line<R: Read>(
    byte_iter: &mut Bytes<&mut BufReader<R>>,
) -> Result<HttpStatus, HttpRequestParsingError> {
    let mut status_line: Vec<u8> = Vec::new();

    loop {
        let byte = byte_iter.next();
        if byte.is_none() {
            break;
        }
        let byte = byte.unwrap().unwrap();

        if byte == b'\r' {
            let next_byte = byte_iter.next().unwrap().unwrap();
            if next_byte == b'\n' {
                break;
            } else {
                status_line.push(byte);
                status_line.push(next_byte);
                continue;
            }
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

pub fn parse_request_status_line<R: Read>(
    byte_iter: &mut Bytes<&mut BufReader<R>>,
) -> Result<HttpRequestStatusLine, HttpRequestParsingError> {
    let mut status_line: Vec<u8> = Vec::new();

    loop {
        let byte = byte_iter.next();
        if byte.is_none() {
            break;
        }
        let byte = byte.unwrap().unwrap();

        if byte == b'\r' {
            let next_byte = byte_iter.next().unwrap().unwrap();
            if next_byte == b'\n' {
                break;
            } else {
                status_line.push(byte);
                status_line.push(next_byte);
                continue;
            }
        }
        status_line.push(byte);
    }
    let status_line = String::from_utf8_lossy(&status_line).to_string();

    let parts = status_line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(HttpRequestParsingError::MalformedStatusLine);
    }

    Ok(HttpRequestStatusLine {
        method: parts[0].into(),
        uri: parts[1].to_string(),
    })
}

pub fn parse_headers<R: Read>(
    byte_iter: &mut Bytes<&mut BufReader<R>>,
) -> Result<HttpHeaders, HttpRequestParsingError> {
    let mut headers = HttpHeaders::new();

    let mut header: Vec<u8> = Vec::new();
    loop {
        let byte = byte_iter.next();
        if byte.is_none() {
            break;
        }
        let byte = byte.unwrap().unwrap();

        if byte == b'\r' {
            let next_byte = byte_iter.next().unwrap().unwrap();
            if next_byte != b'\n' {
                header.push(byte);
                header.push(next_byte);
                continue;
            }
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

#[derive(Debug, PartialEq)]
pub enum HttpRequestParsingError {
    MalformedStatusLine,
    MalformedHeader,
    UnknownHttpRequestMethod,
    UnexpectedEOF,
}

fn parse_header_line(line: String) -> Result<(String, String), HttpRequestParsingError> {
    match line.split_once(": ") {
        Some((header, value)) => Ok((header.trim().to_string(), value.trim().to_string())),
        None => Err(HttpRequestParsingError::MalformedHeader),
    }
}
