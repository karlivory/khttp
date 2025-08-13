use crate::Headers;
use HttpParsingError::*;
use memchr::memchr;
use std::{error::Error, fmt::Display, io};

mod request;
mod response;
pub mod simd;
pub use request::Request;
pub use response::Response;

#[inline]
fn parse_headers(buf: &[u8]) -> Result<(Headers<'_>, &[u8]), HttpParsingError> {
    let mut headers = Headers::new();
    let mut buf = buf;

    loop {
        if let Some(rest) = buf.strip_prefix(b"\r\n") {
            return Ok((headers, rest));
        }

        // find '\n'
        let nl = match memchr(b'\n', buf) {
            Some(p) => p,
            None => return Err(UnexpectedEof),
        };

        // require CRLF
        if nl == 0 || buf[nl - 1] != b'\r' {
            buf = &buf[nl + 1..];
            continue;
        }

        let line = &buf[..nl - 1];
        let (name, value) = parse_header_line(line)?;
        headers.add(name, value);

        buf = &buf[nl + 1..];
    }
}
#[inline(always)]
fn parse_header_line(line: &[u8]) -> Result<(&str, &[u8]), HttpParsingError> {
    let colon = memchr(b':', line).ok_or(MalformedHeader)?;
    if !line[..colon]
        .iter()
        .copied()
        .all(is_valid_header_field_byte)
    {
        return Err(MalformedHeader);
    }

    let name_str = unsafe { std::str::from_utf8_unchecked(&line[..colon]) };
    let value = &line[colon + 1..].trim_ascii_start();
    Ok((name_str, value))
}

#[inline]
fn parse_version(buf: &[u8]) -> Result<(u8, &[u8]), HttpParsingError> {
    if let Some(rest) = buf.strip_prefix(b"HTTP/1.") {
        let (&minor, rest) = rest.split_first().ok_or(UnexpectedEof)?;
        return match minor {
            b'1' => Ok((1, rest)),
            b'0' => Ok((0, rest)),
            _ => Err(UnsupportedHttpVersion),
        };
    }

    if b"HTTP/1.".starts_with(&buf[..buf.len().min(7)]) {
        return Err(UnexpectedEof);
    }
    Err(UnsupportedHttpVersion)
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
