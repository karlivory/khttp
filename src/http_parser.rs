use crate::common::{HttpHeaders, HttpMethod, HttpStatus, RequestUri};
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
    pub version: String,
}

#[derive(Debug)]
pub struct HttpRequestParts<R: Read> {
    pub headers: HttpHeaders,
    pub method: HttpMethod,
    pub uri: RequestUri,
    pub reader: BufReader<R>,
    pub http_version: String,
}

impl<R: Read> HttpRequestParser<R> {
    pub fn new(stream: R) -> Self {
        Self {
            reader: BufReader::new(stream),
        }
    }

    pub fn parse(mut self) -> Result<HttpRequestParts<R>, HttpParsingError> {
        let mut line_buf = String::with_capacity(256);
        let status_line = parse_request_status_line(&mut self.reader, &mut line_buf)?;
        let headers = parse_headers(&mut self.reader, &mut line_buf)?;

        Ok(HttpRequestParts {
            method: status_line.method,
            uri: RequestUri::new(status_line.uri),
            http_version: status_line.version,
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
        let mut line_buf = String::with_capacity(256);
        let status = parse_response_status_line(&mut self.reader, &mut line_buf)?;
        let headers = parse_headers(&mut self.reader, &mut line_buf)?;

        Ok(HttpResponseParts {
            status,
            headers,
            reader: self.reader,
        })
    }
}

fn read_crlf_line<R: BufRead>(r: &mut R, buf: &mut String) -> io::Result<bool> {
    buf.clear();
    let n = r.read_line(buf)?;
    if n == 0 {
        return Ok(false);
    }
    if buf.ends_with("\r\n") {
        buf.truncate(buf.len() - 2);
    } else if buf.ends_with('\n') {
        buf.pop();
    }
    Ok(true)
}

pub fn parse_response_status_line<R: BufRead>(
    reader: &mut R,
    buf: &mut String,
) -> Result<HttpStatus, HttpParsingError> {
    if !read_crlf_line(reader, buf)? {
        return Err(HttpParsingError::UnexpectedEof);
    }

    let mut parts = buf.splitn(3, ' ');
    let _http = parts.next().ok_or(HttpParsingError::MalformedStatusLine)?;
    let code = parts
        .next()
        .ok_or(HttpParsingError::MalformedStatusLine)?
        .parse::<u16>()
        .map_err(|_| HttpParsingError::MalformedStatusLine)?;
    let reason = parts
        .next()
        .ok_or(HttpParsingError::MalformedStatusLine)?
        .to_string();

    if !(100..=999).contains(&code) {
        return Err(HttpParsingError::MalformedStatusLine);
    }

    Ok(HttpStatus::owned(code, reason))
}

pub fn parse_request_status_line<R: BufRead>(
    reader: &mut R,
    buf: &mut String,
) -> Result<HttpRequestStatusLine, HttpParsingError> {
    if !read_crlf_line(reader, buf)? {
        return Err(HttpParsingError::UnexpectedEof);
    }

    let mut parts = buf.split_whitespace();
    let method = parts.next().ok_or(HttpParsingError::MalformedStatusLine)?;
    let uri = parts.next().ok_or(HttpParsingError::MalformedStatusLine)?;
    let version = parts.next().ok_or(HttpParsingError::MalformedStatusLine)?;

    if !version.starts_with("HTTP/") {
        return Err(HttpParsingError::MalformedStatusLine);
    }

    Ok(HttpRequestStatusLine {
        method: method.into(),
        uri: uri.to_string(),
        version: version.to_string(),
    })
}

pub fn parse_headers<R: BufRead>(
    reader: &mut R,
    buf: &mut String,
) -> Result<HttpHeaders, HttpParsingError> {
    let mut headers = HttpHeaders::new();

    loop {
        match read_crlf_line(reader, buf) {
            Ok(true) => {
                if buf.trim().is_empty() {
                    return Ok(headers);
                }

                let (name, value) = buf
                    .split_once(':')
                    .ok_or(HttpParsingError::MalformedHeader)?;

                let name = name.trim();
                validate_field_name(name)?;

                let value = value.trim();

                headers.add(name, value);
            }
            Ok(false) => {
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
