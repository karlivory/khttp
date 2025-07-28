use crate::Method;
use std::{
    cmp::max,
    collections::HashMap,
    hash::{Hash, Hasher},
};

pub trait HttpRouter {
    type Route;

    fn new() -> Self;
    fn match_route<'a, 'r>(
        &'a self,
        method: &Method,
        path: &'r str,
    ) -> Option<Match<'a, 'r, Self::Route>>;
    fn add_route(&mut self, method: &Method, path: &str, route: Self::Route);
    fn remove_route(&mut self, method: &Method, path: &str) -> Option<Self::Route>;
}

pub struct Router<T> {
    routes: HashMap<Method, HashMap<RouteEntry, T>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RouteEntry(Vec<RouteSegment>);

impl<T> HttpRouter for Router<T> {
    type Route = T;

    fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    fn add_route(&mut self, method: &Method, path: &str, route: T) {
        let route_entry = parse_route(path);
        self.routes
            .entry(method.clone())
            .or_default()
            .insert(route_entry, route);
    }

    fn remove_route(&mut self, method: &Method, path: &str) -> Option<T> {
        self.routes
            .get_mut(method)
            .and_then(|m| m.remove(&parse_route(path)))
    }

    fn match_route<'a, 'r>(
        &'a self,
        method: &Method,
        mut uri: &'r str,
    ) -> Option<Match<'a, 'r, Self::Route>> {
        if uri.starts_with("/") {
            uri = &uri[1..];
        }

        let routes = self.routes.get(method)?;

        #[allow(clippy::type_complexity)]
        let mut matched: Vec<(
            u16,                 // lml
            Precedence,          // precedence_tag
            &Vec<RouteSegment>,  // &pattern_segments
            &T,                  // &route
            HashMap<&str, &str>, // params
        )> = Vec::new();

        let mut max_lml = 0u16;
        for (RouteEntry(pattern), route) in routes.iter() {
            let mut uri_iter = uri.split('/');
            let mut params = HashMap::new();
            let mut ok = true;
            let mut lml = 0u16;
            let mut counting_prefix = true;

            for seg_part in pattern {
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

            // If there are still unmatched URI parts, the match fails unless we ended with **.
            if ok && uri_iter.next().is_some() {
                // pattern is exhausted but uri is not
                if !matches!(pattern.last(), Some(RouteSegment::DoubleWildcard)) {
                    ok = false;
                }
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

        matched.sort_by(|a, b| b.1.cmp(&a.1)); // descending precedence
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
    Wildcard,       // "*"
    DoubleWildcard, // "**"
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
            // TODO: write documentation on route overwriting: /users/:id will overwrite /users/:slug
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
