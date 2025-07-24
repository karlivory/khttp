// src/common.rs

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Display;
use std::fmt::{self};
use std::str::FromStr;

// ---------------------------------------------------------------------
// HttpHeaders
// ---------------------------------------------------------------------

#[derive(Debug, Default, Clone, PartialEq)]
pub struct HttpHeaders {
    headers: HashMap<String, Vec<String>>,
}

impl From<HashMap<&str, &str>> for HttpHeaders {
    fn from(value: HashMap<&str, &str>) -> Self {
        let mut headers = HttpHeaders::new();
        for (key, val) in value {
            headers.add(key, val);
        }
        headers
    }
}

impl From<HashMap<String, String>> for HttpHeaders {
    fn from(value: HashMap<String, String>) -> Self {
        let mut headers = HttpHeaders::new();
        for (key, val) in value {
            headers.add(&key, &val);
        }
        headers
    }
}

impl From<Vec<(&str, &str)>> for HttpHeaders {
    fn from(value: Vec<(&str, &str)>) -> Self {
        let mut headers = HttpHeaders::new();
        for (key, val) in value {
            headers.add(key, val);
        }
        headers
    }
}

impl HttpHeaders {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get_map(&self) -> &HashMap<String, Vec<String>> {
        &self.headers
    }

    pub fn get_count(&self) -> usize {
        self.headers.len()
    }

    pub fn add(&mut self, name: &str, value: &str) {
        self.headers
            .entry(name.to_lowercase())
            .or_default()
            .push(value.to_string());
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_lowercase())?
            .last()
            .map(|s| s.as_str())
    }

    pub fn get_all<'a>(&'a self, name: &str) -> impl Iterator<Item = &'a str> {
        self.headers
            .get(&name.to_lowercase())
            .into_iter()
            .flat_map(|v| v.iter().map(|s| s.as_str()))
    }

    pub fn set(&mut self, name: &str, value: &str) {
        self.headers
            .insert(name.to_lowercase(), vec![value.to_string()]);
    }

    pub fn remove(&mut self, name: &str) -> Option<Vec<String>> {
        self.headers.remove(&name.to_lowercase())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.headers.contains_key(&name.to_lowercase())
    }

    pub const CONTENT_LENGTH: &str = "content-length";
    pub const CONTENT_TYPE: &str = "content-type";
    pub const TRANSFER_ENCODING: &str = "transfer-encoding";
    pub const CONNECTION: &str = "connection";

    pub fn get_content_length(&self) -> Option<u64> {
        let cl = self.get(Self::CONTENT_LENGTH)?;
        if let Ok(n) = cl.trim().parse::<u64>() {
            return Some(n);
        }
        None
    }

    pub fn set_content_length(&mut self, len: u64) {
        self.set(Self::CONTENT_LENGTH, &len.to_string());
    }

    pub fn set_transfer_encoding_chunked(&mut self) {
        self.set(Self::TRANSFER_ENCODING, TransferEncoding::CHUNKED);
    }

    pub fn is_transfer_encoding_chunked(&self) -> bool {
        match self.get(Self::TRANSFER_ENCODING) {
            Some(te) => te
                .split(',')
                .any(|t| t.trim().eq_ignore_ascii_case(TransferEncoding::CHUNKED)),
            None => false,
        }
    }
}

pub struct TransferEncoding {}

impl TransferEncoding {
    pub const CHUNKED: &str = "chunked";
}

