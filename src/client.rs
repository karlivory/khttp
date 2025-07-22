// src/client.rs
use crate::common::{HttpBodyReader, HttpHeaders, HttpMethod, HttpStatus};
use crate::http_parser::{HttpParsingError, HttpResponseParser};
use crate::http_printer::HttpPrinter;
use std::error::Error;
use std::fmt::Display;
use std::io::{self, Read};
use std::net::TcpStream;

pub struct Client {
    address: String,
}

impl Client {
    pub fn new(address: &str) -> Client {
        Self {
            address: address.to_string(),
        }
    }
    pub fn get(&self, uri: &str, headers: &HttpHeaders) -> Result<HttpResponse, HttpClientError> {
        self.exchange(&HttpMethod::Get, uri, headers, &[][..])
    }

    pub fn post(
        &self,
        uri: &str,
        headers: &HttpHeaders,
        body: impl Read,
    ) -> Result<HttpResponse, HttpClientError> {
        self.exchange(&HttpMethod::Post, uri, headers, body)
    }

    pub fn exchange(
        &self,
        method: &HttpMethod,
        uri: &str,
        headers: &HttpHeaders,
        body: impl Read,
    ) -> Result<HttpResponse, HttpClientError> {
        // establish connection
        let mut stream = ClientRequestTcpStream::new(&self.address)?;

        stream.write(method, uri, headers, body)?;

        // read response
        let response = stream.read()?;

        Ok(response)
    }
}

struct ClientRequestTcpStream {
    stream: TcpStream,
}

// #[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    pub headers: HttpHeaders,
    pub status: HttpStatus,
    body: HttpBodyReader<TcpStream>,
}

impl HttpResponse {
    pub fn get_body_reader(&mut self) -> &mut HttpBodyReader<TcpStream> {
        &mut self.body
    }

    pub fn read_body(&mut self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.body.read_to_end(&mut buf).unwrap();
        self.close_connection();
        buf
    }

    pub fn read_body_to_string(&mut self) -> String {
        let mut buf = String::new();
        self.body.read_to_string(&mut buf).unwrap();
        self.close_connection();
        buf
    }

    pub fn close_connection(&mut self) {
        use std::net::Shutdown;
        let _ = self.body.reader.get_mut().shutdown(Shutdown::Both);
    }
}

impl Drop for HttpResponse {
    fn drop(&mut self) {
        self.close_connection();
    }
}

impl ClientRequestTcpStream {
    fn new(host: &str) -> Result<Self, HttpClientError> {
        let stream = TcpStream::connect(host);
        match stream {
            Ok(stream) => Ok(ClientRequestTcpStream { stream }),
            Err(e) => Err(HttpClientError::ConnectionFailure(e)),
        }
    }

    fn write(
        &mut self,
        method: &HttpMethod,
        uri: &str,
        headers: &HttpHeaders,
        body: impl Read,
    ) -> Result<(), HttpClientError> {
        HttpPrinter::new(&self.stream)
            .write_request(method, uri, headers, body)
            .map_err(HttpClientError::WriteFailure)?;
        Ok(())
    }

    fn read(self) -> Result<HttpResponse, HttpClientError> {
        let parts = HttpResponseParser::new(self.stream).parse()?;
        let content_len = parts.headers.get_content_length().unwrap_or(0);
        let response = HttpResponse {
            headers: parts.headers,
            status: parts.status,
            body: HttpBodyReader {
                reader: parts.reader,
                remaining: content_len as u64,
            },
        };
        Ok(response)
    }
}

impl From<HttpParsingError> for HttpClientError {
    fn from(e: HttpParsingError) -> Self {
        HttpClientError::ParsingFailure(e)
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum HttpClientError {
    ConnectionFailure(io::Error),
    WriteFailure(io::Error),
    ReadFailure(io::Error),
    ParsingFailure(HttpParsingError),
}

impl Error for HttpClientError {}

impl Display for HttpClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use HttpClientError::*;
        match self {
            ConnectionFailure(e) => write!(f, "Connection failure: {}", e),
            WriteFailure(e) => write!(f, "Failed to write to tcp socket: {}", e),
            ReadFailure(e) => write!(f, "Failed to read from tcp socket: {}", e),
            ParsingFailure(e) => write!(f, "Failed to parse http response: {}", e),
        }
    }
}
