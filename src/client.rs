use crate::{BodyReader, Headers, HttpParsingError, HttpPrinter, Method, Parser, Status};
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
    pub fn get(&self, uri: &str, headers: Headers) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Get, uri, headers, &[][..])
    }

    pub fn head(&self, uri: &str, headers: Headers) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Head, uri, headers, &[][..])
    }

    pub fn put(
        &self,
        uri: &str,
        headers: Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Put, uri, headers, body)
    }

    pub fn patch(
        &self,
        uri: &str,
        headers: Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Patch, uri, headers, body)
    }

    pub fn post(
        &self,
        uri: &str,
        headers: Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Post, uri, headers, body)
    }

    pub fn delete(
        &self,
        uri: &str,
        headers: Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Delete, uri, headers, body)
    }

    pub fn options(
        &self,
        uri: &str,
        headers: Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Options, uri, headers, body)
    }

    pub fn trace(
        &self,
        uri: &str,
        headers: Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Trace, uri, headers, body)
    }

    pub fn exchange(
        &self,
        method: &Method,
        uri: &str,
        headers: Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        // establish connection
        let mut stream = ClientRequestTcpStream::new(&self.address)?;

        // write request
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
pub struct ClientResponseHandle {
    pub headers: Headers,
    pub status: Status,
    body: BodyReader<TcpStream>,
}

impl ClientResponseHandle {
    pub fn get_body_reader(&mut self) -> &mut BodyReader<TcpStream> {
        &mut self.body
    }

    pub fn read_body(&mut self) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        self.body.read_to_end(&mut buf)?;
        self.close_connection().ok();
        Ok(buf)
    }

    pub fn read_body_to_string(&mut self) -> io::Result<String> {
        let mut buf = String::new();
        self.body.read_to_string(&mut buf)?;
        self.close_connection().ok();
        Ok(buf)
    }

    pub fn stream(&self) -> &TcpStream {
        self.body.inner().get_ref()
    }

    pub fn stream_mut(&mut self) -> &mut TcpStream {
        self.body.inner_mut().get_mut()
    }

    pub fn close_connection(&mut self) -> io::Result<()> {
        self.stream_mut().shutdown(std::net::Shutdown::Both)
    }
}

impl Drop for ClientResponseHandle {
    fn drop(&mut self) {
        self.close_connection().ok();
    }
}

impl ClientRequestTcpStream {
    fn new(host: &str) -> Result<Self, ClientError> {
        let stream = TcpStream::connect(host);
        match stream {
            Ok(stream) => Ok(ClientRequestTcpStream { stream }),
            Err(e) => Err(ClientError::ConnectionFailure(e)),
        }
    }

    fn write(
        &mut self,
        method: &Method,
        uri: &str,
        headers: Headers,
        body: impl Read,
    ) -> Result<(), ClientError> {
        HttpPrinter::new(&self.stream)
            .write_request(method, uri, headers, body)
            .map_err(ClientError::WriteFailure)?;
        Ok(())
    }

    fn read(self) -> Result<ClientResponseHandle, ClientError> {
        let parts = Parser::new(self.stream).parse_response(&None, &None, &None)?; // TODO
        let body = BodyReader::from(&parts.headers, parts.reader);
        let response = ClientResponseHandle {
            headers: parts.headers,
            status: parts.status,
            body,
        };
        Ok(response)
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    ConnectionFailure(io::Error),
    WriteFailure(io::Error),
    ReadFailure(io::Error),
    ParsingFailure(HttpParsingError),
}

impl From<HttpParsingError> for ClientError {
    fn from(e: HttpParsingError) -> Self {
        ClientError::ParsingFailure(e)
    }
}

impl Error for ClientError {}

impl Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use ClientError::*;
        match self {
            ConnectionFailure(e) => write!(f, "Connection failure: {}", e),
            WriteFailure(e) => write!(f, "Failed to write to tcp socket: {}", e),
            ReadFailure(e) => write!(f, "Failed to read from tcp socket: {}", e),
            ParsingFailure(e) => write!(f, "Failed to parse http response: {}", e),
        }
    }
}
