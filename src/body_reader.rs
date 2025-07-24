// src/body_reader.rs
use crate::common::HttpHeaders;
use std::io::{self, BufRead, BufReader, Read};

pub enum BodyReader<R: Read> {
    Fixed { inner: BufReader<R>, remaining: u64 },
    Chunked(ChunkedReader<BufReader<R>>),
    Eof(BufReader<R>),
    Empty(BufReader<R>),
}

impl<R: Read> Read for BodyReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            BodyReader::Empty(_) => Ok(0),
            BodyReader::Fixed { inner, remaining } => {
                if *remaining == 0 {
                    return Ok(0);
                }
                let to_read = (*remaining as usize).min(buf.len());
                let n = inner.read(&mut buf[..to_read])?;
                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "fixed body truncated",
                    ));
                }
                *remaining -= n as u64;
                Ok(n)
            }
            BodyReader::Eof(inner) => inner.read(buf),
            BodyReader::Chunked(r) => r.read(buf),
        }
    }
}

impl<R: Read> BodyReader<R> {
    pub fn from(headers: &HttpHeaders, reader: BufReader<R>) -> Self {
        if headers.is_transfer_encoding_chunked() {
            return BodyReader::Chunked(ChunkedReader::new(reader));
            // TODO: document: other transfer-encodings are ignored
        }
        if let Some(cl) = headers.get_content_length() {
            if cl == 0 {
                BodyReader::Empty(reader)
            } else {
                BodyReader::Fixed {
                    inner: reader,
                    remaining: cl,
                }
            }
        } else {
            BodyReader::Eof(reader)
        }
    }

    pub fn inner_mut(&mut self) -> &mut BufReader<R> {
        match self {
            BodyReader::Fixed { inner, .. } => inner,
            BodyReader::Chunked(c) => c.inner_mut(),
            BodyReader::Eof(br) => br,
            BodyReader::Empty(br) => br,
        }
    }

    pub fn inner(&self) -> &BufReader<R> {
        match self {
            BodyReader::Fixed { inner, .. } => inner,
            BodyReader::Chunked(c) => c.inner(),
            BodyReader::Eof(br) => br,
            BodyReader::Empty(br) => br,
        }
    }
}

pub struct ChunkedReader<R: BufRead> {
    inner: R,
    state: ChunkState,
    remaining_in_chunk: u64,
}

impl<R: BufRead> ChunkedReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            state: ChunkState::ReadSize,
            remaining_in_chunk: 0,
        }
    }

    fn read_chunk_size(&mut self) -> io::Result<()> {
        let mut line = String::new();
        let n = self.inner.read_line(&mut line)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "chunked size eof",
            ));
        }
        let line = line.trim_end_matches(['\r', '\n']);
        let hex = line.split(';').next().unwrap_or("");
        let size = u64::from_str_radix(hex, 16)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad chunk size"))?;
        self.remaining_in_chunk = size;
        self.state = if size == 0 {
            ChunkState::Done
        } else {
            ChunkState::ReadData
        };
        Ok(())
    }

    pub fn inner(&self) -> &R {
        &self.inner
    }
    pub fn inner_mut(&mut self) -> &mut R {
        &mut self.inner
    }
}

impl<R: BufRead> Read for ChunkedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.state {
                ChunkState::ReadSize => {
                    self.read_chunk_size()?;
                    if matches!(self.state, ChunkState::Done) {
                        // skip trailers
                        let mut line = String::new();
                        loop {
                            let n = self.inner.read_line(&mut line)?;
                            if n == 0 || line == "\r\n" || line == "\n" {
                                break;
                            }
                            line.clear();
                        }
                        return Ok(0);
                    }
                }
                ChunkState::ReadData => {
                    if self.remaining_in_chunk == 0 {
                        self.state = ChunkState::ReadCrlfAfterChunk;
                        continue;
                    }
                    let to_read = (self.remaining_in_chunk as usize).min(buf.len());
                    let n = self.inner.read(&mut buf[..to_read])?;
                    if n == 0 {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "chunk truncated",
                        ));
                    }
                    self.remaining_in_chunk -= n as u64;
                    return Ok(n);
                }
                ChunkState::ReadCrlfAfterChunk => {
                    let mut crlf = [0u8; 2];
                    self.inner.read_exact(&mut crlf)?;
                    if &crlf != b"\r\n" {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "missing CRLF after chunk",
                        ));
                    }
                    self.state = ChunkState::ReadSize;
                }
                ChunkState::Done => return Ok(0),
            }
        }
    }
}

enum ChunkState {
    ReadSize,
    ReadData,
    ReadCrlfAfterChunk,
    Done,
}
