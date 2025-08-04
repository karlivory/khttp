use std::borrow::Cow;
use std::fmt;
use std::sync::LazyLock;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Headers<'a> {
    headers: Vec<(Cow<'a, str>, Cow<'a, [u8]>)>,
    content_length: Option<u64>,
    chunked: bool,
    transfer_encoding: Vec<u8>,
    connection_close: bool,
    connection_values: Vec<u8>,
}

static HEADERS_VEC_INIT_CAPACITY: usize = 16; // rough guess, could be benchmarked
pub static EMPTY_HEADERS: LazyLock<Headers<'static>> = LazyLock::new(Headers::new);

impl<'a> Headers<'a> {
    pub fn empty() -> &'static Headers<'static> {
        &EMPTY_HEADERS
    }

    pub fn new() -> Self {
        Self {
            headers: Vec::with_capacity(HEADERS_VEC_INIT_CAPACITY),
            content_length: None,
            transfer_encoding: Vec::new(),
            chunked: false,
            connection_close: false,
            connection_values: Vec::new(),
        }
    }

    pub fn get_all(&self) -> &[(Cow<'a, str>, Cow<'a, [u8]>)] {
        &self.headers
    }

    pub fn get_count(&self) -> usize {
        self.headers.len()
    }

    pub fn add<N, V>(&mut self, name: N, value: V)
    where
        N: Into<Cow<'a, str>>,
        V: Into<Cow<'a, [u8]>>,
    {
        let name = name.into();
        let value = value.into();

        // TODO: only trim OWS (SP/HTAB), trim_ascii* is too permissive
        if name.eq_ignore_ascii_case(Self::CONTENT_LENGTH) {
            if let Ok(s) = std::str::from_utf8(&value) {
                self.content_length = s.trim_ascii().parse().ok();
            }
            return;
        } else if name.eq_ignore_ascii_case(Self::TRANSFER_ENCODING) {
            value
                .split(|&b| b == b',')
                .map(|v| v.trim_ascii_start())
                .for_each(|v| {
                    if v.eq_ignore_ascii_case(b"chunked") {
                        self.set_transfer_encoding_chunked();
                    } else {
                        if !self.transfer_encoding.is_empty() {
                            self.transfer_encoding.extend_from_slice(b", ");
                        }
                        self.transfer_encoding.extend_from_slice(v);
                    }
                });
            return;
        } else if name.eq_ignore_ascii_case(Self::CONNECTION) {
            value
                .split(|&b| b == b',')
                .map(|v| v.trim_ascii_start())
                .for_each(|v| {
                    if v.eq_ignore_ascii_case(b"close") {
                        self.set_connection_close();
                    } else {
                        if !self.connection_values.is_empty() {
                            self.connection_values.extend_from_slice(b", ");
                        }
                        self.connection_values.extend_from_slice(v);
                    }
                });
            return;
        }

        self.headers.push((name, value));
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.headers
            .iter()
            .rev()
            .find(|(k, _)| k.as_ref().eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_ref())
    }

    pub fn set<N, V>(&mut self, name: N, value: V)
    where
        N: Into<Cow<'a, str>>,
        V: Into<Cow<'a, [u8]>>,
    {
        let name = name.into();
        let value = value.into();

        self.headers.retain(|(k, _)| !k.eq_ignore_ascii_case(&name));

        if name.eq_ignore_ascii_case(Self::CONTENT_LENGTH) {
            if let Ok(s) = std::str::from_utf8(&value) {
                self.content_length = s.trim().parse().ok();
            }
        }

        self.headers.push((name, value));
    }

    pub fn remove(&mut self, name: &str) -> Vec<Cow<'a, [u8]>> {
        if name.eq_ignore_ascii_case(Self::CONTENT_LENGTH) {
            self.content_length = None;
        }

        let mut removed = Vec::new();
        self.headers.retain(|(k, v)| {
            if k.eq_ignore_ascii_case(name) {
                removed.push(v.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    pub const CONTENT_LENGTH: &'static str = "content-length";
    pub const CONTENT_TYPE: &'static str = "content-type";
    pub const TRANSFER_ENCODING: &'static str = "transfer-encoding";
    pub const CONNECTION: &'static str = "connection";

    pub fn get_content_length(&self) -> Option<u64> {
        self.content_length
    }

    pub fn set_content_length(&mut self, len: Option<u64>) {
        self.content_length = len;
    }

    pub fn set_transfer_encoding_chunked(&mut self) {
        self.chunked = true;

        if self.transfer_encoding.is_empty() {
            self.transfer_encoding.extend_from_slice(b"chunked");
        } else {
            self.transfer_encoding.extend_from_slice(b", chunked");
        }
    }

    pub fn is_transfer_encoding_chunked(&self) -> bool {
        self.chunked
    }

    pub fn get_transfer_encoding(&self) -> &[u8] {
        &self.transfer_encoding
    }

    pub fn set_connection_close(&mut self) {
        self.connection_close = true;
        if self.connection_values.is_empty() {
            self.connection_values.extend_from_slice(b"close");
        } else {
            self.connection_values.extend_from_slice(b", close");
        }
    }

    pub fn is_connection_close(&self) -> bool {
        self.connection_close
    }

    pub fn get_connection_values(&self) -> &[u8] {
        &self.connection_values
    }

    pub fn is_100_continue(&self) -> bool {
        self.get("expect")
            .map(|val| val.eq_ignore_ascii_case(b"100-continue"))
            .unwrap_or(false)
    }
}

impl fmt::Display for Headers<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (key, val) in &self.headers {
            writeln!(f, "{}: {}", key, String::from_utf8_lossy(val))?;
        }
        Ok(())
    }
}

impl<'a> From<Vec<(Cow<'a, str>, Cow<'a, [u8]>)>> for Headers<'a> {
    fn from(vec: Vec<(Cow<'a, str>, Cow<'a, [u8]>)>) -> Headers<'a> {
        let mut headers = Headers::new();
        for (k, v) in vec {
            headers.add(k, v);
        }
        headers
    }
}

impl<'a> From<&'a [(&str, &[u8])]> for Headers<'a> {
    fn from(slice: &'a [(&str, &[u8])]) -> Self {
        let mut headers = Headers::new();
        for (k, v) in slice {
            headers.add(*k, *v);
        }
        headers
    }
}
