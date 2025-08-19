use crate::{BodyReader, Headers, HttpParsingError, HttpPrinter, Method, Response, Status};
use std::error::Error;
use std::fmt::Display;
use std::io::{self, Read};
use std::mem::MaybeUninit;
use std::net::{TcpStream, ToSocketAddrs};

static MAX_RESPONSE_HEAD: usize = 8196;

#[derive(Clone)]
pub struct Client<A> {
    address: A,
    req_buf: MaybeUninit<[u8; MAX_RESPONSE_HEAD]>,
}

impl<A> Client<A>
where
    A: ToSocketAddrs,
{
    pub fn new(address: A) -> Client<A> {
        Self {
            address,
            req_buf: MaybeUninit::uninit(),
        }
    }

    pub fn exchange(
        &mut self,
        method: &Method,
        uri: &str,
        headers: &Headers,
        body: impl Read,
    ) -> Result<ClientResponseHandle<'_>, ClientError> {
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

pub struct ClientResponseHandle<'r> {
    pub headers: Headers<'r>,
    pub status: Status<'r>,
    body: BodyReader<'r, TcpStream>,
}

impl<'r> ClientResponseHandle<'r> {
    pub fn body(&mut self) -> &mut BodyReader<'r, TcpStream> {
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

    pub fn into_parts(self) -> (Status<'r>, Headers<'r>, BodyReader<'r, TcpStream>) {
        (self.status, self.headers, self.body)
    }
}

impl ClientRequestTcpStream {
    fn new<A>(host: A) -> Result<Self, ClientError>
    where
        A: ToSocketAddrs,
    {
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
        HttpPrinter::write_request(&mut self.stream, method, uri, headers, body)
            .map_err(ClientError::WriteFailure)?;
        Ok(())
    }

    fn read(
        mut self,
        buf: &mut MaybeUninit<[u8; MAX_RESPONSE_HEAD]>,
    ) -> Result<ClientResponseHandle<'_>, ClientError> {
        let buf_ptr = buf.as_mut_ptr() as *mut u8;

        // safety: we're gonna read n<=MAX_RESPONSE_HEAD bytes, and only use those
        let buf = unsafe { std::slice::from_raw_parts_mut(buf_ptr, MAX_RESPONSE_HEAD) };

        let n = match self.stream.read(buf) {
            Ok(0) => return Err(ClientError::UnexpectedEof),
            Ok(n) => n,
            Err(e) => return Err(ClientError::ReadFailure(e)),
        };
        let res = match Response::parse(&buf[..n]) {
            Ok(o) => o,
            Err(e) => return Err(ClientError::ParsingFailure(e)),
        };
        let body = BodyReader::from_response(&buf[res.buf_offset..n], self.stream, &res.headers);

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

macro_rules! define_method {
    ($name:ident, $method:ident, no_body) => {
        impl<A> Client<A>
        where
            A: ToSocketAddrs,
        {
            pub fn $name(
                &mut self,
                uri: &str,
                headers: &Headers,
            ) -> Result<ClientResponseHandle<'_>, ClientError> {
                self.exchange(&Method::$method, uri, headers, std::io::empty())
            }
        }
    };
    ($name:ident, $method:ident, body) => {
        impl<A> Client<A>
        where
            A: ToSocketAddrs,
        {
            pub fn $name(
                &mut self,
                uri: &str,
                headers: &Headers,
                body: impl Read,
            ) -> Result<ClientResponseHandle<'_>, ClientError> {
                self.exchange(&Method::$method, uri, headers, body)
            }
        }
    };
}
define_method!(get, Get, no_body);
define_method!(head, Head, no_body);
define_method!(post, Post, body);
define_method!(put, Put, body);
define_method!(patch, Patch, body);
define_method!(delete, Delete, body);
define_method!(options, Options, body);
define_method!(trace, Trace, body);
