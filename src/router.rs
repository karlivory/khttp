use crate::Method;
use std::{array::from_fn, collections::HashMap, mem};

pub struct RouterBuilder<T> {
    methods: [MethodBucket<T>; 8],
    extensions: HashMap<String, MethodBucket<T>>,
    fallback_route: T,
}

pub struct Router<T> {
    methods: [MethodBucket<T>; 8],
    extensions: HashMap<String, MethodBucket<T>>,
    fallback_route: T,
}

#[derive(Debug, Clone)]
pub struct Match<'a, 'r, T> {
    pub route: &'a T,
    pub params: RouteParams<'a, 'r>,
}

impl<'a, 'r, T> Match<'a, 'r, T> {
    pub fn new(route: &'a T, params: RouteParams<'a, 'r>) -> Self {
        Match { route, params }
    }

    pub fn no_params(route: &'a T) -> Self {
        Match {
            route,
            params: RouteParams::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RouteParams<'a, 'r>(Vec<(&'a str, &'r str)>);

impl<'a, 'r> RouteParams<'a, 'r> {
    #[inline]
    pub fn new() -> Self {
        Self(Vec::new())
    }

    #[inline]
    pub fn clear(&mut self) {
        self.0.clear();
    }

    #[inline]
    pub fn get(&self, key: &str) -> Option<&'r str> {
        self.0.iter().find_map(|(k, v)| (*k == key).then_some(*v))
    }

    #[inline]
    pub fn insert(&mut self, key: &'a str, val: &'r str) {
        if self.0.is_empty() {
            self.0.reserve_exact(1)
        }
        self.0.push((key, val));
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&'a str, &'r str)> + '_ {
        self.0.iter().copied()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RoutePattern {
    pattern: Vec<RouteSegment>,
    last_prec: Precedence,
}

/// Per-method storage:
/// - `literals`: exact, all-literal paths as full strings (normalized, no leading '/')
/// - `patterns`: param/wildcard routes
#[derive(Debug, Clone)]
struct MethodBucket<T> {
    literals: Vec<(String, T)>,
    patterns: Vec<(RoutePattern, T)>,
}

impl<T> Default for MethodBucket<T> {
    fn default() -> Self {
        Self {
            literals: Vec::new(),
            patterns: Vec::new(),
        }
    }
}

impl<T> MethodBucket<T> {
    fn add_route(&mut self, path: &str, route: T) {
        let (norm, entry) = parse_route(path);
        let literal = entry
            .pattern
            .iter()
            .all(|x| matches!(x, RouteSegment::Literal(_)));

        if literal {
            self.literals.retain(|(k, _)| k != &norm);
            self.literals.push((norm, route));
        } else {
            self.patterns.retain(|(k, _)| k != &entry);
            self.patterns.push((entry, route));
        }
    }

    /// Finalize once at build time: sort literals for binary search.
    fn finalize(&mut self) {
        self.literals.sort_unstable_by(|a, b| a.0.cmp(&b.0));
    }

    #[inline]
    fn find_literal(&self, norm_path: &str) -> Option<&T> {
        match self
            .literals
            .binary_search_by_key(&norm_path, |(k, _)| k.as_str())
        {
            Ok(i) => Some(&self.literals[i].1),
            Err(_) => None,
        }
    }
}

impl<T> RouterBuilder<T> {
    pub fn new(fallback_route: T) -> Self {
        Self {
            methods: from_fn(|_| MethodBucket::default()),
            extensions: HashMap::new(),
            fallback_route,
        }
    }

    pub fn add_route(&mut self, method: &Method, path: &str, route: T) {
        match method {
            Method::Custom(x) => {
                self.extensions
                    .entry(x.clone())
                    .or_default()
                    .add_route(path, route);
            }
            _ => self.methods[method.index()].add_route(path, route),
        }
    }

    pub fn set_fallback_route(&mut self, route: T) {
        self.fallback_route = route;
    }

    pub fn build(mut self) -> Router<T> {
        for b in &mut self.methods {
            b.finalize();
        }
        for b in self.extensions.values_mut() {
            b.finalize();
        }
        Router {
            methods: self.methods,
            extensions: self.extensions,
            fallback_route: self.fallback_route,
        }
    }
}

impl<T> Router<T> {
    pub fn match_route<'a, 'r>(&'a self, method: &Method, mut uri: &'r str) -> Match<'a, 'r, T> {
        if uri.starts_with('/') {
            uri = &uri[1..]; // normalize: strip leading slash
        }

        let bucket = match method {
            Method::Custom(x) => match self.extensions.get(x) {
                Some(b) => b,
                None => return Match::no_params(&self.fallback_route),
            },
            _ => &self.methods[method.index()],
        };

        // fast path: exact literal route
        if let Some(route) = bucket.find_literal(uri) {
            return Match::no_params(route);
        }

        let mut best_lml: i32 = -1;
        let mut best_prec = Precedence::DoubleWildcard;
        let mut best_route: Option<&T> = None;
        let mut best_params = RouteParams::new();

        let mut route_params = RouteParams::new();
        for (RoutePattern { pattern, last_prec }, route) in &bucket.patterns {
            let mut uri_iter = uri.split('/');
            let mut ok = true;
            let mut lml = 0; // longest matching literal
            let mut counting_prefix = true;

            route_params.clear();
            for seg_part in pattern.iter() {
                let uri_part = uri_iter.next();

                match seg_part {
                    RouteSegment::DoubleWildcard => break, // matches until end
                    RouteSegment::Wildcard => {
                        if uri_part.is_none() {
                            ok = false;
                            break;
                        }
                        counting_prefix = false;
                    }
                    RouteSegment::Param(name) => {
                        if let Some(v) = uri_part {
                            route_params.insert(name.as_str(), v);
                        } else {
                            ok = false;
                            break;
                        }
                        counting_prefix = false;
                    }
                    RouteSegment::Literal(lit) => {
                        if let Some(v) = uri_part {
                            if lit == v {
                                if counting_prefix {
                                    lml += 1;
                                }
                            } else {
                                ok = false;
                                break;
                            }
                        } else {
                            ok = false;
                            break;
                        }
                    }
                }
            }

            // if uri has extra parts, pattern must end with "**"
            if ok && uri_iter.next().is_some() {
                ok = *last_prec == Precedence::DoubleWildcard;
            }

            if !ok {
                continue;
            }

            // compare against best (tie-break on precedence)
            if lml > best_lml || (lml == best_lml && *last_prec > best_prec) {
                best_lml = lml;
                best_prec = *last_prec;
                best_route = Some(route);
                mem::swap(&mut best_params, &mut route_params);
            }
        }

        match best_route {
            Some(route) => Match::new(route, best_params),
            None => Match::no_params(&self.fallback_route),
        }
    }
}

#[derive(Debug, Clone, Eq)]
enum RouteSegment {
    Literal(String),
    Param(String),
    Wildcard,
    DoubleWildcard,
}

impl PartialEq for RouteSegment {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Literal(l0), Self::Literal(r0)) => l0 == r0,
            (Self::Param(_), Self::Param(_)) => true,
            (Self::Wildcard, Self::Wildcard) => true,
            (Self::DoubleWildcard, Self::DoubleWildcard) => true,
            _ => false,
        }
    }
}

