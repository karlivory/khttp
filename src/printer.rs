use crate::{Headers, Method, Status};
use std::io::{self, BufWriter, Read, Write};

const HTTP_VERSION: &[u8] = b"HTTP/1.1";
const CRLF: &[u8] = b"\r\n";
const PROBE_MAX: usize = 8 * 1024;
const RESPONSE_100_CONTINUE: &[u8] = b"HTTP/1.1 100 Continue\r\n\r\n";

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
        headers: &Headers,
        body: R,
    ) -> io::Result<()> {
        let strat = decide_body_strategy(headers, body)?;
        let head = build_response_head(status, headers, &strat);
        self.dispatch(head, strat)
    }

    pub fn write_request<R: Read>(
        &mut self,
        method: &Method,
        uri: &str,
        headers: &Headers,
        body: R,
    ) -> io::Result<()> {
        let strat = decide_body_strategy(headers, body)?;
        let head = build_request_head(method, uri, headers, &strat);
        self.dispatch(head, strat)
    }

    #[inline]
    pub fn write_100_continue(&mut self) -> io::Result<()> {
        self.writer.write_all(RESPONSE_100_CONTINUE)?;
        self.writer.flush()
    }

    #[inline]
    fn write_fast(&mut self, body: &[u8]) -> io::Result<()> {
        self.writer.write_all(body)
    }

    #[inline]
    fn write_streaming<R: Read>(&mut self, mut body: R) -> io::Result<()> {
        std::io::copy(&mut body, &mut self.writer).map(|_| ())
    }

    #[inline]
    fn write_chunked<R: Read>(&mut self, mut body: R) -> io::Result<()> {
        // TODO: why is this slow in benchmarks?
        let mut buf = [0u8; 8 * 1024];
        loop {
            let n = body.read(&mut buf)?;
            if n == 0 {
                break;
            }
            self.write_chunk(&buf[..n])?;
        }

        // terminating chunk
        self.writer.write_all(b"0\r\n\r\n")
    }

    #[inline]
    fn write_chunk(&mut self, bytes: &[u8]) -> io::Result<()> {
        write!(&mut self.writer, "{:X}\r\n", bytes.len())?;
        self.writer.write_all(bytes)?;
        self.writer.write_all(CRLF)
    }

    #[inline]
    fn dispatch<R: Read>(&mut self, head: Vec<u8>, strat: BodyStrategy<R>) -> io::Result<()> {
        self.writer.write_all(&head)?;
        match strat {
            BodyStrategy::Fast(buf, _) => self.write_fast(&buf),
            BodyStrategy::Streaming(reader, _) => self.write_streaming(reader),
            BodyStrategy::Chunked { reader } => self.write_chunked(reader),
            BodyStrategy::AutoChunked { prefix, reader } => {
                self.write_chunk(&prefix)?;
                self.write_chunked(reader)
            }
        }
    }
}

// -------------------------------------------------------------------------
// BODY STRATEGY SELECTION
// -------------------------------------------------------------------------

enum BodyStrategy<R: Read> {
    Fast(Vec<u8>, u64),
    Streaming(R, u64),
    Chunked { reader: R },
    AutoChunked { prefix: Vec<u8>, reader: R },
}

