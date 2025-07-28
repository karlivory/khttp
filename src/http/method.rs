use std::{
    fmt::{self, Display},
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
        match value {
            "GET" => Method::Get,
            "POST" => Method::Post,
            "HEAD" => Method::Head,
            "PUT" => Method::Put,
            "PATCH" => Method::Patch,
            "DELETE" => Method::Delete,
            "OPTIONS" => Method::Options,
            "TRACE" => Method::Trace,
            _ => Method::Custom(value.to_string()),
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
    pub fn index(&self) -> usize {
        match self {
            Method::Get => 0,
            Method::Post => 1,
            Method::Head => 2,
            Method::Put => 3,
            Method::Patch => 4,
            Method::Delete => 5,
            Method::Options => 6,
            Method::Trace => 7,
            Method::Custom(_) => 8,
        }
    }
}

impl Display for Method {
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
