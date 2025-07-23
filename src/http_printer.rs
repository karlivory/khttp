// src/http_printer.rs
use crate::common::{HttpHeaders, HttpMethod, HttpStatus, TransferEncoding};
use std::io::{self, BufWriter, Read, Write};

const HTTP_VERSION: &[u8] = b"HTTP/1.1";
const CRLF: &[u8] = b"\r\n";
const PROBE_MAX: usize = 8 * 1024;

pub struct HttpPrinter<W: Write> {
    writer: BufWriter<W>,
}

impl<W: Write> HttpPrinter<W> {
    pub fn new(stream: W) -> Self {
        Self {
            writer: BufWriter::with_capacity(128 * 1024, stream),
        }
    }

    // -------------------------------------------------------------------------
    // WRITING RESPONSE
    // -------------------------------------------------------------------------

    pub fn write_response(
        &mut self,
        status: &HttpStatus,
        mut headers: HttpHeaders,
        mut body: impl Read,
    ) -> io::Result<()> {
        if let Some(te) = headers.get(HttpHeaders::TRANSFER_ENCODING) {
            if te.eq_ignore_ascii_case(TransferEncoding::CHUNKED) {
                // Transfer-Encoding: chunked
                return self.write_response_chunked(status, headers, &[], body);
            } else {
                // NB! document this: only "chunked" encoding is supported
                headers.remove(HttpHeaders::TRANSFER_ENCODING);
            }
        }

        if let Some(cl) = headers.get_content_length() {
            if cl <= PROBE_MAX {
                // fast path: read body into a Vec once
                // also: reconfigure CL if it was set incorrectly
                let mut buf = Vec::with_capacity(cl);
                let mut limited = body.by_ref().take(cl as u64);
                limited.read_to_end(&mut buf)?;
                headers.set_content_length(buf.len());
                return self.write_response_fast(status, &headers, &buf);
            } else {
                // big body with CL: stream directly, no probe
                return self.write_response_streaming(status, &headers, &[], body);
            }
        }

        // No CL, no TE: probe to see if the body is small enough
        let (prefix, complete) = probe_body(&mut body, PROBE_MAX)?;
        if complete {
            // fast path
            headers.set_content_length(prefix.len());
            self.write_response_fast(status, &headers, &prefix)
        } else {
            // Larger than probe -> chunked
            self.write_response_chunked(status, headers, &prefix, body)
        }
    }

    fn write_response_chunked(
        &mut self,
        status: &HttpStatus,
        mut headers: HttpHeaders,
        prefix: &[u8],
        mut body: impl Read,
    ) -> io::Result<()> {
        headers.remove(HttpHeaders::CONTENT_LENGTH);
        headers.set_transfer_encoding_chunked();

        let head = build_response_head(status, &headers);
        self.writer.write_all(&head)?;

        if !prefix.is_empty() {
            write_chunk(&mut self.writer, prefix)?;
        }

        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = body.read(&mut buf)?;
            if n == 0 {
                break;
            }
            write_chunk(&mut self.writer, &buf[..n])?;
        }

        // terminating chunk
        self.writer.write_all(b"0\r\n\r\n")
    }

    fn write_response_fast(
        &mut self,
        status: &HttpStatus,
        headers: &HttpHeaders,
        body: &[u8],
    ) -> io::Result<()> {
        let head = build_response_head(status, headers);
        self.writer.write_all(&head)?;
        self.writer.write_all(body)
    }

    fn write_response_streaming(
        &mut self,
        status: &HttpStatus,
        headers: &HttpHeaders,
        prefix: &[u8],
        mut body: impl Read,
    ) -> io::Result<()> {
        let head = build_response_head(status, headers);
        self.writer.write_all(&head)?;
        if !prefix.is_empty() {
            self.writer.write_all(prefix)?;
        }
        std::io::copy(&mut body, &mut self.writer).map(|_| ())
    }

    // -------------------------------------------------------------------------
    // WRITING REQUEST
    // -------------------------------------------------------------------------

    pub fn write_request(
        &mut self,
        method: &HttpMethod,
        uri: &str,
        headers: &HttpHeaders,
        mut body: impl Read,
    ) -> io::Result<()> {
        let head = build_request_head(method, uri, headers);
        self.writer.write_all(&head)?;
        std::io::copy(&mut body, &mut self.writer).map(|_| ())
    }
}

#[inline(always)]
fn get_head_vector(header_count: usize) -> Vec<u8> {
    // rough guess: 64 bytes status + 40 bytes per header
    Vec::with_capacity(64 + header_count * 40)
}

fn build_response_head(status: &HttpStatus, headers: &HttpHeaders) -> Vec<u8> {
    let mut head = get_head_vector(headers.get_count());

    // status line
    head.extend_from_slice(HTTP_VERSION);
    head.extend_from_slice(b" ");
    head.extend_from_slice(status.code.to_string().as_bytes());
    head.extend_from_slice(b" ");
    head.extend_from_slice(status.reason.as_bytes());
    head.extend_from_slice(CRLF);

    // headers
    add_headers(&mut head, headers);
    head.extend_from_slice(CRLF);

    head
}

fn build_request_head(method: &HttpMethod, uri: &str, headers: &HttpHeaders) -> Vec<u8> {
    let mut head = get_head_vector(headers.get_count());

    // status line
    head.extend_from_slice(method.to_string().as_bytes());
    head.extend_from_slice(b" ");
    head.extend_from_slice(uri.as_bytes());
    head.extend_from_slice(b" ");
    head.extend_from_slice(HTTP_VERSION);
    head.extend_from_slice(CRLF);

    // headers
    add_headers(&mut head, headers);
    head.extend_from_slice(CRLF);

    head
}

fn add_headers(buf: &mut Vec<u8>, headers: &HttpHeaders) {
    for (k, v) in headers.get_map() {
        buf.extend_from_slice(k.as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(v.as_bytes());
        buf.extend_from_slice(CRLF);
    }
}

fn write_chunk<W: Write>(dst: &mut W, bytes: &[u8]) -> io::Result<()> {
    write!(dst, "{:X}\r\n", bytes.len())?;
    dst.write_all(bytes)?;
    dst.write_all(CRLF)
}

fn probe_body<R: Read>(src: &mut R, max: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut collected = Vec::with_capacity(max.min(4096));
    let mut buf = [0u8; 1024];
    while collected.len() < max {
        let to_read = (max - collected.len()).min(buf.len());
        let n = src.read(&mut buf[..to_read])?;
        if n == 0 {
            return Ok((collected, true));
        }
        collected.extend_from_slice(&buf[..n]);
    }
    Ok((collected, false))
}
