use std::collections::{HashMap, hash_map};
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};

#[derive(Debug, Clone, PartialEq)]
pub enum HeaderValue {
    Single(String),
    Multi(Vec<String>),
}

impl HeaderValue {
    pub fn iter(&self) -> HeaderValueIter<'_> {
        match self {
            HeaderValue::Single(val) => HeaderValueIter::Single(Some(val.as_str())),
            HeaderValue::Multi(vals) => HeaderValueIter::Multi(vals.iter().map(|s| s.as_str())),
        }
    }

    pub fn last(&self) -> Option<&str> {
        match self {
            HeaderValue::Single(val) => Some(val.as_str()),
            HeaderValue::Multi(vals) => vals.last().map(|s| s.as_str()),
        }
    }
}

pub enum HeaderValueIter<'a> {
    Single(Option<&'a str>),
    Multi(std::iter::Map<std::slice::Iter<'a, String>, fn(&String) -> &str>),
}

impl<'a> Iterator for HeaderValueIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            HeaderValueIter::Single(opt) => opt.take(),
            HeaderValueIter::Multi(iter) => iter.next(),
        }
    }
}

impl<'a> IntoIterator for &'a HeaderValue {
    type Item = &'a str;
    type IntoIter = HeaderValueIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Headers {
    headers: HashMap<String, HeaderValue, BuildHasherDefault<AsciiHasher>>,
    content_length: Option<u64>,
}

impl Headers {
    pub fn new() -> Self {
        Self {
            headers: HashMap::default(),
            content_length: None,
        }
    }

    pub fn get_map(&self) -> &HashMap<String, HeaderValue, BuildHasherDefault<AsciiHasher>> {
        &self.headers
    }

    pub fn get_count(&self) -> usize {
        self.headers.len()
    }

    pub fn add(&mut self, name: &str, value: &str) {
        let key = name.to_ascii_lowercase();
        if key == Self::CONTENT_LENGTH {
            self.content_length = value.trim().parse().ok();
            return;
        }

        match self.headers.entry(key) {
            hash_map::Entry::Vacant(e) => {
                e.insert(HeaderValue::Single(value.to_string()));
            }
            hash_map::Entry::Occupied(mut e) => match e.get_mut() {
                HeaderValue::Single(existing) => {
                    let old = std::mem::take(existing);
                    *e.get_mut() = HeaderValue::Multi(vec![old, value.to_string()]);
                }
                HeaderValue::Multi(vec) => vec.push(value.to_string()),
            },
        }
    }

    /// # Safety
    /// Caller must ensure `name` is lowercase ASCII-US.
    pub unsafe fn add_unchecked(&mut self, name: &str, value: &str) {
        if name == Self::CONTENT_LENGTH {
            self.content_length = value.trim().parse().ok();
            return;
        }

        match self.headers.entry(name.to_string()) {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(HeaderValue::Single(value.to_string()));
            }
            std::collections::hash_map::Entry::Occupied(mut e) => match e.get_mut() {
                HeaderValue::Single(existing) => {
                    let old = std::mem::take(existing);
                    *e.get_mut() = HeaderValue::Multi(vec![old, value.to_string()]);
                }
                HeaderValue::Multi(vec) => vec.push(value.to_string()),
            },
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_ascii_lowercase())?.last()
    }

    pub fn get_all(&self, name: &str) -> impl Iterator<Item = &str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .into_iter()
            .flat_map(|hv| hv.iter())
    }

    pub fn set(&mut self, name: &str, value: &str) {
        let key = name.to_ascii_lowercase();
        if key == Self::CONTENT_LENGTH {
            self.content_length = value.trim().parse().ok();
            return;
        }
        self.headers
            .insert(key, HeaderValue::Single(value.to_string()));
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

    pub fn set_content_length(&mut self, len: u64) {
        self.content_length = Some(len);
    }

    pub fn set_transfer_encoding_chunked(&mut self) {
        self.set(Self::TRANSFER_ENCODING, "chunked");
    }

    pub fn is_transfer_encoding_chunked(&self) -> bool {
        self.get(Self::TRANSFER_ENCODING)
            .map(|te| {
                te.split(',')
                    .any(|t| t.trim().eq_ignore_ascii_case("chunked"))
            })
            .unwrap_or(false)
    }

    pub fn set_connection_close(&mut self) {
        self.set(Self::CONNECTION, "close");
    }

    pub fn is_connection_close(&self) -> bool {
        self.get(Self::CONNECTION)
            .map(|val| val.eq_ignore_ascii_case("close"))
            .unwrap_or(false)
    }

    pub fn is_100_continue(&self) -> bool {
        self.get("expect")
            .map(|val| val.eq_ignore_ascii_case("100-continue"))
            .unwrap_or(false)
    }
}

impl fmt::Display for Headers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (key, value) in &self.headers {
            for val in value.iter() {
                writeln!(f, "{}: {}", key, val)?;
            }
        }
        Ok(())
    }
}

impl From<Vec<(&str, &str)>> for Headers {
    fn from(vec: Vec<(&str, &str)>) -> Self {
        let mut headers = Headers::new();
        for (k, v) in vec {
            headers.add(k, v);
        }
        headers
    }
}

impl From<&[(&str, &[&str])]> for Headers {
    fn from(slice: &[(&str, &[&str])]) -> Self {
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
