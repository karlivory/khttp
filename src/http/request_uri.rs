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

        // origin-form
        if uri.starts_with('/') {
            return uri.split(['?', '#']).next().unwrap_or("/");
        }

        // absolute-form
        if let Some(scheme_end) = uri.find("://") {
            let after_scheme = &uri[scheme_end + 3..];
            if let Some(path_start) = after_scheme.find('/') {
                return after_scheme[path_start..]
                    .split(['?', '#'])
                    .next()
                    .unwrap_or("/");
            } else {
                return "/";
            }
        }

        // fallback / authority-form
        uri.split(['?', '#']).next().unwrap_or("/")
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
