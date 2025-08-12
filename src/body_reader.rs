use crate::Headers;
use std::cmp::min;
use std::io::{self, BufRead, BufReader, ErrorKind, Read};

const BUF_SIZE: usize = 4096;

pub struct BodyReader<'a, R: Read>(BodyEncoding<'a, R>);

enum BodyEncoding<'a, R> {
    Fixed(FixedReader<'a, R>),
    Chunked(ChunkedReader<'a, R>),
    Eof(BufReader<StreamWithLeftover<'a, R>>),
    Empty(R),
}

impl<'a, R: Read> BodyReader<'a, R> {
    pub fn from_request(leftover: &'a [u8], stream: R, headers: &Headers) -> Self {
        if let Some(content_len) = headers.get_content_length() {
            if content_len > 0 {
                Self::new_fixed(leftover, stream, content_len as usize)
            } else {
                Self::new_empty(stream)
            }
        } else if headers.is_transfer_encoding_chunked() {
            Self::new_chunked(leftover, stream)
        } else {
            Self::new_empty(stream)
        }
    }

    pub fn from_response(leftover: &'a [u8], stream: R, headers: &Headers) -> Self {
        if let Some(content_len) = headers.get_content_length() {
            if content_len > 0 {
                Self::new_fixed(leftover, stream, content_len as usize)
            } else {
                Self::new_empty(stream)
            }
        } else if headers.is_transfer_encoding_chunked() {
            Self::new_chunked(leftover, stream)
        } else {
            Self::new_eof(leftover, stream)
        }
    }

    #[inline]
    pub fn new_fixed(leftover: &'a [u8], stream: R, content_length: usize) -> Self {
        Self(BodyEncoding::Fixed(FixedReader::new(
            leftover,
            stream,
            content_length,
        )))
    }

    #[inline]
    pub fn new_chunked(leftover: &'a [u8], stream: R) -> Self {
        Self(BodyEncoding::Chunked(ChunkedReader::new(leftover, stream)))
    }

    #[inline]
    pub fn new_eof(leftover: &'a [u8], stream: R) -> Self {
        Self(BodyEncoding::Eof(BufReader::with_capacity(
            BUF_SIZE,
            StreamWithLeftover::new(leftover, stream),
        )))
    }

    #[inline]
    pub fn new_empty(stream: R) -> Self {
        Self(BodyEncoding::Empty(stream))
    }

    pub fn string(&mut self) -> io::Result<String> {
        let mut buf = String::new();
        self.read_to_string(&mut buf).map(|_| buf)
    }

    pub fn vec(&mut self) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.read_to_end(&mut buf).map(|_| buf)
    }

    pub fn inner_mut(&mut self) -> &mut R {
        match &mut self.0 {
            BodyEncoding::Fixed(FixedReader { inner, .. }) => inner.get_mut().inner_mut(),
            BodyEncoding::Chunked(ChunkedReader { inner, .. }) => inner.get_mut().inner_mut(),
            BodyEncoding::Eof(reader) => reader.get_mut().inner_mut(),
            BodyEncoding::Empty(s) => s,
        }
    }

    pub fn inner(&self) -> &R {
        match &self.0 {
            BodyEncoding::Fixed(FixedReader { inner, .. }) => inner.get_ref().inner(),
            BodyEncoding::Chunked(ChunkedReader { inner, .. }) => inner.get_ref().inner(),
            BodyEncoding::Eof(reader) => reader.get_ref().inner(),
            BodyEncoding::Empty(s) => s,
        }
    }

    pub fn drain(&mut self) {
        let mut buf = [0u8; 1024];
        loop {
            match &mut self.0 {
                BodyEncoding::Fixed(reader) => {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(_) => continue,
                        Err(_) => break, // silently stop draining
                    }
                }
                BodyEncoding::Chunked(reader) => match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(_) => continue,
                    Err(_) => break,
                },
                BodyEncoding::Eof(_) => return,
                BodyEncoding::Empty(_) => return,
            }
        }
    }
}

impl<R: Read> Read for BodyReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.0 {
            BodyEncoding::Fixed(r) => r.read(buf),
            BodyEncoding::Chunked(c) => c.read(buf),
            BodyEncoding::Eof(r) => r.read(buf),
            BodyEncoding::Empty(_) => Ok(0),
        }
    }
}

impl<R: Read> BufRead for BodyReader<'_, R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        match &mut self.0 {
            BodyEncoding::Fixed(r) => r.fill_buf(),
            BodyEncoding::Chunked(c) => c.fill_buf(),
            BodyEncoding::Eof(r) => r.fill_buf(),
            BodyEncoding::Empty(_) => Ok(&[]),
        }
    }
    fn consume(&mut self, amt: usize) {
        match &mut self.0 {
            BodyEncoding::Fixed(r) => r.consume(amt),
            BodyEncoding::Chunked(c) => c.consume(amt),
            BodyEncoding::Eof(r) => r.consume(amt),
            BodyEncoding::Empty(_) => {}
        }
    }
}

struct StreamWithLeftover<'a, R> {
    leftover: &'a [u8],
    offset: usize,
    stream: R,
}

impl<'a, R> StreamWithLeftover<'a, R> {
    fn new(leftover: &'a [u8], stream: R) -> Self {
        Self {
            leftover,
            offset: 0,
            stream,
        }
    }

