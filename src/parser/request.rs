use super::{
    parse_headers, parse_version,
    simd::{match_path_vectored, match_uri_vectored},
    HttpParsingError::{self, *},
};
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
    // hot paths: GET and POST
    if let Some(rest) = buf.strip_prefix(b"GET ") {
        return Ok((Method::Get, rest));
    }
    if let Some(rest) = buf.strip_prefix(b"POST ") {
        return Ok((Method::Post, rest));
    }

    let mut i = 0;
    while i < buf.len() {
        let b = buf[i];
        if b == b' ' {
            let method_bytes = &buf[..i];

            let method = match method_bytes {
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

    Err(UnexpectedEof) // no 'SP' found
}

#[inline]
fn parse_uri(buf: &[u8]) -> Result<(RequestUri<'_>, &[u8]), HttpParsingError> {
    // step 1: classify first byte
    let origin_form = match *buf.first().ok_or(UnexpectedEof)? {
        b'/' => true,
        b'*' => {
            return Ok((
                RequestUri::new("*", 0, 1),
                buf.get(1..).ok_or(MalformedStatusLine)?,
            ));
        }
        _ => false,
    };

    let mut path_start_i = 0; // start of path (e.g. -> /api/v1/...)

    // step 2: advance to start of path (absolute- or authority-form)
    let mut i = 0;
    if !origin_form {
        // scan for the first slash that is NOT part of "://"
        while i < buf.len() {
            let b = buf[i];

            match b {
                // skip the scheme separator "://"
                b':' if i + 2 < buf.len() && &buf[i..i + 3] == b"://" => {
                    i += 3;
                    continue;
                }
                // start of path
                b'/' => {
                    path_start_i = i;
                    break;
                }
                // end of uri
                b' ' => {
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

        // no slash found => authority-form ("example.com:443")
        if path_start_i == 0 {
            // validate up to the mandatory space
            let mut j = i;
            j += match_uri_vectored(&buf[j..]);
            match buf.get(j).copied() {
                Some(b' ') => {
                    // SAFETY: ASCII subset validated byte-by-byte above
                    let uri = unsafe { core::str::from_utf8_unchecked(&buf[..j]) };
                    let rest = buf.get(j + 1..).ok_or(MalformedStatusLine)?;
                    return Ok((RequestUri::new(uri, 0, 0), rest));
                }
                Some(_) => return Err(MalformedStatusLine),
                None => return Err(MalformedStatusLine),
            }
        }
    }

    // Step 3: we are at start of path (`i` points at '/'; or 0 for origin-form).
    // scan path up to first '?' or SP
    i += match_path_vectored(&buf[i..]);

    // i is now equal to end of path (OR first illegal character)
    let path_end_i = i;

    // if next byte is '?', consume query using bulk URI validation until SP.
    if let Some(&b'?') = buf.get(i) {
        i += 1; // skip '?'
        let n = match_uri_vectored(&buf[i..]); // scan path up to SP
        i += n;
        match buf.get(i).copied() {
            Some(b' ') => {}                            // all good, we found the SP
            Some(_) => return Err(MalformedStatusLine), // invalid char
            None => return Err(UnexpectedEof),          // TODO: is this correct?
        }
    } else {
        // otherwise we must be at SP right after the path
        match buf.get(i) {
            Some(b' ') => {}
            Some(_) => return Err(MalformedStatusLine),
            None => return Err(UnexpectedEof),
        }
    }

    // SAFETY: every byte validated as US-ASCII subset (within UTF8)
    let uri = unsafe { core::str::from_utf8_unchecked(&buf[..i]) };

    let rest = buf.get(i + 1..).ok_or(UnexpectedEof)?; // skip space
    Ok((RequestUri::new(uri, path_start_i, path_end_i), rest))
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
pub(crate) fn is_valid_uri_byte(b: u8) -> bool {
    URI_BYTE_MASK[b as usize]
}