fn decide_body_strategy<R: Read>(headers: &Headers, mut body: R) -> io::Result<BodyStrategy<R>> {
    // TE: chunked explicitly requested
    if headers.is_transfer_encoding_chunked() {
        return Ok(BodyStrategy::Chunked { reader: body });
    }

    // Caller provided CL
    if let Some(cl) = headers.get_content_length() {
        const FAST_LIMIT: u64 = PROBE_MAX as u64;
        if cl <= FAST_LIMIT {
            let mut buf = Vec::with_capacity(cl as usize);
            let mut limited = body.by_ref().take(cl);
            limited.read_to_end(&mut buf)?;
            return Ok(BodyStrategy::Fast(buf, cl));
        } else {
            return Ok(BodyStrategy::Streaming(body, cl));
        }
    }

    // No CL, no TE -> probe
    let (prefix, complete) = probe_body(&mut body, PROBE_MAX)?;
    if complete {
        let cl = prefix.len();
        Ok(BodyStrategy::Fast(prefix, cl as u64))
    } else {
        Ok(BodyStrategy::AutoChunked {
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

fn build_response_head<R: Read>(
    status: &Status,
    headers: &Headers,
    strat: &BodyStrategy<R>,
) -> Vec<u8> {
    let mut head = get_head_vector(headers.get_count());

    // status line
    head.extend_from_slice(HTTP_VERSION);
    head.extend_from_slice(b" ");
    head.extend_from_slice(&u16_to_ascii_3digits(status.code));
    head.extend_from_slice(b" ");
    head.extend_from_slice(status.reason.as_bytes());
    head.extend_from_slice(CRLF);

    // headers
    add_headers(&mut head, headers, strat);

    // finalize
    head.extend_from_slice(CRLF);
    head
}

fn build_request_head<R: Read>(
    method: &Method,
    uri: &str,
    headers: &Headers,
    strat: &BodyStrategy<R>,
) -> Vec<u8> {
    let mut head = get_head_vector(headers.get_count());

    // request line
    head.extend_from_slice(method.as_str().as_bytes());
    head.extend_from_slice(b" ");
    head.extend_from_slice(uri.as_bytes());
    head.extend_from_slice(b" ");
    head.extend_from_slice(HTTP_VERSION);
    head.extend_from_slice(CRLF);

    // headers
    add_headers(&mut head, headers, strat);

    // finalize
    head.extend_from_slice(CRLF);
    head
}

const CONTENT_LENGTH_HEADER: &[u8] = b"content-length: ";
const TRANSFER_ENCODING_HEADER: &[u8] = b"transfer-encoding: ";
const TRANSFER_ENCODING_HEADER_CHUNKED: &[u8] = b"transfer-encoding: chunked";
const CONNECTION_HEADER: &[u8] = b"connection: close";

#[inline]
fn add_content_length_header(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(CONTENT_LENGTH_HEADER);
    let mut num_buf = [0u8; 20]; // enough to hold any u64 in base 10
    let len = u64_to_ascii_buf(value, &mut num_buf);
    buf.extend_from_slice(&num_buf[..len]);
}

#[inline]
fn add_headers<R: Read>(buf: &mut Vec<u8>, headers: &Headers, strat: &BodyStrategy<R>) {
    for (name, value) in headers.iter() {
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(value);
        buf.extend_from_slice(CRLF);
    }
    // // set "connection" header
    let connection = headers.get_connection_values();
    if !connection.is_empty() {
        buf.extend_from_slice(CONNECTION_HEADER);
        buf.extend_from_slice(connection);
        buf.extend_from_slice(CRLF);
    }
    // // set framing headers ("Transfer-Encoding" OR "Content-Length")
    match strat {
        BodyStrategy::Fast(_, cl) => {
            add_content_length_header(buf, *cl);
        }
        BodyStrategy::Streaming(_, cl) => {
            add_content_length_header(buf, *cl);
        }
        BodyStrategy::Chunked { .. } => {
            let encodings = headers.get_transfer_encoding();
            debug_assert!(!encodings.is_empty());
            buf.extend_from_slice(TRANSFER_ENCODING_HEADER);
            buf.extend_from_slice(encodings);
        }
        BodyStrategy::AutoChunked { .. } => {
            debug_assert!(!headers.is_transfer_encoding_chunked());
            buf.extend_from_slice(TRANSFER_ENCODING_HEADER_CHUNKED);
            let encodings = headers.get_transfer_encoding();
            if !encodings.is_empty() {
                buf.extend_from_slice(b", ");
                buf.extend_from_slice(encodings);
            }
        }
    }
    buf.extend_from_slice(CRLF);
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

fn u16_to_ascii_3digits(n: u16) -> [u8; 3] {
    let hundreds = n / 100;
    let tens = (n / 10) % 10;
    let ones = n % 10;

    [
        b'0' + (hundreds as u8),
        b'0' + (tens as u8),
        b'0' + (ones as u8),
    ]
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
