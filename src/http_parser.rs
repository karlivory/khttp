// src/http_parser.rs
//
// responsibility: parsing HttpResponseParts or HttpRequestParts from R: std::io::Read

use crate::common::{HttpHeaders, HttpMethod, HttpStatus};
use std::{
    error::Error,
    fmt::Display,
    io::{BufReader, Bytes, Read},
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
        let mut peekable = self.reader.by_ref().bytes();
        let byte_iter = peekable.by_ref();

        let status_line = parse_request_status_line(byte_iter)?;
        let headers = parse_headers(byte_iter)?;

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
        let mut peekable = self.reader.by_ref().bytes();
        let byte_iter = peekable.by_ref();

        let status = parse_response_status_line(byte_iter)?;
        let headers = parse_headers(byte_iter)?;

        Ok(HttpResponseParts {
            status,
            headers,
            reader: self.reader,
        })
    }
}

pub fn parse_response_status_line<R: Read>(
    byte_iter: &mut Bytes<&mut BufReader<R>>,
) -> Result<HttpStatus, HttpParsingError> {
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
        return Err(HttpParsingError::MalformedStatusLine);
    }

    let code = parts[1]
        .parse::<u16>()
        .map_err(|_| HttpParsingError::MalformedStatusLine)?;
    let reason = parts[2].to_string();

    Ok(HttpStatus::new(code, reason))
}

pub fn parse_request_status_line<R: Read>(
    byte_iter: &mut Bytes<&mut BufReader<R>>,
) -> Result<HttpRequestStatusLine, HttpParsingError> {
    let mut status_line: Vec<u8> = Vec::new();

    loop {
        let byte = byte_iter.next();
        if byte.is_none() {
            break;
        }
        let byte = byte.unwrap()?;

        if byte == b'\r' {
            let next_byte = byte_iter.next().unwrap()?;
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
    if parts.len() < 2 || parts.len() > 3 {
        return Err(HttpParsingError::MalformedStatusLine);
    }

    let method = parts[0].into();
    let raw_uri = parts[1];

    // handle absolute form uri-s
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

pub fn parse_headers<R: Read>(
    byte_iter: &mut Bytes<&mut BufReader<R>>,
) -> Result<HttpHeaders, HttpParsingError> {
    let mut headers = HttpHeaders::new();

    let mut header_line_bytes: Vec<u8> = Vec::new();
    loop {
        let byte = byte_iter.next();
        if byte.is_none() {
            break;
        }
        let byte = byte.unwrap()?;

        if byte == b'\r' {
            let next_byte = byte_iter.next().unwrap()?;
            if next_byte != b'\n' {
                header_line_bytes.push(byte);
                header_line_bytes.push(next_byte);
                continue;
            }
            if header_line_bytes.is_empty() {
                break;
            }
            let header_line = String::from_utf8_lossy(&header_line_bytes).to_string();
            header_line_bytes = Vec::new();

            let (header, value) = parse_header_line(header_line)?;
            headers.add_header(&header, &value);
            continue;
        }
        header_line_bytes.push(byte);
    }

    Ok(headers)
}

fn parse_header_line(line: String) -> Result<(String, String), HttpParsingError> {
    match line.split_once(": ") {
        Some((header, value)) => Ok((header.to_string(), value.to_string())),
        None => Err(HttpParsingError::MalformedHeader),
    }
}

#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub enum HttpParsingError {
    MalformedStatusLine,
    MalformedHeader,
    IOError,
}

impl Error for HttpParsingError {}

impl Display for HttpParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use HttpParsingError::*;
        match self {
            MalformedStatusLine => write!(f, "Malformed status line!"),
            MalformedHeader => write!(f, "Malformed header!"),
            IOError => write!(f, "IO error!"),
        }
    }
}

impl From<std::io::Error> for HttpParsingError {
    fn from(_: std::io::Error) -> Self {
        HttpParsingError::IOError
    }
}
