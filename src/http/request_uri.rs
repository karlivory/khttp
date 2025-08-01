#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestUri {
    full: String,
    scheme_i: usize,
    path_i_start: usize,
    path_i_end: usize,
}

impl RequestUri {
    pub fn new(uri: String, scheme_i: usize, path_i_start: usize, path_i_end: usize) -> Self {
        RequestUri {
            full: uri,
            scheme_i,
            path_i_start,
            path_i_end,
        }
    }

    pub fn full(&self) -> &str {
        &self.full
    }

    pub fn as_str(&self) -> &str {
        self.full.as_str()
    }

    pub fn scheme(&self) -> Option<&str> {
        // TODO: use scheme_i
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
        &self.full[self.path_i_start..self.path_i_end]
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
