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
        if self.scheme().is_some() {
            let after_scheme = &self.full[self.full.find("://").unwrap() + 3..];
            if let Some(idx) = after_scheme.find('/') {
                let path = &after_scheme[idx..];
                let end = path.find(['?', '#'].as_ref()).unwrap_or(path.len());
                &path[..end]
            } else {
                // RFC 7230: treat missing path as "/"
                "/"
            }
        } else {
            // relative URI (origin-form)
            let end = self
                .full
                .find(['?', '#'].as_ref())
                .unwrap_or(self.full.len());
            &self.full[..end]
        }
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
