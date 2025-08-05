use crate::{Headers, Method, RequestUri, Status};
use std::{
    error::Error,
    fmt::Display,
    io::{self},
};

#[derive(Debug)]
pub struct Response<'b> {
    pub http_version: u8,
    pub status: Status<'b>,
    pub headers: Headers<'b>,
    pub buf_offset: usize,
}

#[derive(Debug)]
pub struct Request<'b> {
    pub method: Method,
    pub uri: RequestUri<'b>,
    pub http_version: u8,
    pub headers: Headers<'b>,
    pub buf_offset: usize,
}

impl<'b> Response<'b> {
    pub fn parse(buf: &'b [u8]) -> Result<Response<'b>, HttpParsingError> {
        let start = buf.len();
        let (http_version, rest) = parse_version(buf)?;
        // Step 2: Skip single space
        let rest = rest.get(1..).ok_or(HttpParsingError::MalformedStatusLine)?;
        let (status, rest) = parse_response_status(rest)?;
        let (headers, rest) = parse_headers(rest)?;

        // return buf offset
        Ok(Response {
            http_version,
            status,
            headers,
            buf_offset: start - rest.len(),
        })
    }
}

fn parse_response_status_code(buf: &[u8]) -> Result<u16, HttpParsingError> {
    use HttpParsingError::MalformedStatusLine;
    let hundreds = match buf.first().ok_or(MalformedStatusLine)? {
        x if (*x >= b'0' && *x <= b'9') => *x,
        _ => return Err(MalformedStatusLine),
    };
    let tens = match buf.get(1).ok_or(MalformedStatusLine)? {
        x if (*x >= b'0' && *x <= b'9') => *x,
        _ => return Err(MalformedStatusLine),
    };
    let ones = match buf.get(2).ok_or(MalformedStatusLine)? {
        x if (*x >= b'0' && *x <= b'9') => *x,
        _ => return Err(MalformedStatusLine),
    };

    Ok((hundreds - b'0') as u16 * 100 + (tens - b'0') as u16 * 10 + (ones - b'0') as u16)
}

fn parse_response_status(buf: &[u8]) -> Result<(Status, &[u8]), HttpParsingError> {
    let code = parse_response_status_code(buf)?;
    // check SP
    if buf.get(3).ok_or(HttpParsingError::MalformedStatusLine)? != &b' ' {
        return Err(HttpParsingError::MalformedStatusLine);
    }

    let buf = buf.get(4..).ok_or(HttpParsingError::MalformedStatusLine)?;
    let mut i = 0;
    while i + 1 < buf.len() {
        let c = buf[i];
        if c == b'\r' && buf[i + 1] == b'\n' {
            // safety: we just validated that all chars in buf[..i] are utf8
            let reason = unsafe { std::str::from_utf8_unchecked(&buf[..i]) };
            let rest = buf
                .get(i + 2..) // skip \r\n
                .ok_or(HttpParsingError::MalformedStatusLine)?;
            return Ok((Status::borrowed(code, reason), rest));
        }
        if !(c == b'\t' || c == b' ' || (0x21..=0x7E).contains(&c)) {
            // NB! extended Latin-1 is not allowed because not utf-8
            return Err(HttpParsingError::MalformedStatusLine);
        }
        i += 1;
    }
    Err(HttpParsingError::MalformedStatusLine)
}

impl<'b> Request<'b> {
    pub fn parse(buf: &'b [u8]) -> Result<Request<'b>, HttpParsingError> {
        let start = buf.len();
        let (method, rest) = parse_method(buf)?;
        let (uri, rest) = parse_uri(rest)?;
        let (http_version, rest) = parse_version(rest)?;
        let rest = rest.get(2..).ok_or(HttpParsingError::UnexpectedEof)?; // skip \r\n
        let (headers, rest) = parse_headers(rest)?;

        // return buf offset
        Ok(Request {
            method,
            uri,
            http_version,
            headers,
            buf_offset: start - rest.len(),
        })
    }
}

#[inline]
fn parse_headers(buf: &[u8]) -> Result<(Headers, &[u8]), HttpParsingError> {
    let mut headers = Headers::new();

    let mut buf = buf;
    let mut i = 0;
    while i < buf.len().saturating_sub(1) {
        if buf[i] == b'\r' && buf[i + 1] == b'\n' {
            let line = &buf[..i];
            if line.is_empty() {
                return Ok((headers, &buf[2..]));
            }

            let (name, value) = parse_header_line(line)?;
            headers.add(name, value);

            // Advance to next header line
            buf = &buf[i + 2..];
            i = 0;
        } else {
            i += 1;
        }
    }

    Err(HttpParsingError::UnexpectedEof)
}

#[inline]
fn parse_header_line(line: &[u8]) -> Result<(&str, &[u8]), HttpParsingError> {
    for (i, b) in line.iter().enumerate() {
        if *b == b':' {
            unsafe {
                for c in line[..i].iter() {
                    if !is_valid_header_field_byte(*c) {
                        return Err(HttpParsingError::MalformedHeader);
                    }
                }
                let name_str = std::str::from_utf8_unchecked(&line[..i]);
                let value = &line[i + 1..].trim_ascii_start();
                return Ok((name_str, value));
            }
        }
    }
    Err(HttpParsingError::MalformedHeader) // no ':' found
}

// -------------------------------------------------------------------------
// Utils
// -------------------------------------------------------------------------

#[inline]
fn parse_version(buf: &[u8]) -> Result<(u8, &[u8]), HttpParsingError> {
    const PREFIX: &[u8] = b"HTTP/";

    // HTTP/1.x takes 8 chars
    let rest = buf.get(8..).ok_or(HttpParsingError::UnexpectedEof)?;
    if &buf[..PREFIX.len()] != PREFIX {
        return Err(HttpParsingError::UnsupportedHttpVersion);
    }
    let version_number = &buf[PREFIX.len()..PREFIX.len() + 3];
    match version_number {
        b"1.0" => Ok((0, rest)),
        b"1.1" => Ok((1, rest)),
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
fn is_valid_header_field_byte(b: u8) -> bool {
    HEADER_FIELD_BYTE_MASK[b as usize]
}

static URI_BYTE_MASK: [bool; 256] = make_uri_byte_mask();

#[inline(always)]
fn is_valid_uri_byte(b: u8) -> bool {
    URI_BYTE_MASK[b as usize]
}

#[inline]
fn parse_uri(buf: &[u8]) -> Result<(RequestUri, &[u8]), HttpParsingError> {
    use HttpParsingError::MalformedStatusLine;

    let mut scheme_i = 0; // 0 → no “://” scheme
    let mut path_i_start = 0; // 0 → origin-form  (“/foo”)
    let mut path_i_end = 0; // exclusive; will be set later

    // Step 1: classify first byte
    let first = *buf.first().ok_or(MalformedStatusLine)?;
    let origin_form = match first {
        b' ' => return Err(MalformedStatusLine),
        b'*' => {
            return Ok((
                RequestUri::new("*", 0, 0, 1),
                buf.get(1..).ok_or(MalformedStatusLine)?,
            ));
        }
        b'/' => true,
        _ => false,
    };

    // Step 2: advance to start of path
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

        // no slash found → authority-form ("example.com:443")
        if path_i_start == 0 {
            // TODO: test this
            let uri = unsafe { std::str::from_utf8_unchecked(&buf[..i]) };
            return Ok((
                RequestUri::new(uri, 0, 0, 0),
                buf.get(i + 1..).ok_or(MalformedStatusLine)?,
            ));
        }
    }

    // Step 3: scan path/query until the SP
    while i < buf.len() {
        let b = buf[i];

        match b {
            b' ' => {
                if path_i_end == 0 {
                    path_i_end = i; // set end if we never saw '?'
                }
                break; // end of request-target
            }
            b'?' if path_i_end == 0 => {
                // first '?' marks end-of-path
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
        // we never hit SP -> malformed
        return Err(MalformedStatusLine);
    }

    // SAFETY: every byte already validated as US-ASCII subset
    let uri = unsafe { std::str::from_utf8_unchecked(&buf[..i]) };

    // skip the space so `rest` starts with “HTTP/…”
    let rest = buf.get(i + 1..).ok_or(MalformedStatusLine)?;

    Ok((
        RequestUri::new(uri, scheme_i, path_i_start, path_i_end),
        rest,
    ))
}

#[inline]
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
