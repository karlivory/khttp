use std::{
    fmt::{self},
    str::FromStr,
};

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum Method {
    Get,
    Post,
    Head,
    Put,
    Patch,
    Delete,
    Options,
    Trace,
    Custom(String),
}

impl From<&str> for Method {
    fn from(value: &str) -> Self {
        // compare ignoring ASCII case without allocating
        if value.eq_ignore_ascii_case("GET") {
            Method::Get
        } else if value.eq_ignore_ascii_case("POST") {
            Method::Post
        } else if value.eq_ignore_ascii_case("HEAD") {
            Method::Head
        } else if value.eq_ignore_ascii_case("PUT") {
            Method::Put
        } else if value.eq_ignore_ascii_case("PATCH") {
            Method::Patch
        } else if value.eq_ignore_ascii_case("DELETE") {
            Method::Delete
        } else if value.eq_ignore_ascii_case("OPTIONS") {
            Method::Options
        } else if value.eq_ignore_ascii_case("TRACE") {
            Method::Trace
        } else {
            Method::Custom(value.to_string())
        }
    }
}

impl Method {
    pub fn as_str(&self) -> &str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Head => "HEAD",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Options => "OPTIONS",
            Method::Trace => "TRACE",
            Method::Custom(s) => s.as_str(),
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Method {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Method::from(s))
    }
}

impl AsRef<str> for Method {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq<&str> for Method {
    fn eq(&self, other: &&str) -> bool {
        self.as_str().eq_ignore_ascii_case(other)
    }
}

impl PartialEq<String> for Method {
    fn eq(&self, other: &String) -> bool {
        self.as_str().eq_ignore_ascii_case(other)
    }
}
