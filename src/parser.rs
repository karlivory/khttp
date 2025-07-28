use crate::{Headers, Method, RequestUri, Status};
use std::{
    error::Error,
    fmt::Display,
    io::{self, BufRead, BufReader, Read},
};

pub struct Parser<R: Read> {
    reader: BufReader<R>,
}

#[derive(Debug)]
pub struct RequestParts<R: Read> {
    pub headers: Headers,
    pub method: Method,
    pub uri: RequestUri,
    pub reader: BufReader<R>,
    pub http_version: String,
}

#[derive(Debug)]
pub struct ResponseParts<R: Read> {
    pub headers: Headers,
    pub status: Status,
    pub reader: BufReader<R>,
}

impl<R: Read> Parser<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
        }
    }

    pub fn parse_request(
        mut self,
        max_status_line_length: &Option<usize>,
        max_header_line_length: &Option<usize>,
        max_header_count: &Option<usize>,
    ) -> Result<RequestParts<R>, HttpParsingError> {
        let mut line_buf = String::with_capacity(256);

        let (method, uri, http_version) = match max_status_line_length {
            Some(limit) => parse_request_status_line(
                &mut LimitedBufRead::new(&mut self.reader, *limit),
                &mut line_buf,
            )?,
            None => parse_request_status_line(&mut self.reader, &mut line_buf)?,
        };
        let headers = match max_header_line_length {
            Some(limit) => parse_headers(
                &mut LimitedBufRead::new(&mut self.reader, *limit),
                &mut line_buf,
                max_header_count,
            )?,
            None => parse_headers(&mut self.reader, &mut line_buf, max_header_count)?,
        };

        Ok(RequestParts {
            method,
            uri,
            http_version,
            headers,
            reader: self.reader,
        })
    }

    pub fn parse_response(
        mut self,
        max_status_line_length: &Option<usize>,
        max_header_line_length: &Option<usize>,
        max_header_count: &Option<usize>,
    ) -> Result<ResponseParts<R>, HttpParsingError> {
        let mut line_buf = String::with_capacity(256);
        let status = match max_status_line_length {
            Some(limit) => parse_response_status_line(
                &mut LimitedBufRead::new(&mut self.reader, *limit),
                &mut line_buf,
            )?,
            None => parse_response_status_line(&mut self.reader, &mut line_buf)?,
        };

        let headers = match max_header_line_length {
            Some(limit) => parse_headers(
                &mut LimitedBufRead::new(&mut self.reader, *limit),
                &mut line_buf,
                max_header_count,
            )?,
            None => parse_headers(&mut self.reader, &mut line_buf, max_header_count)?,
        };

        Ok(ResponseParts {
            status,
            headers,
            reader: self.reader,
        })
    }
}

// -------------------------------------------------------------------------
// Utils
// -------------------------------------------------------------------------

fn read_crlf_line<R: BufRead>(r: &mut R, buf: &mut String) -> io::Result<bool> {
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

fn parse_response_status_line<R: BufRead>(
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
) -> Result<(Method, RequestUri, String), HttpParsingError> {
    match read_crlf_line(reader, buf) {
        Ok(true) => (),
        Ok(false) => return Err(HttpParsingError::UnexpectedEof),
        Err(e) if e.kind() == io::ErrorKind::Other => {
            return Err(HttpParsingError::StatusLineTooLong);
        }
        Err(e) => return Err(e.into()),
    }

    let mut parts = buf.split_ascii_whitespace();
    let method = parts.next().ok_or(HttpParsingError::MalformedStatusLine)?;
    let uri = parts.next().ok_or(HttpParsingError::MalformedStatusLine)?;
    let version = parts.next().ok_or(HttpParsingError::MalformedStatusLine)?;

    Ok((
        method.into(),
        RequestUri::new(uri.to_string()),
        version.to_string(),
    ))
}

pub fn parse_headers<R: BufRead>(
    reader: &mut R,
    buf: &mut String,
    max_header_count: &Option<usize>,
) -> Result<Headers, HttpParsingError> {
    let mut headers = Headers::new();
    let mut i = 0;
    loop {
        if let Some(limit) = max_header_count {
            if i > *limit {
                return Err(HttpParsingError::TooManyHeaders);
            }
            i += 1;
        }
        buf.clear();
        match read_crlf_line(reader, buf) {
            Ok(true) => {
                if buf.is_empty() {
                    return Ok(headers);
                }

                let (name, value) = parse_header_line(buf)?;
                headers.add(name, value.trim_ascii_start());
            }
            Ok(false) => {
                return Err(HttpParsingError::UnexpectedEof);
            }
            Err(e) if e.kind() == io::ErrorKind::Other => {
                return Err(HttpParsingError::HeaderLineTooLong);
            }
            Err(e) => return Err(HttpParsingError::IOError(e)),
        }
    }
}

fn parse_header_line(line: &str) -> Result<(&str, &str), HttpParsingError> {
    for (i, b) in line.bytes().enumerate() {
        if b == b':' {
            let name = &line[..i];
            let value = &line[i + 1..];

            // validate for US-ASCII
            if name.bytes().any(|b| b <= 0x20 || b >= 0x7f) {
                return Err(HttpParsingError::MalformedHeader);
            }

            return Ok((name, value));
        }
    }
    Err(HttpParsingError::MalformedHeader) // no ':' found
}

struct LimitedBufRead<'a, R: BufRead> {
    inner: &'a mut R,
    remaining: usize,
}

impl<'a, R: BufRead> LimitedBufRead<'a, R> {
    fn new(inner: &'a mut R, max: usize) -> Self {
        Self {
            inner,
            remaining: max,
        }
    }
}

impl<R: BufRead> BufRead for LimitedBufRead<'_, R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.remaining == 0 {
            return Err(io::Error::new(io::ErrorKind::Other, ""));
        }
        let buf = self.inner.fill_buf()?;
        if buf.len() > self.remaining {
            return Err(io::Error::new(io::ErrorKind::Other, ""));
        }
        Ok(buf)
    }

    fn consume(&mut self, amt: usize) {
        let used = std::cmp::min(amt, self.remaining);
        self.remaining -= used;
        self.inner.consume(amt);
    }
}

impl<R: BufRead> Read for LimitedBufRead<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::new(io::ErrorKind::Other, ""));
        }
        let to_read = std::cmp::min(buf.len(), self.remaining);
        let n = self.inner.read(&mut buf[..to_read])?;
        self.remaining -= n;
        Ok(n)
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum HttpParsingError {
    MalformedStatusLine,
    MalformedHeader,
    UnexpectedEof,
    StatusLineTooLong,
    HeaderLineTooLong,
    TooManyHeaders,
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
            MalformedStatusLine => write!(f, "malformed status line"),
            MalformedHeader => write!(f, "malformed header"),
            UnexpectedEof => write!(f, "unexpected eof"),
            StatusLineTooLong => write!(f, "status line too long"),
            HeaderLineTooLong => write!(f, "header line too long"),
            TooManyHeaders => write!(f, "too many headers"),
            IOError(e) => write!(f, "io error: {}", e),
        }
    }
}

impl From<std::io::Error> for HttpParsingError {
    fn from(e: std::io::Error) -> Self {
        HttpParsingError::IOError(e)
    }
}
