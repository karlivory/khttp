use crate::Headers;
use HttpParsingError::*;
use std::{error::Error, fmt::Display, io};

mod request;
mod response;
pub use request::Request;
pub use response::Response;

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

    Err(UnexpectedEof)
}

#[inline]
fn parse_header_line(line: &[u8]) -> Result<(&str, &[u8]), HttpParsingError> {
    for (i, b) in line.iter().enumerate() {
        if *b == b':' {
            unsafe {
                for c in line[..i].iter() {
                    if !is_valid_header_field_byte(*c) {
                        return Err(MalformedHeader);
                    }
                }
                let name_str = std::str::from_utf8_unchecked(&line[..i]);
                let value = &line[i + 1..].trim_ascii_start();
                return Ok((name_str, value));
            }
        }
    }
    Err(MalformedHeader) // no ':' found
}

#[inline]
fn parse_version(buf: &[u8]) -> Result<(u8, &[u8]), HttpParsingError> {
    const PREFIX: &[u8] = b"HTTP/";

    // HTTP/1.x takes 8 chars
    let rest = buf.get(8..).ok_or(UnexpectedEof)?;
    if &buf[..PREFIX.len()] != PREFIX {
        return Err(UnsupportedHttpVersion);
    }
    let version_number = &buf[PREFIX.len()..PREFIX.len() + 3];
    match version_number {
        b"1.0" => Ok((0, rest)),
        b"1.1" => Ok((1, rest)),
        _ => Err(UnsupportedHttpVersion),
    }
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
