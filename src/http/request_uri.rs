#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestUri {
    full: String,
}

impl From<&str> for RequestUri {
    fn from(value: &str) -> Self {
        Self {
            full: value.to_string(),
        }
    }
}

impl RequestUri {
    pub fn new(uri: String) -> Self {
        RequestUri { full: uri }
    }

    pub fn full(&self) -> &str {
        &self.full
    }

    pub fn as_str(&self) -> &str {
        self.full.as_str()
    }

    pub fn scheme(&self) -> Option<&str> {
        self.full.find("://").map(|idx| &self.full[..idx])
    }

    pub fn authority(&self) -> Option<&str> {
        let rest = self.full.strip_prefix(&format!("{}://", self.scheme()?))?;
        match rest.find('/') {
            Some(idx) => Some(&rest[..idx]),
            None => Some(rest),
        }
    }

    pub fn path(&self) -> &str {
        let uri = self.full.as_str();
        let bytes = uri.as_bytes();

        // Case 1: origin-form (starts with '/')
        if bytes.first() == Some(&b'/') {
            for (i, &b) in bytes.iter().enumerate() {
                if b == b'?' || b == b'#' {
                    return &uri[..i];
                }
            }
            return uri;
        }

        // Case 2: absolute-form (e.g., http://host/path)
        if let Some(mut scheme_end) = find_colon_slash_slash(bytes) {
            scheme_end += 3; // skip "://"
            for i in scheme_end..bytes.len() {
                if bytes[i] == b'/' {
                    let start = i;
                    for (j, c) in bytes.iter().enumerate() {
                        if *c == b'?' || *c == b'#' {
                            return &uri[start..j];
                        }
                    }
                    return &uri[start..];
                }
            }
            return "/";
        }

        // Case 3: authority-form or fallback
        let mut end = bytes.len();
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'?' || b == b'#' {
                end = i;
                break;
            }
        }
        &uri[..end]
    }

    pub fn query(&self) -> Option<&str> {
        let hash_idx = self.full.find('#');
        let qmark_idx = self.full.find('?')?;

        if let Some(hash_pos) = hash_idx {
            if qmark_idx > hash_pos {
                return None;
            }
        }

        let end = hash_idx.unwrap_or(self.full.len());
        Some(&self.full[qmark_idx + 1..end])
    }

    pub fn fragment(&self) -> Option<&str> {
        self.full.find('#').map(|idx| &self.full[idx + 1..])
    }
}

#[inline(always)]
fn find_colon_slash_slash(bytes: &[u8]) -> Option<usize> {
    let n = bytes.len().saturating_sub(2);
    if n > 2 {
        for i in 0..n {
            if bytes[i] == b':' && bytes[i + 1] == b'/' && bytes[i + 2] == b'/' {
                return Some(i);
            }
        }
    }
    None
}
