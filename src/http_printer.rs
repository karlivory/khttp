use std::io::{self, BufWriter, Read, Write};

use crate::common::{HTTP_VERSION, HttpHeaders, HttpMethod, HttpStatus};

static CRLF: &[u8] = b"\r\n";

pub struct HttpPrinter<W: Write> {
    writer: BufWriter<W>,
}

impl<W: Write> HttpPrinter<W> {
    pub fn new(stream: W) -> Self {
        Self {
            writer: BufWriter::new(stream),
        }
    }

    pub fn write_response_fast(
        &mut self,
        status: &HttpStatus,
        headers: &HttpHeaders,
        body: &[u8],
    ) -> io::Result<()> {
        let head = build_response_head(status, headers);
        self.writer.write_all(&head)?;
        self.writer.write_all(body)
    }

    pub fn write_response_streaming(
        &mut self,
        status: &HttpStatus,
        headers: &HttpHeaders,
        mut body: impl Read,
    ) -> io::Result<()> {
        let head = build_response_head(status, headers);
        self.writer.write_all(&head)?;
        copy_stream(&mut body, &mut self.writer)
    }

    pub fn write_request(
        &mut self,
        method: &HttpMethod,
        uri: &str,
        headers: &HttpHeaders,
        mut body: impl Read,
    ) -> io::Result<()> {
        let head = build_request_head(method, uri, headers);
        self.writer.write_all(&head)?;
        copy_stream(&mut body, &mut self.writer)
    }
}

#[inline]
fn get_head_vector(header_count: usize) -> Vec<u8> {
    // rough guess: 64 bytes status + 40 bytes per header
    Vec::with_capacity(64 + header_count * 40)
}

fn build_response_head(status: &HttpStatus, headers: &HttpHeaders) -> Vec<u8> {
    let mut head = get_head_vector(headers.get_count());

    // status line
    head.extend_from_slice(HTTP_VERSION.as_bytes());
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
    head.extend_from_slice(HTTP_VERSION.as_bytes());
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

fn copy_stream<R: Read, W: Write>(src: &mut R, dst: &mut W) -> io::Result<()> {
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = src.read(&mut buf)?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n])?;
    }
    Ok(())
}
