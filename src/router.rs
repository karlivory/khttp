use crate::Method;
use std::{array::from_fn, cmp::max, collections::HashMap};

pub trait HttpRouter {
    type Route;
    fn match_route<'a, 'r>(&'a self, method: &Method, path: &'r str) -> Match<'a, 'r, Self::Route>;
}

pub struct RouterBuilder<T> {
    methods: [Vec<(RouteEntry, T)>; 8],
    extensions: HashMap<String, Vec<(RouteEntry, T)>>,
    fallback_route: T,
}

pub struct Router<T> {
    methods: [Vec<(RouteEntry, T)>; 8],
    extensions: HashMap<String, Vec<(RouteEntry, T)>>,
    fallback_route: T,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RouteEntry {
    pattern: Vec<RouteSegment>,
    literal: bool, // -> whether the route patterns contains only literals
}

impl<T> RouterBuilder<T> {
    pub fn new(fallback_route: T) -> Self {
        Self {
            methods: from_fn(|_| Vec::new()),
            extensions: HashMap::new(),
            fallback_route,
        }
    }

    pub fn add_route(&mut self, method: &Method, path: &str, route: T) {
        let route_entry = parse_route(path);
        let routes = match method {
            Method::Custom(x) => self.extensions.entry(x.clone()).or_default(),
            _ => &mut self.methods[method.index()],
        };

        routes.retain(|(k, _)| k != &route_entry);
        routes.push((route_entry, route));
    }

    pub fn set_fallback_route(&mut self, route: T) {
        self.fallback_route = route;
    }

    pub fn build(self) -> Router<T> {
        Router {
            methods: self.methods,
            extensions: self.extensions,
            fallback_route: self.fallback_route,
        }
    }
}

impl<T> HttpRouter for Router<T> {
    type Route = T;

    fn match_route<'a, 'r>(
        &'a self,
        method: &Method,
        mut uri: &'r str,
    ) -> Match<'a, 'r, Self::Route> {
        if uri.starts_with('/') {
            uri = &uri[1..];
        }

        let routes = match method {
            Method::Custom(x) => match self.extensions.get(x) {
                Some(r) => r,
                None => return Match::no_params(&self.fallback_route),
            },
            _ => &self.methods[method.index()],
        };

        let mut matched: Vec<(u16, Precedence, &[RouteSegment], &T, RouteParams)> = Vec::new();

        let mut max_lml = 0u16;
        for (RouteEntry { pattern, literal }, route) in routes.iter() {
            let mut uri_iter = uri.split('/');
            let mut params = RouteParams::new();
            let mut ok = true;
            let mut lml = 0u16;
            let mut counting_prefix = true;

            for seg_part in pattern.iter() {
                let uri_part = uri_iter.next();

                match seg_part {
                    RouteSegment::DoubleWildcard => break,
                    RouteSegment::Wildcard => {
                        if uri_part.is_none() {
                            ok = false;
                            break;
                        }
                        counting_prefix = false;
                    }
                    RouteSegment::Param(name) => {
                        if let Some(v) = uri_part {
                            params.insert(name.as_str(), v);
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

            // if there are still unmatched uri parts, pattern must end with **
            if ok && uri_iter.next().is_some() {
                ok = matches!(pattern.last(), Some(RouteSegment::DoubleWildcard));
            }

            if ok {
                if lml as usize == pattern.len() && *literal {
                    return Match::new(route, params); // perfect match
                }

                max_lml = max(max_lml, lml);
                let prec = precedence_of(pattern.last());
                matched.push((lml, prec, pattern, route, params));
            }
        }

        matched.retain(|(l, _, _, _, _)| *l == max_lml);
        if matched.is_empty() {
            return Match::no_params(&self.fallback_route);
        }

        matched.sort_by(|a, b| b.1.cmp(&a.1));
        let (_, _, _, best_route, params) = matched.remove(0);
        Match::new(best_route, params)
    }
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

#[derive(Debug, Clone)]
pub struct RouteParams<'a, 'r>(Vec<(&'a str, &'r str)>);

impl<'a, 'r> RouteParams<'a, 'r> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn get(&self, key: &str) -> Option<&'r str> {
        self.0.iter().find_map(|(k, v)| (*k == key).then_some(*v))
    }

    pub fn iter(&self) -> impl Iterator<Item = &(&'a str, &'r str)> + '_ {
        self.0.iter()
    }

    pub fn insert(&mut self, key: &'a str, val: &'r str) {
        self.0.push((key, val));
    }
}

impl Default for RouteParams<'_, '_> {
    fn default() -> Self {
        Self::new()
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

fn parse_route(mut route_str: &str) -> RouteEntry {
    if route_str.starts_with('/') {
        route_str = &route_str[1..];
    }
    let pattern: Vec<RouteSegment> = route_str.split('/').map(parse_route_segment).collect();
    let literal = pattern
        .iter()
        .all(|x| matches!(x, RouteSegment::Literal(_)));
    RouteEntry { pattern, literal }
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
