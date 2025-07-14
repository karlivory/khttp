// src/http_printer.rs
//
// responsibility: writing HttpResponse or HttpRequest to T: std::io::Write

use std::io::{self, BufWriter, Write};

use crate::common::{HttpHeaders, HttpRequest, HttpResponse};

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

    pub fn write_response(&mut self, response: &HttpResponse) -> io::Result<()> {
        // status line
        write!(
            &mut self.writer,
            "{} {} {}",
            crate::common::HTTP_VERSION,
            response.status.code,
            response.status.reason
        )?;
        self.writer.write_all(CARRIAGE_BREAK)?;

        // headers
        self.write_headers(&response.headers)?;
        self.writer.write_all(CARRIAGE_BREAK)?;

        // body
        if let Some(body) = &response.body {
            self.writer.write_all(body)?;
        }
        Ok(())
    }

    pub fn write_request(&mut self, request: &HttpRequest) -> io::Result<()> {
        // status line
        write!(
            &mut self.writer,
            "{} {} {}",
            request.method,
            request.uri,
            crate::common::HTTP_VERSION
        )?;
        self.writer.write_all(CARRIAGE_BREAK)?;

        // headers
        self.write_headers(&request.headers)?;
        self.writer.write_all(CARRIAGE_BREAK)?;

        // body
        if let Some(body) = &request.body {
            self.writer.write_all(body)?;
        }
        Ok(())
    }

    fn write_headers(&mut self, headers: &HttpHeaders) -> io::Result<()> {
        for (header, value) in headers.get_header_map() {
            write!(&mut self.writer, "{}: {}\r\n", header, value)?;
        }
        Ok(())
    }
}
