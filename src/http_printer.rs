// src/http_printer.rs
//
// responsibility: writing HttpResponse or HttpRequest to T: std::io::Write

use std::io::{self, BufWriter, Read, Write, copy};

use crate::common::{HttpHeaders, HttpMethod, HttpStatus};

static CARRIAGE_BREAK: &[u8] = "\r\n".as_bytes();

pub struct HttpPrinter<W: Write> {
    writer: BufWriter<W>,
}

impl<W: Write> HttpPrinter<W> {
    pub fn new(stream: W) -> Self {
        Self {
            writer: BufWriter::new(stream),
        }
    }

    pub fn write_response(
        &mut self,
        status: &HttpStatus,
        headers: &HttpHeaders,
        mut body: impl Read,
    ) -> io::Result<()> {
        // status line
        write!(
            &mut self.writer,
            "{} {} {}",
            crate::common::HTTP_VERSION,
            status.code,
            status.reason
        )?;
        self.writer.write_all(CARRIAGE_BREAK)?;

        // headers
        self.write_headers(headers)?;
        self.writer.write_all(CARRIAGE_BREAK)?;

        // body
        copy(&mut body, &mut self.writer)?;

        Ok(())
    }

    pub fn write_request(
        &mut self,
        method: &HttpMethod,
        uri: &str,
        headers: &HttpHeaders,
        mut body: impl Read,
    ) -> io::Result<()> {
        // status line
        write!(
            &mut self.writer,
            "{} {} {}",
            method,
            uri,
            crate::common::HTTP_VERSION
        )?;
        self.writer.write_all(CARRIAGE_BREAK)?;

        // headers
        self.write_headers(headers)?;
        self.writer.write_all(CARRIAGE_BREAK)?;

        // body
        copy(&mut body, &mut self.writer)?;

        Ok(())
    }

    fn write_headers(&mut self, headers: &HttpHeaders) -> io::Result<()> {
        for (header, value) in headers.get_header_map() {
            write!(&mut self.writer, "{}: {}\r\n", header, value)?;
        }
        Ok(())
    }
}
