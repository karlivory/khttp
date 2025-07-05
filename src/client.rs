// src/client.rs
use crate::common::{HttpHeaders, HttpMethod, HttpRequest, HttpResponse};
use crate::http_parser::HttpParser;
use crate::http_printer::HttpPrinter;
use std::io::{self};
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

impl ClientRequestTcpStream {
    fn new(host: &str) -> Result<Self, HttpClientError> {
        let stream = TcpStream::connect(host);
        match stream {
            Ok(s) => Ok(ClientRequestTcpStream { stream: s }),
            Err(e) => Err(HttpClientError::ConnectionFailure(e)),
        }
    }

    fn write(&mut self, request: &HttpRequest) -> Result<(), HttpClientError> {
        HttpPrinter::new(&self.stream)
            .write_request(request)
            .map_err(HttpClientError::WriteFailure)?;
        Ok(())
    }

    fn read(&mut self) -> Result<HttpResponse, HttpClientError> {
        HttpParser::new(&self.stream)
            .parse_response()
            .map_err(|_| HttpClientError::ParsingFailure)
    }
}

#[derive(Debug)]
pub enum HttpClientError {
    ConnectionFailure(io::Error),
    WriteFailure(io::Error),
    ReadFailure(io::Error),
    ParsingFailure,
}
