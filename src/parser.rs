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
    pub http_version: u8,
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
        let mut line_buf: Vec<u8> = Vec::with_capacity(256);

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
        let mut line_buf: Vec<u8> = Vec::with_capacity(256);

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

fn read_crlf_line<R: BufRead>(r: &mut R, buf: &mut Vec<u8>) -> io::Result<bool> {
    let n = r.read_until(b'\n', buf)?;
    if n == 0 {
        return Ok(false);
    }

    if buf.ends_with(b"\r\n") {
        buf.truncate(buf.len() - 2);
    } else if buf.ends_with(b"\n") {
        buf.truncate(buf.len() - 1);
    }
    Ok(true)
}

fn parse_response_status_line<R: BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> Result<Status, HttpParsingError> {
    if !read_crlf_line(reader, buf)? {
        return Err(HttpParsingError::UnexpectedEof);
    }
    // TODO: optimize
    let line = std::str::from_utf8(buf).map_err(|_| HttpParsingError::MalformedStatusLine)?;
    let mut parts = line.splitn(3, ' ');
    let _http_version = parts.next().ok_or(HttpParsingError::MalformedStatusLine)?;
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

// request-line = method SP request-target SP HTTP-version
// ref: https://datatracker.ietf.org/doc/html/rfc9112#name-request-line
fn parse_request_status_line<R: BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> Result<(Method, RequestUri, u8), HttpParsingError> {
    match read_crlf_line(reader, buf) {
        Ok(true) => (),
        Ok(false) => return Err(HttpParsingError::UnexpectedEof),
        Err(e) if e.kind() == io::ErrorKind::Other => {
            return Err(HttpParsingError::StatusLineTooLong);
        }
        Err(e) => return Err(e.into()),
    }

    let (method, rest) = parse_method(buf)?;
    let (uri, rest) = parse_uri(rest)?;
    let version = parse_version(rest)?;

    Ok((method, uri, version))
}

fn parse_version(buf: &[u8]) -> Result<u8, HttpParsingError> {
    const PREFIX: &[u8] = b"HTTP/";

    if !buf.starts_with(PREFIX) {
        return Err(HttpParsingError::MalformedStatusLine);
    }
    let version_number = &buf[PREFIX.len()..];
    match version_number {
        b"1" => Ok(0),
        b"1.1" => Ok(1),
        _ => Err(HttpParsingError::UnsupportedHttpVersion),
    }
}

const fn make_uri_byte_mask() -> [bool; 256] {
    let mut mask = [false; 256];
    let valid =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~:/?#[]@!$&'()*+,;=%";
    let mut i = 0;
    while i < valid.len() {
        mask[valid[i] as usize] = true;
        i += 1;
    }
    mask
}

const fn make_header_field_byte_mask() -> [bool; 256] {
    let mut mask = [false; 256];
    let valid = b"!#$%&'*+-.^_`|~ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut i = 0;
    while i < valid.len() {
        mask[valid[i] as usize] = true;
        i += 1;
    }
    mask
}

static HEADER_FIELD_BYTE_MASK: [bool; 256] = make_header_field_byte_mask();

#[inline(always)]
pub fn is_valid_header_field_byte(b: u8) -> bool {
    HEADER_FIELD_BYTE_MASK[b as usize]
}

static URI_BYTE_MASK: [bool; 256] = make_uri_byte_mask();

#[inline(always)]
pub fn is_valid_uri_byte(b: u8) -> bool {
    URI_BYTE_MASK[b as usize]
}

fn parse_uri(buf: &[u8]) -> Result<(RequestUri, &[u8]), HttpParsingError> {
    use HttpParsingError::MalformedStatusLine;

    let mut scheme_i = 0; // 0 → no “://” scheme
    let mut path_i_start = 0; // 0 → origin-form  (“/foo”)
    let mut path_i_end = 0; // exclusive; will be set later

    // ──────────────── 1. classify first byte ───────────────────────────
    let first = *buf.first().ok_or(MalformedStatusLine)?;
    let origin_form = match first {
        b' ' => return Err(MalformedStatusLine),
        b'*' => {
            return Ok((
                RequestUri::new(String::from("*"), 0, 0, 1),
                buf.get(1..).ok_or(MalformedStatusLine)?,
            ));
        }
        b'/' => true,
        _ => false,
    };

    // ──────────────── 2. advance to start of path ──────────────────────
    let mut i = 0;

    if !origin_form {
        // uri is either absolute or authority-form
        while i < buf.len() {
            let b = buf[i];

            match b {
                // detect first “://”
                b':' if scheme_i == 0 && i + 2 < buf.len() && &buf[i..i + 3] == b"://" => {
                    scheme_i = i;
                    i += 3;
                    continue;
                }

                b'/' => {
                    path_i_start = i;
                    break;
                }

                // validate authority byte
                _ => {
                    if !is_valid_uri_byte(b) {
                        return Err(MalformedStatusLine);
                    }
                }
            }

            i += 1;
        }

        // no slash found → authority-form (“example.com:443”)
        if path_i_start == 0 {
            // TODO: test this
            let uri = unsafe { std::str::from_utf8_unchecked(&buf[..i]) };
            return Ok((
                RequestUri::new(uri.to_string(), 0, 0, 0),
                buf.get(i + 1..).ok_or(MalformedStatusLine)?,
            ));
        }
    }

    // ──────────────── 3. scan path/query until the SP ──────────────────
    while i < buf.len() {
        let b = buf[i];

        match b {
            b' ' => {
                if path_i_end == 0 {
                    path_i_end = i; // set end if we never saw “?”
                }
                break; // end of request-target
            }
            b'?' if path_i_end == 0 => {
                // first ‘?’ marks end-of-path
                path_i_end = i;
            }
            _ => {
                if !is_valid_uri_byte(b) {
                    return Err(MalformedStatusLine);
                }
            }
        }

        i += 1;
    }

    if path_i_end == 0 {
        // we never hit ‘ ’ → malformed
        return Err(MalformedStatusLine);
    }

    // ──────────────── 4. build &str and remainder slice ────────────────
    // SAFETY: every byte already validated as US-ASCII subset
    let uri = unsafe { std::str::from_utf8_unchecked(&buf[..i]) };

    // skip the space so `rest` starts with “HTTP/…”
    let rest = buf.get(i + 1..).ok_or(MalformedStatusLine)?;

    Ok((
        RequestUri::new(uri.to_string(), scheme_i, path_i_start, path_i_end),
        rest,
    ))
}

fn parse_method(buf: &[u8]) -> Result<(Method, &[u8]), HttpParsingError> {
    let mut i = 0;

    while i < buf.len() {
        let b = buf[i];
        if b == b' ' {
            // Found end of method
            let method_bytes = &buf[..i];

            // Match known methods directly
            let method = match method_bytes {
                b"GET" => Method::Get,
                b"POST" => Method::Post,
                b"HEAD" => Method::Head,
                b"PUT" => Method::Put,
                b"PATCH" => Method::Patch,
                b"DELETE" => Method::Delete,
                b"OPTIONS" => Method::Options,
                b"TRACE" => Method::Trace,
                _ => {
                    // Validate and fallback to Custom
                    if !method_bytes.iter().all(|b| b.is_ascii_alphabetic()) {
                        return Err(HttpParsingError::MalformedStatusLine);
                    }
                    let s = unsafe { std::str::from_utf8_unchecked(method_bytes) };
                    Method::Custom(s.to_string())
                }
            };

            return Ok((method, &buf[i + 1..]));
        }

        i += 1;
    }

    Err(HttpParsingError::MalformedStatusLine)
}

pub fn parse_headers<R: BufRead>(
    reader: &mut R,
    buf: &mut Vec<u8>,
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
                // safety: parse_header_line lowercases 'name'
                unsafe { headers.add_unchecked(name, value) };
            }
            Ok(false) => return Err(HttpParsingError::UnexpectedEof),
            Err(e) if e.kind() == io::ErrorKind::Other => {
                return Err(HttpParsingError::HeaderLineTooLong);
            }
            Err(e) => return Err(HttpParsingError::IOError(e)),
        }
    }
}

fn parse_header_line(line: &mut [u8]) -> Result<(&str, &str), HttpParsingError> {
    for (i, b) in line.iter_mut().enumerate() {
        if *b == b':' {
            // parse header name: check if ASCII-US, then normalize to lowercase
            for c in line[..i].iter_mut() {
                if !is_valid_header_field_byte(*c) {
                    return Err(HttpParsingError::MalformedHeader);
                }
                c.make_ascii_lowercase();
            }

            // safety: every char in name_str is us-ascii
            let name_str = unsafe { std::str::from_utf8_unchecked(&line[..i]) };

            // parse header value: just a str
            let value = &line[i + 1..].trim_ascii_start();
            let value_str =
                std::str::from_utf8(value).map_err(|_| HttpParsingError::MalformedHeader)?;

            return Ok((name_str, value_str));
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
    UnsupportedHttpVersion,
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
            UnsupportedHttpVersion => write!(f, "invalid http version"),
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