impl std::fmt::Display for HttpHeaders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (k, vs) in &self.headers {
            for v in vs {
                writeln!(f, "{}: {}", k, v)?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------
// HttpMethod
// ---------------------------------------------------------------------

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

impl From<&str> for HttpMethod {
    fn from(value: &str) -> Self {
        // compare ignoring ASCII case without allocating
        if value.eq_ignore_ascii_case("GET") {
            HttpMethod::Get
        } else if value.eq_ignore_ascii_case("POST") {
            HttpMethod::Post
        } else if value.eq_ignore_ascii_case("HEAD") {
            HttpMethod::Head
        } else if value.eq_ignore_ascii_case("PUT") {
            HttpMethod::Put
        } else if value.eq_ignore_ascii_case("PATCH") {
            HttpMethod::Patch
        } else if value.eq_ignore_ascii_case("DELETE") {
            HttpMethod::Delete
        } else if value.eq_ignore_ascii_case("OPTIONS") {
            HttpMethod::Options
        } else if value.eq_ignore_ascii_case("TRACE") {
            HttpMethod::Trace
        } else {
            HttpMethod::Custom(value.to_string())
        }
    }
}

impl HttpMethod {
    pub fn as_str(&self) -> &str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Head => "HEAD",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Options => "OPTIONS",
            HttpMethod::Trace => "TRACE",
            HttpMethod::Custom(s) => s.as_str(),
        }
    }
}

impl Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for HttpMethod {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(HttpMethod::from(s))
    }
}

impl AsRef<str> for HttpMethod {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq<&str> for HttpMethod {
    fn eq(&self, other: &&str) -> bool {
        self.as_str().eq_ignore_ascii_case(other)
    }
}

impl PartialEq<String> for HttpMethod {
    fn eq(&self, other: &String) -> bool {
        self.as_str().eq_ignore_ascii_case(other)
    }
}

// ---------------------------------------------------------------------
// HttpStatus
// ---------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpStatus {
    pub code: u16,
    pub reason: Cow<'static, str>,
}

impl HttpStatus {
    pub const fn borrowed(code: u16, reason: &'static str) -> Self {
        Self {
            code,
            reason: Cow::Borrowed(reason),
        }
    }
    pub fn owned(code: u16, reason: String) -> Self {
        Self {
            code,
            reason: Cow::Owned(reason),
        }
    }
    pub fn with_reason<S: Into<String>>(mut self, s: S) -> Self {
        self.reason = Cow::Owned(s.into());
        self
    }
    pub fn set_reason<S: Into<String>>(&mut self, s: S) {
        self.reason = Cow::Owned(s.into());
    }
}

impl fmt::Display for HttpStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code, self.reason)
    }
}

impl From<u16> for HttpStatus {
    fn from(code: u16) -> Self {
        Self::of(code)
    }
}
impl PartialEq<u16> for HttpStatus {
    fn eq(&self, other: &u16) -> bool {
        self.code == *other
    }
}

macro_rules! define_statuses {
    ($( $code:literal => $ident:ident, $reason:expr );* $(;)?) => {
        impl HttpStatus {
            $(
                pub const $ident: HttpStatus = HttpStatus::borrowed($code, $reason);
            )*

            pub const fn of(code: u16) -> Self {
                match code {
                    $(
                        $code => HttpStatus::$ident,
                    )*
                    _ => HttpStatus::borrowed(code, ""),
                }
            }
        }
    };
}