/// Returns ("normalized path without leading slash", "route pattern")
fn parse_route(mut route_str: &str) -> (String, RoutePattern) {
    if route_str.starts_with('/') {
        route_str = &route_str[1..];
    }
    let norm = route_str.to_string(); // "" for "/"
    let pattern: Vec<RouteSegment> = route_str.split('/').map(parse_route_segment).collect();
    let last_prec = precedence_of(pattern.last());
    (norm, RoutePattern { pattern, last_prec })
}

fn parse_route_segment(s: &str) -> RouteSegment {
    match s {
        "*" => RouteSegment::Wildcard,
        "**" => RouteSegment::DoubleWildcard,
        _ if s.starts_with(':') => RouteSegment::Param(s[1..].to_string()),
        x => RouteSegment::Literal(x.to_string()),
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
enum Precedence {
    DoubleWildcard = 0,
    Wildcard = 1,
    Param = 2,
    Literal = 3,
}

fn precedence_of(last: Option<&RouteSegment>) -> Precedence {
    match last {
        Some(RouteSegment::Literal(_)) => Precedence::Literal,
        Some(RouteSegment::Param(_)) => Precedence::Param,
        Some(RouteSegment::Wildcard) => Precedence::Wildcard,
        Some(RouteSegment::DoubleWildcard) => Precedence::DoubleWildcard,
        None => Precedence::DoubleWildcard,
    }
}
