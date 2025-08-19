#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestUri<'a> {
    full: &'a str,
    path_i_start: usize,
    path_i_end: usize,
}

impl<'a> RequestUri<'a> {
    pub fn new(uri: &'a str, path_i_start: usize, path_i_end: usize) -> Self {
        RequestUri {
            full: uri,
            path_i_start,
            path_i_end,
        }
    }

    pub fn as_str(&self) -> &str {
        self.full
    }

    pub fn scheme(&self) -> Option<&str> {
        self.full.find("://").map(|idx| &self.full[..idx])
    }

    pub fn authority(&self) -> Option<&str> {
        if let Some(scheme_i) = self.full.find("://") {
            // absolute-form, e.g. https://example.com[:port][/path][?query]
            let start = scheme_i + 3;
            if self.path_i_start == 0 {
                let rest = &self.full[start..];
                match rest.find('?') {
                    Some(i) => Some(&rest[..i]),
                    None => Some(rest),
                }
            } else {
                Some(&self.full[start..self.path_i_start])
            }
        } else if self.full.starts_with('/') {
            // origin-form: no authority
            None
        } else {
            // authority-form (CONNECT), e.g. example.com[:port]
            match self.full.find(['/', '?']) {
                Some(i) => Some(&self.full[..i]),
                None => Some(self.full),
            }
        }
    }

    pub fn path(&self) -> &str {
        &self.full[self.path_i_start..self.path_i_end]
    }

    pub fn path_and_query(&self) -> &str {
        if self.path_i_end != 0 {
            &self.full[self.path_i_start..]
        } else {
            if let Some(scheme_i) = self.full.find("://") {
                if let Some(rel_q) = self.full[scheme_i + 3..].find('?') {
                    return &self.full[scheme_i + 3 + rel_q..];
                }
            }
            ""
        }
    }

    pub fn query(&self) -> Option<&str> {
        let path_start = &self.full[self.path_i_end..];
        let qmark_i = path_start.find('?')?;
        Some(&path_start[qmark_i + 1..])
    }
}

impl std::fmt::Display for RequestUri<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.full)
    }
}
