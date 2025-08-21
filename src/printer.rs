use crate::{Headers, Status};
use std::{
    io::{self, BufWriter, IoSlice, Read, Write},
    mem::MaybeUninit,
};

const CRLF: &[u8] = b"\r\n";
const PROBE_MAX: usize = 8 * 1024;
const INLINE_COPY_MAX: usize = 2 * 1024;
const RESPONSE_100_CONTINUE: &[u8] = b"HTTP/1.1 100 Continue\r\n\r\n";
const RESPONSE_HEAD_BUF_INIT_CAP: usize = 512;

pub struct HttpPrinter;

impl HttpPrinter {
    pub fn write_response_empty<W: Write>(
        mut writer: W,
        status: &Status,
        headers: &Headers,
    ) -> io::Result<()> {
        let mut head = Vec::with_capacity(RESPONSE_HEAD_BUF_INIT_CAP);

        // status line
        head.extend_from_slice(b"HTTP/1.1 ");
        head.extend_from_slice(&u16_to_ascii(status.code));
        head.extend_from_slice(status.reason.as_bytes());
        head.extend_from_slice(CRLF);

        // headers
        for (name, value) in headers.iter() {
            head.extend_from_slice(name.as_bytes());
            head.extend_from_slice(b": ");
            head.extend_from_slice(value);
            head.extend_from_slice(CRLF);
        }
        if headers.is_with_date_header() {
            let date_buf = crate::date::get_date_now();
            head.extend_from_slice(&date_buf);
        }
        head.extend_from_slice(b"content-length: 0\r\n\r\n");

        writer.write_all(&head)
    }

    pub fn write_response_bytes<W: Write>(
        writer: W,
        status: &Status,
        headers: &Headers,
        body: &[u8],
    ) -> io::Result<()> {
        let mut head = Vec::with_capacity(RESPONSE_HEAD_BUF_INIT_CAP);

        // status line
        head.extend_from_slice(b"HTTP/1.1 ");
        head.extend_from_slice(&u16_to_ascii(status.code));
        head.extend_from_slice(status.reason.as_bytes());
        head.extend_from_slice(CRLF);

        // headers
        for (name, value) in headers.iter() {
            head.extend_from_slice(name.as_bytes());
            head.extend_from_slice(b": ");
            head.extend_from_slice(value);
            head.extend_from_slice(CRLF);
        }
        if headers.is_with_date_header() {
            let date_buf = crate::date::get_date_now();
            head.extend_from_slice(&date_buf);
        }
        add_content_length_header(&mut head, body.len() as u64);
        head.extend_from_slice(CRLF);

        // body
        write_vectored_bytes(writer, head, body)
    }

    pub fn write_response<W: Write, R: Read>(
        writer: W,
        status: &Status,
        headers: &Headers,
        body: R,
    ) -> io::Result<()> {
        let strat = decide_body_strategy(headers, body)?;
        let head = build_response_head(status, headers, &strat);

        match strat {
            BodyStrategy::Fast(buf, _) => write_vectored_bytes(writer, head, &buf),
            BodyStrategy::Streaming(reader, _) => {
                let mut bw = BufWriter::new(writer);
                bw.write_all(&head)?;
                write_streaming(&mut bw, reader)
            }
            BodyStrategy::Chunked { reader } => {
                let mut bw = BufWriter::new(writer);
                bw.write_all(&head)?;
                write_chunked(bw, reader)
            }
            BodyStrategy::AutoChunked { prefix, reader } => {
                let mut bw = BufWriter::new(writer);
                bw.write_all(&head)?;
                write_chunk(&mut bw, &prefix)?;
                write_chunked(bw, reader)
            }
        }
    }

    #[cfg(feature = "client")]
    pub fn write_request<W: Write, R: Read>(
        writer: W,
        method: &crate::Method,
        uri: &str,
        headers: &Headers,
        mut body: R,
    ) -> io::Result<()> {
        let strat = decide_body_strategy(headers, &mut body)?;
        let head = build_request_head(method, uri, headers, &strat);

        match strat {
            BodyStrategy::Fast(buf, _) => write_vectored_bytes(writer, head, &buf),
            BodyStrategy::Streaming(reader, _) => {
                let mut bw = BufWriter::new(writer);
                bw.write_all(&head)?;
                write_streaming(&mut bw, reader)
            }
            BodyStrategy::Chunked { reader } => {
                let mut bw = BufWriter::new(writer);
                bw.write_all(&head)?;
                write_chunked(bw, reader)
            }
            BodyStrategy::AutoChunked { prefix, reader } => {
                let mut bw = BufWriter::new(writer);
                bw.write_all(&head)?;
                write_chunk(&mut bw, &prefix)?;
                write_chunked(bw, reader)
            }
        }
    }

