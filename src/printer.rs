use crate::{Headers, Method, Status};
use std::io::{self, BufWriter, Read, Write};

const HTTP_VERSION: &[u8] = b"HTTP/1.1";
const CRLF: &[u8] = b"\r\n";
const PROBE_MAX: usize = 8 * 1024;
const RESPONSE_100_CONTINUE: &[u8] = b"HTTP/1.1 100 Continue\r\n\r\n";

const CONTENT_LENGTH_HEADER: &[u8] = b"content-length";

pub struct HttpPrinter<W: Write> {
    writer: BufWriter<W>,
}

impl<W: Write> HttpPrinter<W> {
    pub fn new(stream: W) -> Self {
        Self {
            writer: BufWriter::new(stream),
        }
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    pub fn write_response<R: Read>(
        &mut self,
        status: &Status,
        mut headers: Headers,
        body: R,
    ) -> io::Result<()> {
        let strat = decide_body_strategy(&mut headers, body)?;
        let head = build_response_head(status, &headers);
        self.dispatch(head, strat)
    }

    pub fn write_request<R: Read>(
        &mut self,
        method: &Method,
        uri: &str,
        mut headers: Headers,
        body: R,
    ) -> io::Result<()> {
        let strat = decide_body_strategy(&mut headers, body)?;
        let head = build_request_head(method, uri, &headers);
        self.dispatch(head, strat)
    }

    pub fn write_100_continue(&mut self) -> io::Result<()> {
        self.writer.write_all(RESPONSE_100_CONTINUE)?;
        self.writer.flush()
    }

    fn write_fast(&mut self, head: &[u8], body: &[u8]) -> io::Result<()> {
        self.writer.write_all(head)?;
        self.writer.write_all(body)
    }

    fn write_streaming<R: Read>(&mut self, head: &[u8], mut body: R) -> io::Result<()> {
        self.writer.write_all(head)?;
        std::io::copy(&mut body, &mut self.writer).map(|_| ())
    }

    fn write_chunked<R: Read>(
        &mut self,
        head: &[u8],
        prefix: &[u8],
        mut body: R,
    ) -> io::Result<()> {
        self.writer.write_all(head)?;

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

    fn dispatch<R: Read>(&mut self, head: Vec<u8>, strat: BodyStrategy<R>) -> io::Result<()> {
        match strat {
            BodyStrategy::Fast(buf) => self.write_fast(&head, &buf),
            BodyStrategy::Streaming(reader) => self.write_streaming(&head, reader),
            BodyStrategy::Chunked { prefix, reader } => self.write_chunked(&head, &prefix, reader),
        }
    }
}

// -------------------------------------------------------------------------
// BODY STRATEGY SELECTION
// -------------------------------------------------------------------------

enum BodyStrategy<R: Read> {
    Fast(Vec<u8>),
    Streaming(R),
    Chunked { prefix: Vec<u8>, reader: R },
}

fn decide_body_strategy<R: Read>(
    headers: &mut Headers,
    mut body: R,
) -> io::Result<BodyStrategy<R>> {
    // TE: chunked explicitly requested
    if headers.is_transfer_encoding_chunked() {
        headers.remove(Headers::CONTENT_LENGTH);
        headers.set_transfer_encoding_chunked();
        return Ok(BodyStrategy::Chunked {
            prefix: Vec::new(),
            reader: body,
        });
    }

    // Caller provided CL
    if let Some(cl) = headers.get_content_length() {
        const FAST_LIMIT: u64 = PROBE_MAX as u64;
        if cl <= FAST_LIMIT {
            let mut buf = Vec::with_capacity(cl as usize);
            let mut limited = body.by_ref().take(cl);
            limited.read_to_end(&mut buf)?;
            return Ok(BodyStrategy::Fast(buf));
        } else {
            return Ok(BodyStrategy::Streaming(body));
        }
    }

    // No CL, no TE -> probe
    let (prefix, complete) = probe_body(&mut body, PROBE_MAX)?;
    if complete {
        headers.set_content_length(prefix.len() as u64);
        Ok(BodyStrategy::Fast(prefix))
    } else {
        headers.set_transfer_encoding_chunked();
        Ok(BodyStrategy::Chunked {
            prefix,
            reader: body,
        })
    }
}

// -------------------------------------------------------------------------
// HEAD CONSTRUCTION
// -------------------------------------------------------------------------

#[inline(always)]
fn get_head_vector(header_count: usize) -> Vec<u8> {
    // rough guess: 64 bytes status + 40 bytes per header
    Vec::with_capacity(64 + header_count * 40)
}

fn build_response_head(status: &Status, headers: &Headers) -> Vec<u8> {
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

fn build_request_head(method: &Method, uri: &str, headers: &Headers) -> Vec<u8> {
    let mut head = get_head_vector(headers.get_count());

    // request line
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

fn add_headers(buf: &mut Vec<u8>, headers: &Headers) {
    for (k, values) in headers.get_map() {
        for v in values {
            buf.extend_from_slice(k.as_bytes());
            buf.extend_from_slice(b": ");
            buf.extend_from_slice(v.as_bytes());
            buf.extend_from_slice(CRLF);
        }
    }
    if let Some(cl) = headers.get_content_length() {
        buf.extend_from_slice(CONTENT_LENGTH_HEADER);
        buf.extend_from_slice(b": ");
        let mut num_buf = [0u8; 20]; // enough to hold any u64 in base 10
        let len = u64_to_ascii_buf(cl, &mut num_buf);
        buf.extend_from_slice(&num_buf[..len]);
        buf.extend_from_slice(CRLF);
    }
}

// -------------------------------------------------------------------------
// UTILS
// -------------------------------------------------------------------------

fn u64_to_ascii_buf(mut n: u64, buf: &mut [u8; 20]) -> usize {
    if n == 0 {
        buf[0] = b'0';
        return 1;
    }

    let mut i = 20;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }

    let len = 20 - i;
    buf.copy_within(i..20, 0);
    len
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