    pub fn inner_mut(&mut self) -> &mut R {
        &mut self.stream
    }

    pub fn inner(&self) -> &R {
        &self.stream
    }
}

impl<R: Read> Read for StreamWithLeftover<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.offset < self.leftover.len() {
            let avail = &self.leftover[self.offset..];
            let to_copy = min(avail.len(), buf.len());
            buf[..to_copy].copy_from_slice(&avail[..to_copy]);
            self.offset += to_copy;
            return Ok(to_copy);
        }
        self.stream.read(buf)
    }
}

// ---------------------------------------------------------------------
// content-length: x
// ---------------------------------------------------------------------

struct FixedReader<'a, R> {
    inner: BufReader<StreamWithLeftover<'a, R>>,
    remaining: usize,
}

impl<'a, R: Read> FixedReader<'a, R> {
    fn new(leftover: &'a [u8], stream: R, len: usize) -> Self {
        Self {
            inner: BufReader::with_capacity(BUF_SIZE, StreamWithLeftover::new(leftover, stream)),
            remaining: len,
        }
    }
}

impl<R: Read> Read for FixedReader<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let to_read = min(self.remaining, buf.len());
        let n = self.inner.read(&mut buf[..to_read])?;
        if n == 0 {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                "fixed body truncated",
            ));
        }
        self.remaining -= n;
        Ok(n)
    }
}

impl<R: Read> BufRead for FixedReader<'_, R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.remaining == 0 {
            return Ok(&[]);
        }
        let buf = self.inner.fill_buf()?;
        let len = min(buf.len(), self.remaining);
        Ok(&buf[..len])
    }

    fn consume(&mut self, amt: usize) {
        debug_assert!(amt <= self.remaining, "no more remaining");
        self.inner.consume(amt);
        self.remaining -= amt;
    }
}

// ---------------------------------------------------------------------
// transfer‑encoding: chunked
// ---------------------------------------------------------------------

struct ChunkedReader<'a, R> {
    inner: BufReader<StreamWithLeftover<'a, R>>,
    state: ChunkState,
    remaining_in_chunk: usize,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum ChunkState {
    Size,
    Data,
    Crlf,
    Trailer,
    Done,
}

impl<'a, R: Read> ChunkedReader<'a, R> {
    fn new(leftover: &'a [u8], stream: R) -> Self {
        Self {
            inner: BufReader::with_capacity(BUF_SIZE, StreamWithLeftover::new(leftover, stream)),
            state: ChunkState::Size,
            remaining_in_chunk: 0,
        }
    }

    fn read_chunk_size(&mut self) -> io::Result<()> {
        let mut line = String::new();
        if self.inner.read_line(&mut line)? == 0 {
            return Err(io::Error::new(ErrorKind::UnexpectedEof, "chunk size eof"));
        }
        let hex = line
            .split(';')
            .next()
            .unwrap_or("")
            .trim_end_matches(['\r', '\n']);
        self.remaining_in_chunk = usize::from_str_radix(hex, 16)
            .map_err(|_| io::Error::new(ErrorKind::InvalidData, "invalid chunk size"))?;
        self.state = if self.remaining_in_chunk == 0 {
            ChunkState::Trailer
        } else {
            ChunkState::Data
        };
        Ok(())
    }
}

impl<R: Read> Read for ChunkedReader<'_, R> {
    fn read(&mut self, mut out: &mut [u8]) -> io::Result<usize> {
        let mut written = 0;
        loop {
            match self.state {
                ChunkState::Size => {
                    self.read_chunk_size()?;
                    continue;
                }
                ChunkState::Data => {
                    if self.remaining_in_chunk == 0 {
                        self.state = ChunkState::Crlf;
                        continue;
                    }
                    if out.is_empty() {
                        break;
                    }
                    let to_read = min(self.remaining_in_chunk, out.len());
                    let n = self.inner.read(&mut out[..to_read])?;
                    if n == 0 {
                        return Err(io::Error::new(ErrorKind::UnexpectedEof, "chunk truncated"));
                    }
                    self.remaining_in_chunk -= n;
                    written += n;
                    out = &mut out[n..];
                    if self.remaining_in_chunk == 0 || out.is_empty() {
                        break;
                    }
                }
                ChunkState::Crlf => {
                    let mut crlf = [0u8; 2];
                    self.inner.read_exact(&mut crlf)?;
                    if &crlf != b"\r\n" {
                        return Err(io::Error::new(
                            ErrorKind::InvalidData,
                            "missing CRLF after chunk",
                        ));
                    }
                    self.state = ChunkState::Size;
                }
                ChunkState::Trailer => {
                    let mut line = String::new();
                    loop {
                        let n = self.inner.read_line(&mut line)?;
                        if n == 0 || line == "\r\n" || line == "\n" {
                            break;
                        }
                        line.clear();
                    }
                    self.state = ChunkState::Done;
                }
                ChunkState::Done => break,
            }
        }
        if written == 0 && matches!(self.state, ChunkState::Done) {
            Ok(0)
        } else {
            Ok(written)
        }
    }
}

impl<R: Read> BufRead for ChunkedReader<'_, R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        self.inner.consume(amt)
    }
}

// ---------------------------------------------------------------------
// Drain body on drop so the connection can be re‑used
// ---------------------------------------------------------------------

impl<R: Read> Drop for BodyReader<'_, R> {
    fn drop(&mut self) {
        self.drain();
    }
}