    #[inline]
    pub fn write_100_continue<W: Write>(mut writer: W) -> io::Result<()> {
        writer.write_all(RESPONSE_100_CONTINUE)
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

#[inline]
fn decide_body_strategy<R: Read>(headers: &Headers, mut body: R) -> io::Result<BodyStrategy<R>> {
    if headers.is_transfer_encoding_chunked() {
        return Ok(BodyStrategy::Chunked { reader: body });
    }

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

#[inline]
fn build_response_head<R: Read>(
    status: &Status,
    headers: &Headers,
    strat: &BodyStrategy<R>,
) -> Vec<u8> {
    let mut head = Vec::with_capacity(RESPONSE_HEAD_BUF_INIT_CAP);

    head.extend_from_slice(b"HTTP/1.1 ");
    head.extend_from_slice(&u16_to_ascii(status.code));
    head.extend_from_slice(status.reason.as_bytes());
    head.extend_from_slice(CRLF);

    add_headers(&mut head, headers, strat);

    head.extend_from_slice(CRLF);
    head
}

#[cfg(feature = "client")]
fn build_request_head<R: Read>(
    method: &crate::Method,
    uri: &str,
    headers: &Headers,
    strat: &BodyStrategy<R>,
) -> Vec<u8> {
    let mut head = Vec::with_capacity(RESPONSE_HEAD_BUF_INIT_CAP);

    head.extend_from_slice(method.as_str().as_bytes());
    head.extend_from_slice(b" ");
    head.extend_from_slice(uri.as_bytes());
    head.extend_from_slice(b" ");
    head.extend_from_slice(b"HTTP/1.1\r\n");

    add_headers(&mut head, headers, strat);

    head.extend_from_slice(CRLF);
    head
}

const CONTENT_LENGTH_HEADER: &[u8] = b"content-length: ";
const TRANSFER_ENCODING_HEADER_CHUNKED: &[u8] = b"transfer-encoding: chunked\r\n";

#[inline]
fn add_content_length_header(buf: &mut Vec<u8>, value: u64) {
    buf.extend_from_slice(CONTENT_LENGTH_HEADER);
    let mut num_buf = [0u8; 20]; // enough to hold any u64 in base 10
    let len = u64_to_ascii_buf(value, &mut num_buf);
    buf.extend_from_slice(&num_buf[..len]);
    buf.extend_from_slice(CRLF);
}

#[inline]
fn add_headers<R: Read>(buf: &mut Vec<u8>, headers: &Headers, strat: &BodyStrategy<R>) {
    for (name, value) in headers.iter() {
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(b": ");
        buf.extend_from_slice(value);
        buf.extend_from_slice(CRLF);
    }
    if headers.is_with_date_header() {
        let date_buf = crate::date::get_date_now();
        buf.extend_from_slice(&date_buf);
    }
    match strat {
        BodyStrategy::Fast(_, cl) => add_content_length_header(buf, *cl),
        BodyStrategy::Streaming(_, cl) => add_content_length_header(buf, *cl),
        BodyStrategy::Chunked { .. } => { /* NOP (caller requested TE:chunked) */ }
        BodyStrategy::AutoChunked { .. } => {
            debug_assert!(!headers.is_transfer_encoding_chunked());
            buf.extend_from_slice(TRANSFER_ENCODING_HEADER_CHUNKED);
        }
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

fn u16_to_ascii(n: u16) -> [u8; 4] {
    let hundreds = n / 100;
    let tens = (n / 10) % 10;
    let ones = n % 10;

    [
        b'0' + (hundreds as u8),
        b'0' + (tens as u8),
        b'0' + (ones as u8),
        b' ',
    ]
}

fn probe_body<R: Read>(src: &mut R, max: usize) -> io::Result<(Vec<u8>, bool)> {
    let mut collected = Vec::with_capacity(128);

    while collected.len() < max {
        if collected.spare_capacity_mut().is_empty() {
            let need = (max - collected.len()).min(1024);
            collected.reserve(need);
        }
        let remaining = max - collected.len();

        let n = {
            let spare = collected.spare_capacity_mut();
            let to_read = remaining.min(spare.len());

            // SAFETY: we expose only the first `to_read` bytes to `read`, which will initialize up to `n` of them
            let buf =
                unsafe { std::slice::from_raw_parts_mut(spare.as_mut_ptr() as *mut u8, to_read) };
            src.read(buf)?
        };

        if n == 0 {
            return Ok((collected, true));
        }

        // SAFETY: `read` initialized exactly `n` bytes.
        unsafe { collected.set_len(collected.len() + n) };
    }

    Ok((collected, false))
}

#[inline]
fn write_vectored_bytes<W: Write>(mut writer: W, mut head: Vec<u8>, body: &[u8]) -> io::Result<()> {
    // for smaller bodies, it's faster to just copy + write_all
    if body.len() < INLINE_COPY_MAX {
        head.reserve(body.len());
        head.extend_from_slice(body);
        return writer.write_all(&head);
    }
    let iov = [IoSlice::new(&head), IoSlice::new(body)];
    let n = writer.write_vectored(&iov)?;
    if n < head.len() {
        writer.write_all(&head[n..])?;
        writer.write_all(body)
    } else {
        let offset = n - head.len();
        writer.write_all(&body[offset..])
    }
}

#[inline]
fn write_streaming<W: Write, R: Read>(writer: &mut W, mut body: R) -> io::Result<()> {
    std::io::copy(&mut body, writer).map(|_| ())
}

#[inline]
fn write_chunk<W: Write>(writer: &mut W, bytes: &[u8]) -> io::Result<()> {
    write!(writer, "{:X}\r\n", bytes.len())?;
    writer.write_all(bytes)?;
    writer.write_all(CRLF)
}

#[inline]
fn write_chunked<W: Write, R: Read>(mut writer: W, mut body: R) -> io::Result<()> {
    // TODO: fine-tune the buffer size
    let mut buf: [MaybeUninit<u8>; 128 * 1024] = unsafe { MaybeUninit::uninit().assume_init() };

    loop {
        let n = {
            let dst: &mut [u8] =
                unsafe { std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, buf.len()) };
            body.read(dst)?
        };

        if n == 0 {
            break;
        }

        // SAFETY: The first `n` bytes were just written by `read`, so they are initialized.
        let init_slice: &[u8] = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, n) };

        write_chunk(&mut writer, init_slice)?;
    }

    // terminating chunk
    writer.write_all(b"0\r\n\r\n")
}
