use super::{HttpParsingError, HttpParsingError::*, parse_headers, parse_version};
use crate::{Headers, Method, RequestUri};

#[derive(Debug)]
pub struct Request<'b> {
    pub method: Method,
    pub uri: RequestUri<'b>,
    pub http_version: u8,
    pub headers: Headers<'b>,
    pub buf_offset: usize,
}

impl<'b> Request<'b> {
    pub fn parse(buf: &'b [u8]) -> Result<Request<'b>, HttpParsingError> {
        let start = buf.len();
        let (method, rest) = parse_method(buf)?;
        let (uri, rest) = parse_uri(rest)?;
        let (http_version, rest) = parse_version(rest)?;
        let rest = rest.get(2..).ok_or(UnexpectedEof)?; // skip "\r\n"
        let (headers, rest) = parse_headers(rest)?;

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
fn parse_method(buf: &[u8]) -> Result<(Method, &[u8]), HttpParsingError> {
    let mut i = 0;

    while i < buf.len() {
        let b = buf[i];
        if b == b' ' {
            let method_bytes = &buf[..i];

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
                    if !method_bytes.iter().all(|b| b.is_ascii_alphabetic()) {
                        return Err(MalformedStatusLine);
                    }
                    let s = unsafe { std::str::from_utf8_unchecked(method_bytes) };
                    Method::Custom(s.to_string())
                }
            };

            return Ok((method, &buf[i + 1..]));
        }

        i += 1;
    }

    Err(MalformedStatusLine)
}

#[inline]
fn parse_uri(buf: &[u8]) -> Result<(RequestUri, &[u8]), HttpParsingError> {
    let mut scheme_i = 0; // 0 -> no “://” scheme
    let mut path_i_start = 0; // 0 -> uri is in origin-form (“/foo”)
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

static URI_BYTE_MASK: [bool; 256] = make_uri_byte_mask();

#[inline(always)]
fn is_valid_uri_byte(b: u8) -> bool {
    URI_BYTE_MASK[b as usize]
}
