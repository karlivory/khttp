use super::{HttpParsingError, HttpParsingError::*, parse_headers, parse_version};
use crate::{Headers, Status};

#[derive(Debug)]
pub struct Response<'b> {
    pub http_version: u8,
    pub status: Status<'b>,
    pub headers: Headers<'b>,
    pub buf_offset: usize,
}

impl<'b> Response<'b> {
    pub fn parse(buf: &'b [u8]) -> Result<Response<'b>, HttpParsingError> {
        let start = buf.len();
        let (http_version, rest) = parse_version(buf)?;
        let rest = rest.get(1..).ok_or(MalformedStatusLine)?; // skip single SP
        let (status, rest) = parse_response_status(rest)?;
        let (headers, rest) = parse_headers(rest)?;

        Ok(Response {
            http_version,
            status,
            headers,
            buf_offset: start - rest.len(),
        })
    }
}

#[inline]
fn parse_response_status(buf: &[u8]) -> Result<(Status, &[u8]), HttpParsingError> {
    let code = parse_response_status_code(buf)?;
    // check SP
    if buf.get(3).ok_or(MalformedStatusLine)? != &b' ' {
        return Err(MalformedStatusLine);
    }

    let buf = buf.get(4..).ok_or(MalformedStatusLine)?;
    let mut i = 0;
    while i + 1 < buf.len() {
        let c = buf[i];
        if c == b'\r' && buf[i + 1] == b'\n' {
            // safety: we just validated that all chars in buf[..i] are utf8
            let reason = unsafe { std::str::from_utf8_unchecked(&buf[..i]) };
            let rest = buf
                .get(i + 2..) // skip \r\n
                .ok_or(MalformedStatusLine)?;
            return Ok((Status::borrowed(code, reason), rest));
        }
        if !(c == b'\t' || c == b' ' || (0x21..=0x7E).contains(&c)) {
            // NB! extended Latin-1 is not allowed because not utf-8
            return Err(MalformedStatusLine);
        }
        i += 1;
    }
    Err(MalformedStatusLine)
}

#[inline]
fn parse_response_status_code(buf: &[u8]) -> Result<u16, HttpParsingError> {
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
