// src/http_parser.rs

use crate::common::{HttpHeaders, HttpMethod, HttpStatus};
use std::{
    error::Error,
    fmt::Display,
    io::{self, BufRead, BufReader, Read},
};

pub struct HttpRequestParser<R: Read> {
    reader: BufReader<R>,
}

pub struct HttpRequestStatusLine {
    pub method: HttpMethod,
    pub uri: String,
}

pub struct HttpRequestParts<R: Read> {
    pub headers: HttpHeaders,
    pub method: HttpMethod,
    pub uri: String,
    pub reader: BufReader<R>,
}

impl<R: Read> HttpRequestParser<R> {
    pub fn new(stream: R) -> Self {
        Self {
            reader: BufReader::new(stream),
        }
    }

    pub fn parse(mut self) -> Result<HttpRequestParts<R>, HttpParsingError> {
        let mut line_buf = Vec::with_capacity(128);
        let status_line = parse_request_status_line(&mut self.reader, &mut line_buf)?;
        let headers = parse_headers(&mut self.reader, &mut line_buf)?;

        Ok(HttpRequestParts {
            method: status_line.method,
            uri: status_line.uri,
            headers,
            reader: self.reader,
        })
    }
}

pub struct HttpResponseParser<R: Read> {
    reader: BufReader<R>,
}

#[derive(Debug)]
pub struct HttpResponseParts<R: Read> {
    pub headers: HttpHeaders,
    pub status: HttpStatus,
    pub reader: BufReader<R>,
}

impl<R: Read> HttpResponseParser<R> {
    pub fn new(stream: R) -> Self {
        Self {
            reader: BufReader::new(stream),
        }
    }

    pub fn parse(mut self) -> Result<HttpResponseParts<R>, HttpParsingError> {
        let mut line_buf = Vec::with_capacity(128);
        let status = parse_response_status_line(&mut self.reader, &mut line_buf)?;
        let headers = parse_headers(&mut self.reader, &mut line_buf)?;

        Ok(HttpResponseParts {
            status,
            headers,
            reader: self.reader,
        })
    }
}

fn read_crlf_line<R: BufRead>(r: &mut R, buf: &mut Vec<u8>) -> io::Result<bool> {
    buf.clear();
    let n = r.read_until(b'\n', buf)?;
    if n == 0 {
        return Ok(false);
    }
    if n >= 2 && buf[n - 2] == b'\r' {
        buf.truncate(n - 2);
    } else {
        buf.truncate(n - 1);
    }
    Ok(true)
}

pub fn parse_response_status_line<R: BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> Result<HttpStatus, HttpParsingError> {
    if !read_crlf_line(reader, buf)? {
        return Err(HttpParsingError::UnexpectedEof);
    }

    let line = std::str::from_utf8(buf).map_err(|_| HttpParsingError::MalformedStatusLine)?;
    let parts: Vec<&str> = line.splitn(3, ' ').collect();
    if parts.len() < 3 {
        return Err(HttpParsingError::MalformedStatusLine);
    }

    let code = parts[1]
        .parse::<u16>()
        .map_err(|_| HttpParsingError::MalformedStatusLine)?;
    if !(100..=999).contains(&code) {
        // RFC: status code has to be a 3-number digit
        return Err(HttpParsingError::MalformedStatusLine);
    }
    let reason = parts[2].to_string();

    Ok(HttpStatus::owned(code, reason))
}

pub fn parse_request_status_line<R: BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> Result<HttpRequestStatusLine, HttpParsingError> {
    if !read_crlf_line(reader, buf)? {
        return Err(HttpParsingError::UnexpectedEof);
    }

    let line = std::str::from_utf8(buf).map_err(|_| HttpParsingError::MalformedStatusLine)?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if !(2..=3).contains(&parts.len()) {
        return Err(HttpParsingError::MalformedStatusLine);
    }

    let method = parts[0].into();
    let raw_uri = parts[1];

    // Normalize absolute-form to origin-form path (ignore authority/scheme)
    let uri = if raw_uri.starts_with("http://") || raw_uri.starts_with("https://") {
        let pos = raw_uri.find("://").unwrap();
        let after_scheme = &raw_uri[pos + 3..];
        match after_scheme.find('/') {
            Some(path_start) => &after_scheme[path_start..],
            None => "/",
        }
        .to_string()
    } else {
        raw_uri.to_string()
    };

    Ok(HttpRequestStatusLine { method, uri })
}

pub fn parse_headers<R: BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> Result<HttpHeaders, HttpParsingError> {
    let mut headers = HttpHeaders::new();

    loop {
        match read_crlf_line(reader, buf) {
            Ok(true) => {
                // Empty line -> end of header section
                if buf.is_empty() {
                    return Ok(headers);
                }

                // Find first ':'
                let colon = buf
                    .iter()
                    .position(|&b| b == b':')
                    .ok_or(HttpParsingError::MalformedHeader)?;

                let name_bytes = &buf[..colon];
                let value_bytes = &buf[colon + 1..];

                let name = std::str::from_utf8(name_bytes)
                    .map_err(|_| HttpParsingError::MalformedHeader)?;
                validate_field_name(name)?;

                let value_raw = std::str::from_utf8(value_bytes)
                    .map_err(|_| HttpParsingError::MalformedHeader)?;
                let value = value_raw
                    .trim_matches(|c| c == ' ' || c == '\t')
                    .to_string();

                headers.add(name, &value);
            }
            Ok(false) => {
                // EOF before blank line
                return Err(HttpParsingError::UnexpectedEof);
            }
            Err(_) => return Err(HttpParsingError::IOError),
        }
    }
}

#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub enum HttpParsingError {
    MalformedStatusLine,
    MalformedHeader,
    UnexpectedEof,
    IOError,
}

impl Error for HttpParsingError {}

impl Display for HttpParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use HttpParsingError::*;
        match self {
            MalformedStatusLine => write!(f, "Malformed status line!"),
            MalformedHeader => write!(f, "Malformed header!"),
            UnexpectedEof => write!(f, "Unexpected end of stream!"),
            IOError => write!(f, "IO error!"),
        }
    }
}

impl From<std::io::Error> for HttpParsingError {
    fn from(_: std::io::Error) -> Self {
        HttpParsingError::IOError
    }
}

fn validate_field_name(name: &str) -> Result<(), HttpParsingError> {
    if name.is_empty() {
        return Err(HttpParsingError::MalformedHeader);
    }
    if name.bytes().any(|b| b <= 0x20 || b >= 0x7f || b == b':') {
        return Err(HttpParsingError::MalformedHeader);
    }
    Ok(())
}
