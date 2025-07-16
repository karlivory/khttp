// src/client.rs
use crate::common::{HttpBodyReader, HttpHeaders, HttpMethod, HttpRequest, HttpStatus};
use crate::http_parser::{HttpParsingError, HttpResponseParser};
use crate::http_printer::HttpPrinter;
use std::io::{self, Read};
use std::net::TcpStream;

pub struct Client {
    address: String,
    headers: HttpHeaders,
}

impl Client {
    pub fn new(address: &str) -> Client {
        Self {
            address: address.to_string(),
            headers: HttpHeaders::new(),
        }
    }
    pub fn get(&self, uri: String, headers: HttpHeaders) -> Result<HttpResponse, HttpClientError> {
        let request = HttpRequest {
            method: HttpMethod::Get,
            uri,
            body: None,
            headers: self.populate_base_headers(headers),
        };
        self.exchange(request)
    }

    pub fn get_headers(&mut self) -> &HttpHeaders {
        &self.headers
    }

    pub fn get_headers_mut(&mut self) -> &mut HttpHeaders {
        &mut self.headers
    }

    pub fn post(
        &self,
        uri: String,
        headers: HttpHeaders,
        body: Option<Vec<u8>>,
    ) -> Result<HttpResponse, HttpClientError> {
        let request = HttpRequest {
            method: HttpMethod::Post,
            uri,
            body,
            headers: self.populate_base_headers(headers),
        };
        self.exchange(request)
    }

    pub fn exchange(&self, mut request: HttpRequest) -> Result<HttpResponse, HttpClientError> {
        // establish connection
        let mut stream = ClientRequestTcpStream::new(&self.address)?;

        // request middleware
        if let Some(ref body) = request.body {
            request.headers.set_content_length(body.len());
        }
        stream.write(&request)?;

        // read response
        let response = stream.read()?;

        Ok(response)
    }

    fn populate_base_headers(&self, mut headers: HttpHeaders) -> HttpHeaders {
        headers.add_header("host", &self.address);
        headers.add_header("connection", "close");
        headers.add_header("user-agent", "khttp/0.1");
        headers
    }
}

struct ClientRequestTcpStream {
    stream: TcpStream,
}

// #[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    pub headers: HttpHeaders,
    pub status: HttpStatus,
    body: HttpBodyReader,
}

impl HttpResponse {
    pub fn get_body_reader(&mut self) -> &mut HttpBodyReader {
        &mut self.body
    }

    pub fn read_body(&mut self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.body.read_to_end(&mut buf).unwrap();
        buf
    }

    pub fn read_body_to_string(&mut self) -> String {
        let mut buf = String::new();
        self.body.read_to_string(&mut buf).unwrap();
        buf
    }
}

impl HttpResponse {}

impl ClientRequestTcpStream {
    fn new(host: &str) -> Result<Self, HttpClientError> {
        let stream = TcpStream::connect(host);
        match stream {
            Ok(stream) => Ok(ClientRequestTcpStream { stream }),
            Err(e) => Err(HttpClientError::ConnectionFailure(e)),
        }
    }

    fn write(&mut self, request: &HttpRequest) -> Result<(), HttpClientError> {
        HttpPrinter::new(&self.stream)
            .write_request(request)
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
    fn from(_: HttpParsingError) -> Self {
        HttpClientError::ParsingFailure
    }
}

#[derive(Debug)]
pub enum HttpClientError {
    ConnectionFailure(io::Error),
    WriteFailure(io::Error),
    ReadFailure(io::Error),
    ParsingFailure,
}