define_statuses! {
    // 1xx
    100 => CONTINUE, "CONTINUE";
    101 => SWITCHING_PROTOCOLS, "SWITCHING PROTOCOLS";
    102 => PROCESSING, "PROCESSING";
    103 => EARLY_HINTS, "EARLY HINTS";

    // 2xx
    200 => OK, "OK";
    201 => CREATED, "CREATED";
    202 => ACCEPTED, "ACCEPTED";
    203 => NON_AUTHORITATIVE_INFORMATION, "NON-AUTHORITATIVE INFORMATION";
    204 => NO_CONTENT, "NO CONTENT";
    205 => RESET_CONTENT, "RESET CONTENT";
    206 => PARTIAL_CONTENT, "PARTIAL CONTENT";
    207 => MULTI_STATUS, "MULTI-STATUS";
    208 => ALREADY_REPORTED, "ALREADY REPORTED";
    226 => IM_USED, "IM USED";

    // 3xx
    300 => MULTIPLE_CHOICES, "MULTIPLE CHOICES";
    301 => MOVED_PERMANENTLY, "MOVED PERMANENTLY";
    302 => FOUND, "FOUND";
    303 => SEE_OTHER, "SEE OTHER";
    304 => NOT_MODIFIED, "NOT MODIFIED";
    305 => USE_PROXY, "USE PROXY";
    307 => TEMPORARY_REDIRECT, "TEMPORARY REDIRECT";
    308 => PERMANENT_REDIRECT, "PERMANENT REDIRECT";

    // 4xx
    400 => BAD_REQUEST, "BAD REQUEST";
    401 => UNAUTHORIZED, "UNAUTHORIZED";
    402 => PAYMENT_REQUIRED, "PAYMENT REQUIRED";
    403 => FORBIDDEN, "FORBIDDEN";
    404 => NOT_FOUND, "NOT FOUND";
    405 => METHOD_NOT_ALLOWED, "METHOD NOT ALLOWED";
    406 => NOT_ACCEPTABLE, "NOT ACCEPTABLE";
    407 => PROXY_AUTHENTICATION_REQUIRED, "PROXY AUTHENTICATION REQUIRED";
    408 => REQUEST_TIMEOUT, "REQUEST TIMEOUT";
    409 => CONFLICT, "CONFLICT";
    410 => GONE, "GONE";
    411 => LENGTH_REQUIRED, "LENGTH REQUIRED";
    412 => PRECONDITION_FAILED, "PRECONDITION FAILED";
    413 => PAYLOAD_TOO_LARGE, "PAYLOAD TOO LARGE";
    414 => URI_TOO_LONG, "URI TOO LONG";
    415 => UNSUPPORTED_MEDIA_TYPE, "UNSUPPORTED MEDIA TYPE";
    416 => RANGE_NOT_SATISFIABLE, "RANGE NOT SATISFIABLE";
    417 => EXPECTATION_FAILED, "EXPECTATION FAILED";
    418 => IM_A_TEAPOT, "I'M A TEAPOT";
    421 => MISDIRECTED_REQUEST, "MISDIRECTED REQUEST";
    422 => UNPROCESSABLE_ENTITY, "UNPROCESSABLE ENTITY";
    423 => LOCKED, "LOCKED";
    424 => FAILED_DEPENDENCY, "FAILED DEPENDENCY";
    425 => TOO_EARLY, "TOO EARLY";
    426 => UPGRADE_REQUIRED, "UPGRADE REQUIRED";
    428 => PRECONDITION_REQUIRED, "PRECONDITION REQUIRED";
    429 => TOO_MANY_REQUESTS, "TOO MANY REQUESTS";
    431 => REQUEST_HEADER_FIELDS_TOO_LARGE, "REQUEST HEADER FIELDS TOO LARGE";
    451 => UNAVAILABLE_FOR_LEGAL_REASONS, "UNAVAILABLE FOR LEGAL REASONS";

    // 5xx
    500 => INTERNAL_SERVER_ERROR, "INTERNAL SERVER ERROR";
    501 => NOT_IMPLEMENTED, "NOT IMPLEMENTED";
    502 => BAD_GATEWAY, "BAD GATEWAY";
    503 => SERVICE_UNAVAILABLE, "SERVICE UNAVAILABLE";
    504 => GATEWAY_TIMEOUT, "GATEWAY TIMEOUT";
    505 => HTTP_VERSION_NOT_SUPPORTED, "HTTP VERSION NOT SUPPORTED";
    506 => VARIANT_ALSO_NEGOTIATES, "VARIANT ALSO NEGOTIATES";
    507 => INSUFFICIENT_STORAGE, "INSUFFICIENT STORAGE";
    508 => LOOP_DETECTED, "LOOP DETECTED";
    510 => NOT_EXTENDED, "NOT EXTENDED";
    511 => NETWORK_AUTHENTICATION_REQUIRED, "NETWORK AUTHENTICATION REQUIRED";
}
