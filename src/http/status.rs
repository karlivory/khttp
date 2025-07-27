use std::{borrow::Cow, fmt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub code: u16,
    pub reason: Cow<'static, str>,
}

impl Status {
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

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code, self.reason)
    }
}

impl From<u16> for Status {
    fn from(code: u16) -> Self {
        Self::of(code)
    }
}
impl PartialEq<u16> for Status {
    fn eq(&self, other: &u16) -> bool {
        self.code == *other
    }
}

macro_rules! define_statuses {
    ($( $code:literal => $ident:ident, $reason:expr );* $(;)?) => {
        impl Status {
            $(
                pub const $ident: Status = Status::borrowed($code, $reason);
            )*

            pub const fn of(code: u16) -> Self {
                match code {
                    $(
                        $code => Status::$ident,
                    )*
                    _ => Status::borrowed(code, ""),
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
