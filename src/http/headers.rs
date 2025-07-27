use std::collections::HashMap;

#[derive(Debug, Default, Clone, PartialEq)]

pub struct Headers {
    headers: HashMap<String, Vec<String>>,
}

impl From<HashMap<&str, &str>> for Headers {
    fn from(value: HashMap<&str, &str>) -> Self {
        let mut headers = Headers::new();
        for (key, val) in value {
            headers.add(key, val);
        }
        headers
    }
}

impl From<HashMap<String, String>> for Headers {
    fn from(value: HashMap<String, String>) -> Self {
        let mut headers = Headers::new();
        for (key, val) in value {
            headers.add(&key, &val);
        }
        headers
    }
}

impl From<Vec<(&str, &str)>> for Headers {
    fn from(value: Vec<(&str, &str)>) -> Self {
        let mut headers = Headers::new();
        for (key, val) in value {
            headers.add(key, val);
        }
        headers
    }
}

impl From<&[(&str, &str)]> for Headers {
    fn from(value: &[(&str, &str)]) -> Self {
        let mut headers = Headers::new();
        for (key, val) in value {
            headers.add(key, val);
        }
        headers
    }
}

impl From<&[(&str, &[&str])]> for Headers {
    fn from(value: &[(&str, &[&str])]) -> Self {
        let mut headers = Headers::new();
        for (key, vals) in value {
            for val in *vals {
                headers.add(key, val);
            }
        }
        headers
    }
}

impl Headers {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get_map(&self) -> &HashMap<String, Vec<String>> {
        &self.headers
    }

    pub fn get_map_mut(&mut self) -> &mut HashMap<String, Vec<String>> {
        &mut self.headers
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
        self.set(Self::TRANSFER_ENCODING, "chunked");
    }

    pub fn is_transfer_encoding_chunked(&self) -> bool {
        match self.get(Self::TRANSFER_ENCODING) {
            Some(te) => te
                .split(',')
                .any(|t| t.trim().eq_ignore_ascii_case("chunked")),
            None => false,
        }
    }

    pub fn set_connection_close(&mut self) {
        self.set(Self::CONNECTION, "close");
    }

    pub fn is_connection_close(&self) -> bool {
        match self.get(Self::CONNECTION) {
            Some(te) => te.eq_ignore_ascii_case("close"),
            None => false,
        }
    }

    pub fn is_100_continue(&self) -> bool {
        match self.get("expect") {
            Some(te) => te.eq_ignore_ascii_case("100-continue"),
            None => false,
        }
    }
}

impl std::fmt::Display for Headers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (k, vs) in &self.headers {
            for v in vs {
                writeln!(f, "{}: {}", k, v)?;
            }
        }
        Ok(())
    }
}
