use crate::Method;
use std::{
    array::from_fn,
    cmp::max,
    collections::HashMap,
    hash::{Hash, Hasher},
};

pub trait HttpRouter {
    type Route;

    fn match_route<'a, 'r>(
        &'a self,
        method: &Method,
        path: &'r str,
    ) -> Option<Match<'a, 'r, Self::Route>>;
}

#[derive(Default)]
pub struct RouterBuilder<T> {
    standard_methods: [HashMap<RouteEntry, T>; 8],
    extensions: HashMap<String, HashMap<RouteEntry, T>>,
}

pub struct Router<T> {
    standard_methods: [Vec<(RouteEntry, T)>; 8],
    extensions: HashMap<String, Vec<(RouteEntry, T)>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RouteEntry(Vec<RouteSegment>);

impl<T> RouterBuilder<T> {
    pub fn new() -> Self {
        Self {
            standard_methods: from_fn(|_| HashMap::new()),
            extensions: HashMap::new(),
        }
    }
    pub fn add_route(&mut self, method: &Method, path: &str, route: T) {
        let route_entry = parse_route(path);
        match method {
            Method::Custom(x) => self
                .extensions
                .entry(x.clone())
                .or_default()
                .insert(route_entry, route),
            _ => self.standard_methods[method.index()].insert(route_entry, route),
        };
    }

    pub fn build(self) -> Router<T> {
        let mut standard_methods: [Vec<(RouteEntry, T)>; 8] = Default::default();

        for (i, map) in self.standard_methods.into_iter().enumerate() {
            standard_methods[i] = map.into_iter().collect();
        }
        let extensions: HashMap<String, Vec<(RouteEntry, T)>> = self
            .extensions
            .into_iter()
            .map(|(x, y)| (x, y.into_iter().collect()))
            .collect();

        Router {
            standard_methods,
            extensions,
        }
    }
}

impl<T> HttpRouter for Router<T> {
    type Route = T;

    fn match_route<'a, 'r>(
        &'a self,
        method: &Method,
        mut uri: &'r str,
    ) -> Option<Match<'a, 'r, Self::Route>> {
        if uri.starts_with("/") {
            uri = &uri[1..];
        }

        let routes = match method {
            Method::Custom(x) => self.extensions.get(x)?,
            _ => &self.standard_methods[method.index()],
        };

        #[allow(clippy::type_complexity)]
        let mut matched: Vec<(
            u16,
            Precedence,
            &Vec<RouteSegment>,
            &T,
            HashMap<&str, &str>,
        )> = Vec::new();

        let mut max_lml = 0u16;
        for (RouteEntry(pattern), route) in routes.iter() {
            let mut uri_iter = uri.split('/');
            let mut params = HashMap::new();
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
                if lml as usize == pattern.len()
                    && pattern
                        .iter()
                        .all(|s| matches!(s, RouteSegment::Literal(_)))
                {
                    return Some(Match { route, params });
                }

                max_lml = max(max_lml, lml);
                let prec = precedence_of(pattern.last());
                matched.push((lml, prec, pattern, route, params));
            }
        }

        matched.retain(|(l, _, _, _, _)| *l == max_lml);
        if matched.is_empty() {
            return None;
        }

        matched.sort_by(|a, b| b.1.cmp(&a.1));
        let (_, _, _, best_route, params) = matched.remove(0);
        Some(Match {
            route: best_route,
            params,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Match<'a, 'r, T> {
    pub route: &'a T,
    pub params: HashMap<&'a str, &'r str>,
}

#[derive(Debug, Clone, Eq)]
pub enum RouteSegment {
    Literal(String),
    Param(String),
    Wildcard,
    DoubleWildcard,
}

impl Hash for RouteSegment {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            RouteSegment::Literal(s) => {
                core::mem::discriminant(self).hash(state);
                s.hash(state);
            }
            RouteSegment::Wildcard | RouteSegment::DoubleWildcard | RouteSegment::Param(_) => {
                core::mem::discriminant(self).hash(state);
            }
        }
    }
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

fn parse_route_segment(s: &str) -> RouteSegment {
    match s {
        "*" => RouteSegment::Wildcard,
        "**" => RouteSegment::DoubleWildcard,
        _ if s.starts_with(':') => RouteSegment::Param(s[1..].to_string()),
        x => RouteSegment::Literal(x.to_string()),
    }
}

pub fn parse_route(mut route_str: &str) -> RouteEntry {
    if route_str.starts_with("/") {
        route_str = &route_str[1..];
    }
    let segments = route_str.split('/').map(parse_route_segment).collect();
    RouteEntry(segments)
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
