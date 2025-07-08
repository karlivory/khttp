// src/common.rs
use std::collections::HashMap;
use std::fmt;
use std::fmt::Display;

pub static HTTP_VERSION: &str = "HTTP/1.1";

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum HttpMethod {
    Get,
    Post,
    Head,
    Put,
    Patch,
    Delete,
    Options,
    Trace,
    Custom(String),
}

#[derive(Debug, PartialEq)]
pub struct HttpRequest {
    pub body: Option<Vec<u8>>,
    pub headers: HttpHeaders,
    pub method: HttpMethod,
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    pub body: Option<Vec<u8>>,
    pub headers: HttpHeaders,
    pub status: HttpStatus,
}

impl Default for HttpResponse {
    fn default() -> Self {
        Self {
            body: Default::default(),
            headers: Default::default(),
            status: HttpStatus::of(200),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpStatus {
    pub code: u16,
    pub reason: String,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct HttpHeaders {
    headers: HashMap<String, String>,
}

impl From<HashMap<&str, &str>> for HttpHeaders {
    fn from(value: HashMap<&str, &str>) -> Self {
        let mut headers = HttpHeaders::new();
        for (key, val) in value {
            headers.add_header(key, val);
        }
        headers
    }
}

impl From<HashMap<String, String>> for HttpHeaders {
    fn from(value: HashMap<String, String>) -> Self {
        let mut headers = HttpHeaders::new();
        for (key, val) in value {
            headers.add_header(&key, &val);
        }
        headers
    }
}

impl HttpHeaders {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get_header_map(&self) -> &HashMap<String, String> {
        &self.headers
    }

    pub fn add_header(&mut self, name: &str, value: &str) {
        self.headers
            .insert(name.to_lowercase(), value.to_lowercase());
    }

    pub fn remove_header(&mut self, name: &str) -> Option<String> {
        self.headers.remove(name)
    }

    /// headers
    pub fn get_content_length(&self) -> Option<usize> {
        let value = self.headers.get("content-length")?;
        let content_len = value.parse::<usize>();
        content_len.ok()
    }
    pub fn set_content_length(&mut self, len: usize) {
        self.headers
            .insert("content-length".into(), len.to_string());
    }
}

impl HttpResponse {
    pub fn ok(headers: HttpHeaders, body: Option<Vec<u8>>) -> HttpResponse {
        HttpResponse {
            headers,
            status: HttpStatus::of(200),
            body,
        }
    }
}

impl Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HttpMethod::Get => write!(f, "GET"),
            HttpMethod::Post => write!(f, "POST"),
            HttpMethod::Head => write!(f, "HEAD"),
            HttpMethod::Put => write!(f, "PUT"),
            HttpMethod::Patch => write!(f, "PATCH"),
            HttpMethod::Delete => write!(f, "DELETE"),
            HttpMethod::Options => write!(f, "OPTIONS"),
            HttpMethod::Trace => write!(f, "TRACE"),
            HttpMethod::Custom(str) => write!(f, "{}", str),
        }
    }
}

impl HttpStatus {
    pub fn new(code: u16, reason: String) -> Self {
        Self { code, reason }
    }
    pub fn of(code: u16) -> Self {
        Self {
            code,
            reason: get_status_code_reason(code).unwrap_or("").to_string(),
        }
    }
}

fn get_status_code_reason(code: u16) -> Option<&'static str> {
    Some(match code {
        // 1xx: Informational
        100 => "CONTINUE",
        101 => "SWITCHING PROTOCOLS",
        102 => "PROCESSING",
        103 => "EARLY HINTS",

        // 2xx: Success
        200 => "OK",
        201 => "CREATED",
        202 => "ACCEPTED",
        203 => "NON-AUTHORITATIVE INFORMATION",
        204 => "NO CONTENT",
        205 => "RESET CONTENT",
        206 => "PARTIAL CONTENT",
        207 => "MULTI-STATUS",
        208 => "ALREADY REPORTED",
        226 => "IM USED",

        // 3xx: Redirection
        300 => "MULTIPLE CHOICES",
        301 => "MOVED PERMANENTLY",
        302 => "FOUND",
        303 => "SEE OTHER",
        304 => "NOT MODIFIED",
        305 => "USE PROXY",
        307 => "TEMPORARY REDIRECT",
        308 => "PERMANENT REDIRECT",

        // 4xx: Client Error
        400 => "BAD REQUEST",
        401 => "UNAUTHORIZED",
        402 => "PAYMENT REQUIRED",
        403 => "FORBIDDEN",
        404 => "NOT FOUND",
        405 => "METHOD NOT ALLOWED",
        406 => "NOT ACCEPTABLE",
        407 => "PROXY AUTHENTICATION REQUIRED",
        408 => "REQUEST TIMEOUT",
        409 => "CONFLICT",
        410 => "GONE",
        411 => "LENGTH REQUIRED",
        412 => "PRECONDITION FAILED",
        413 => "PAYLOAD TOO LARGE",
        414 => "URI TOO LONG",
        415 => "UNSUPPORTED MEDIA TYPE",
        416 => "RANGE NOT SATISFIABLE",
        417 => "EXPECTATION FAILED",
        418 => "I'M A TEAPOT",
        421 => "MISDIRECTED REQUEST",
        422 => "UNPROCESSABLE ENTITY",
        423 => "LOCKED",
        424 => "FAILED DEPENDENCY",
        425 => "TOO EARLY",
        426 => "UPGRADE REQUIRED",
        428 => "PRECONDITION REQUIRED",
        429 => "TOO MANY REQUESTS",
        431 => "REQUEST HEADER FIELDS TOO LARGE",
        451 => "UNAVAILABLE FOR LEGAL REASONS",

        // 5xx: Server Error
        500 => "INTERNAL SERVER ERROR",
        501 => "NOT IMPLEMENTED",
        502 => "BAD GATEWAY",
        503 => "SERVICE UNAVAILABLE",
        504 => "GATEWAY TIMEOUT",
        505 => "HTTP VERSION NOT SUPPORTED",
        506 => "VARIANT ALSO NEGOTIATES",
        507 => "INSUFFICIENT STORAGE",
        508 => "LOOP DETECTED",
        510 => "NOT EXTENDED",
        511 => "NETWORK AUTHENTICATION REQUIRED",

        _ => return None,
    })
}
