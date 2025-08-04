use crate::{BodyReader, Headers, HttpParsingError, HttpPrinter, Method, Response, Status};
use std::error::Error;
use std::fmt::Display;
use std::io::{self, Read};
use std::net::TcpStream;

pub struct Client {
    address: String,
    req_buf: [u8; 8196],
}

impl Client {
    pub fn new(address: &str) -> Client {
        Self {
            address: address.to_string(),
            req_buf: [0; 8196],
        }
    }
    pub fn get(
        &mut self,
        uri: &str,
        headers: &Headers,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Get, uri, headers, &[][..])
    }

    pub fn head(
        &mut self,
        uri: &str,
        headers: &Headers,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Head, uri, headers, &[][..])
    }

    pub fn put(
        &mut self,
        uri: &str,
        headers: &Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Put, uri, headers, body)
    }

    pub fn patch(
        &mut self,
        uri: &str,
        headers: &Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Patch, uri, headers, body)
    }

    pub fn post(
        &mut self,
        uri: &str,
        headers: &Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Post, uri, headers, body)
    }

    pub fn delete(
        &mut self,
        uri: &str,
        headers: &Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Delete, uri, headers, body)
    }

    pub fn options(
        &mut self,
        uri: &str,
        headers: &Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Options, uri, headers, body)
    }

    pub fn trace(
        &mut self,
        uri: &str,
        headers: &Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        self.exchange(&Method::Trace, uri, headers, body)
    }

    pub fn exchange(
        &mut self,
        method: &Method,
        uri: &str,
        headers: &Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle, ClientError> {
        // establish connection
        let mut stream = ClientRequestTcpStream::new(&self.address)?;

        // write request
        stream.write(method, uri, headers, body)?;

        // read response
        let response = stream.read(&mut self.req_buf)?;

        Ok(response)
    }
}

struct ClientRequestTcpStream {
    stream: TcpStream,
}

// #[derive(Debug, Clone, PartialEq)]
pub struct ClientResponseHandle<'a> {
    pub headers: Headers<'a>,
    pub status: Status,
    body: BodyReader<'a, TcpStream>,
}

impl<'a> ClientResponseHandle<'a> {
    pub fn body(&mut self) -> &mut BodyReader<'a, TcpStream> {
        &mut self.body
    }

    pub fn stream(&self) -> &TcpStream {
        self.body.inner()
    }

    pub fn stream_mut(&mut self) -> &mut TcpStream {
        self.body.inner_mut()
    }

    pub fn close_connection(&mut self) -> io::Result<()> {
        self.stream_mut().shutdown(std::net::Shutdown::Both)
    }
}

impl Drop for ClientResponseHandle<'_> {
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
        headers: &Headers,
        body: impl Read,
    ) -> Result<(), ClientError> {
        HttpPrinter::new(&self.stream)
            .write_request(method, uri, headers, body)
            .map_err(ClientError::WriteFailure)?;
        Ok(())
    }

    fn read(mut self, buf: &mut [u8]) -> Result<ClientResponseHandle<'_>, ClientError> {
        if let Ok(0) = self.stream.read(buf) {
            return Err(ClientError::UnexpectedEof);
        }
        let res = match Response::parse(buf) {
            Ok(o) => o,
            Err(e) => return Err(ClientError::ParsingFailure(e)),
        };
        let body = BodyReader::from_request(&buf[res.buf_offset..], self.stream, &res.headers);

        Ok(ClientResponseHandle {
            headers: res.headers,
            status: res.status,
            body,
        })
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum ClientError {
    ConnectionFailure(io::Error),
    WriteFailure(io::Error),
    ReadFailure(io::Error),
    UnexpectedEof,
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
            UnexpectedEof => write!(f, "unexpected eof"),
        }
    }
}
