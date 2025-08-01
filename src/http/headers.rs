use std::collections::{HashMap, hash_map};
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};

#[derive(Debug, Clone, PartialEq)]
pub enum HeaderValue {
    Single(Vec<u8>),
    Multi(Vec<Vec<u8>>),
}

impl HeaderValue {
    pub fn iter(&self) -> HeaderValueIter<'_> {
        match self {
            HeaderValue::Single(val) => HeaderValueIter::Single(Some(val.as_slice())),
            HeaderValue::Multi(vals) => HeaderValueIter::Multi(vals.iter().map(|v| v.as_slice())),
        }
    }

    pub fn last(&self) -> Option<&[u8]> {
        match self {
            HeaderValue::Single(val) => Some(val.as_slice()),
            HeaderValue::Multi(vals) => vals.last().map(|v| v.as_slice()),
        }
    }
}

pub enum HeaderValueIter<'a> {
    Single(Option<&'a [u8]>),
    Multi(std::iter::Map<std::slice::Iter<'a, Vec<u8>>, fn(&Vec<u8>) -> &[u8]>),
}

impl<'a> Iterator for HeaderValueIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            HeaderValueIter::Single(opt) => opt.take(),
            HeaderValueIter::Multi(iter) => iter.next(),
        }
    }
}

impl<'a> IntoIterator for &'a HeaderValue {
    type Item = &'a [u8];
    type IntoIter = HeaderValueIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Headers {
    headers: HashMap<String, HeaderValue, BuildHasherDefault<AsciiHasher>>,
    content_length: Option<u64>,
    chunked: bool,
    transfer_encoding: Vec<Vec<u8>>,
    connection_close: bool,
    connection_values: Vec<Vec<u8>>,
}

impl Headers {
    pub fn new() -> Self {
        Self {
            headers: HashMap::default(),
            content_length: None,
            transfer_encoding: Vec::new(),
            chunked: false,
            connection_close: false,
            connection_values: Vec::new(),
        }
    }

    pub fn get_map(&self) -> &HashMap<String, HeaderValue, BuildHasherDefault<AsciiHasher>> {
        &self.headers
    }

    pub fn get_count(&self) -> usize {
        self.headers.len()
    }

    pub fn add(&mut self, name: &str, value: &[u8]) {
        match name {
            Self::CONTENT_LENGTH => {
                if let Ok(s) = std::str::from_utf8(value) {
                    self.content_length = s.trim().parse().ok();
                }
                return;
            }
            Self::TRANSFER_ENCODING => {
                value
                    .split(|&b| b == b',')
                    .map(|v| v.trim_ascii_start())
                    .for_each(|v| {
                        if v.eq_ignore_ascii_case(b"chunked") {
                            self.chunked = true;
                        }
                        self.transfer_encoding.push(v.to_vec());
                    });
                return;
            }
            Self::CONNECTION => {
                value
                    .split(|&b| b == b',')
                    .map(|v| v.trim_ascii_start())
                    .for_each(|v| {
                        if v.eq_ignore_ascii_case(b"close") {
                            self.connection_close = true;
                        }
                        self.connection_values.push(v.to_vec());
                    });
                return;
            }
            _ => (),
        }

        let key = name.to_ascii_lowercase();
        match self.headers.entry(key) {
            hash_map::Entry::Vacant(e) => {
                e.insert(HeaderValue::Single(value.to_vec()));
            }
            hash_map::Entry::Occupied(mut e) => match e.get_mut() {
                HeaderValue::Single(existing) => {
                    let old = std::mem::take(existing);
                    *e.get_mut() = HeaderValue::Multi(vec![old, value.to_vec()]);
                }
                HeaderValue::Multi(vec) => vec.push(value.to_vec()),
            },
        }
    }

