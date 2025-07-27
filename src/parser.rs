use crate::common::{Headers, Method, RequestUri, Status};
use std::{
    error::Error,
    fmt::Display,
    io::{self, BufRead, BufReader, Read},
};

pub struct RequestParser<R: Read> {
    reader: BufReader<R>,
}

pub struct RequestStatusLine {
    pub method: Method,
    pub uri: String,
    pub version: String,
}

#[derive(Debug)]
pub struct RequestParts<R: Read> {
    pub headers: Headers,
    pub method: Method,
    pub uri: RequestUri,
    pub reader: BufReader<R>,
    pub http_version: String,
}

impl<R: Read> RequestParser<R> {
    pub fn new(stream: R) -> Self {
        Self {
            reader: BufReader::new(stream),
        }
    }

    pub fn parse(mut self) -> Result<RequestParts<R>, HttpParsingError> {
        let mut line_buf = String::with_capacity(256);
        let status_line = parse_request_status_line(&mut self.reader, &mut line_buf)?;
        let headers = parse_headers(&mut self.reader, &mut line_buf)?;

        Ok(RequestParts {
            method: status_line.method,
            uri: RequestUri::new(status_line.uri),
            http_version: status_line.version,
            headers,
            reader: self.reader,
        })
    }
}

pub struct ResponseParser<R: Read> {
    reader: BufReader<R>,
}

#[derive(Debug)]
pub struct ResponseParts<R: Read> {
    pub headers: Headers,
    pub status: Status,
    pub reader: BufReader<R>,
}

impl<R: Read> ResponseParser<R> {
    pub fn new(stream: R) -> Self {
        Self {
            reader: BufReader::new(stream),
        }
    }

    pub fn parse(mut self) -> Result<ResponseParts<R>, HttpParsingError> {
        let mut line_buf = String::with_capacity(256);
        let status = parse_response_status_line(&mut self.reader, &mut line_buf)?;
        let headers = parse_headers(&mut self.reader, &mut line_buf)?;

        Ok(ResponseParts {
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
) -> Result<Status, HttpParsingError> {
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

    Ok(Status::owned(code, reason))
}

pub fn parse_request_status_line<R: BufRead>(
    reader: &mut R,
    buf: &mut String,
) -> Result<RequestStatusLine, HttpParsingError> {
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

    Ok(RequestStatusLine {
        method: method.into(),
        uri: uri.to_string(),
        version: version.to_string(),
    })
}

pub fn parse_headers<R: BufRead>(
    reader: &mut R,
    buf: &mut String,
) -> Result<Headers, HttpParsingError> {
    let mut headers = Headers::new();

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
            Err(e) => return Err(HttpParsingError::IOError(e)),
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum HttpParsingError {
    MalformedStatusLine,
    MalformedHeader,
    UnexpectedEof,
    IOError(io::Error),
}

impl PartialEq for HttpParsingError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::IOError(_), Self::IOError(_)) => true,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl Error for HttpParsingError {}

impl Display for HttpParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use HttpParsingError::*;
        match self {
            MalformedStatusLine => write!(f, "Malformed status line!"),
            MalformedHeader => write!(f, "Malformed header!"),
            UnexpectedEof => write!(f, "Unexpected end of stream!"),
            IOError(e) => write!(f, "io error: {}", e),
        }
    }
}

impl From<std::io::Error> for HttpParsingError {
    fn from(e: std::io::Error) -> Self {
        HttpParsingError::IOError(e)
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