    /// # Safety
    /// Caller must ensure `name` is lowercase ASCII-US.
    pub unsafe fn add_unchecked(&mut self, name: &str, value: &[u8]) {
        match name {
            Self::CONTENT_LENGTH => {
                if let Ok(s) = std::str::from_utf8(value) {
                    self.content_length = s.trim().parse().ok();
                }
                return;
            }
            Self::TRANSFER_ENCODING => {
                value
                    .split(|&b| b == b',')
                    .map(|v| v.trim_ascii_start())
                    .for_each(|v| {
                        if v.eq_ignore_ascii_case(b"chunked") {
                            self.chunked = true;
                        }
                        self.transfer_encoding.push(v.to_vec());
                    });
                return;
            }
            Self::CONNECTION => {
                value
                    .split(|&b| b == b',')
                    .map(|v| v.trim_ascii_start())
                    .for_each(|v| {
                        if v.eq_ignore_ascii_case(b"close") {
                            self.connection_close = true;
                        }
                        self.connection_values.push(v.to_vec());
                    });
                return;
            }
            _ => (),
        }

        match self.headers.entry(name.to_string()) {
            hash_map::Entry::Vacant(e) => {
                e.insert(HeaderValue::Single(value.to_vec()));
            }
            hash_map::Entry::Occupied(mut e) => match e.get_mut() {
                HeaderValue::Single(existing) => {
                    let old = std::mem::take(existing);
                    *e.get_mut() = HeaderValue::Multi(vec![old, value.to_vec()]);
                }
                HeaderValue::Multi(vec) => vec.push(value.to_vec()),
            },
        }
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.headers.get(&name.to_ascii_lowercase())?.last()
    }

    pub fn get_all(&self, name: &str) -> impl Iterator<Item = &[u8]> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .into_iter()
            .flat_map(|hv| hv.iter())
    }

    pub fn set(&mut self, name: &str, value: &[u8]) {
        let key = name.to_ascii_lowercase();
        if key == Self::CONTENT_LENGTH {
            if let Ok(s) = std::str::from_utf8(value) {
                self.content_length = s.trim().parse().ok();
            }
            return;
        }
        self.headers
            .insert(key, HeaderValue::Single(value.to_vec()));
    }

    pub fn remove(&mut self, name: &str) -> Option<HeaderValue> {
        let key = name.to_ascii_lowercase();
        if key == Self::CONTENT_LENGTH {
            self.content_length = None;
            return None;
        }
        self.headers.remove(&key)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.headers.contains_key(&name.to_ascii_lowercase())
    }

    pub const CONTENT_LENGTH: &str = "content-length";
    pub const CONTENT_TYPE: &str = "content-type";
    pub const TRANSFER_ENCODING: &str = "transfer-encoding";
    pub const CONNECTION: &str = "connection";

    pub fn get_content_length(&self) -> Option<u64> {
        self.content_length
    }

    pub fn set_content_length(&mut self, len: Option<u64>) {
        self.content_length = len;
    }

    pub fn set_transfer_encoding_chunked(&mut self) {
        self.chunked = true;
        self.transfer_encoding.push(b"chunked".to_vec());
    }

    pub fn is_transfer_encoding_chunked(&self) -> bool {
        self.chunked
    }

    pub fn get_transfer_encoding(&self) -> &Vec<Vec<u8>> {
        &self.transfer_encoding
    }

    pub fn set_connection_close(&mut self) {
        self.connection_close = true;
        self.connection_values.push(b"close".to_vec());
    }

    pub fn is_connection_close(&self) -> bool {
        self.connection_close
    }

    pub fn get_connection_values(&self) -> &Vec<Vec<u8>> {
        &self.connection_values
    }

    pub fn is_100_continue(&self) -> bool {
        self.get("expect")
            .map(|val| val.eq_ignore_ascii_case(b"100-continue"))
            .unwrap_or(false)
    }
}

impl fmt::Display for Headers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (key, value) in &self.headers {
            for val in value.iter() {
                writeln!(f, "{}: {}", key, String::from_utf8_lossy(val))?;
            }
        }
        Ok(())
    }
}

impl From<Vec<(&str, &[u8])>> for Headers {
    fn from(vec: Vec<(&str, &[u8])>) -> Self {
        let mut headers = Headers::new();
        for (k, v) in vec {
            headers.add(k, v);
        }
        headers
    }
}

impl From<&[(&str, &[&[u8]])]> for Headers {
    fn from(slice: &[(&str, &[&[u8]])]) -> Self {
        let mut headers = Headers::new();
        for (k, vs) in slice {
            for v in *vs {
                headers.add(k, v);
            }
        }
        headers
    }
}

#[derive(Default)]
pub struct AsciiHasher(u64);

impl Hasher for AsciiHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.wrapping_mul(31) ^ b as u64;
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}
