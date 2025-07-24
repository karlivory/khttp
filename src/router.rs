// src/router.rs

use std::{
    cmp::max,
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::Arc,
};

use crate::common::HttpMethod;

pub trait AppRouter {
    type Route;

    fn new() -> Self;
    fn match_route<'a, 'r>(
        &'a self,
        method: &HttpMethod,
        path: &'r str,
    ) -> Option<Match<'a, 'r, Self::Route>>;
    fn add_route(&mut self, method: &HttpMethod, path: &str, route: Self::Route);
    fn remove_route(&mut self, method: &HttpMethod, path: &str) -> Option<Arc<Self::Route>>;
}

pub struct DefaultRouter<T> {
    routes: HashMap<HttpMethod, HashMap<RouteEntry, Arc<T>>>,
}

impl<T> Default for DefaultRouter<T> {
    fn default() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }
}

impl<T> AppRouter for DefaultRouter<T> {
    type Route = T;

    fn new() -> Self {
        Self::default()
    }

    fn add_route(&mut self, method: &HttpMethod, path: &str, route: T) {
        let route_entry = parse_route(path);
        self.routes
            .entry(method.clone())
            .or_default()
            .insert(route_entry, Arc::new(route));
    }

    fn remove_route(&mut self, method: &HttpMethod, path: &str) -> Option<Arc<T>> {
        self.routes
            .get_mut(method)
            .and_then(|m| m.remove(&parse_route(path)))
    }

    fn match_route<'a, 'r>(
        &'a self,
        method: &HttpMethod,
        uri: &'r str,
    ) -> Option<Match<'a, 'r, Self::Route>> {
        let uri_parts = split_uri_into_parts(uri);
        let routes = self.routes.get(method)?;

        if uri_parts.len() == 1 && uri_parts[0] == "*" {
            return routes.get(&RouteEntry::AsteriskForm).map(|route| Match {
                route,
                params: HashMap::new(),
            });
        };

        #[allow(clippy::type_complexity)]
        let mut matched: Vec<(
            u16,                 // lml
            Precedence,          // precedence_tag
            &Vec<RouteSegment>,  // &pattern_segments
            &Arc<T>,             // &route
            HashMap<&str, &str>, // params
        )> = Vec::new();

        let mut max_lml = 0u16;
        for (pattern, route) in routes.iter() {
            let pattern = match pattern {
                RouteEntry::Standard(p) => p,
                RouteEntry::AsteriskForm => continue,
            };

            let mut params = HashMap::new();
            let n = usize::max(uri_parts.len(), pattern.len());
            let mut ok = true; // whether the route is matching
            let mut lml = 0u16; // longest matching literal
            let mut counting_prefix = true; // whether we are still in prefix (all literals so far)

            for i in 0..n {
                let uri_part = uri_parts.get(i);
                let seg_part = pattern.get(i);

                match seg_part {
                    Some(RouteSegment::DoubleWildcard) => {
                        break;
                    }
                    Some(RouteSegment::Wildcard) => {
                        if uri_part.is_none() {
                            ok = false;
                            break;
                        }
                        counting_prefix = false;
                    }
                    Some(RouteSegment::Param(name)) => {
                        if let Some(v) = uri_part {
                            params.insert(name.as_str(), *v);
                        } else {
                            ok = false;
                            break;
                        }
                        counting_prefix = false;
                    }
                    Some(RouteSegment::Literal(lit)) => {
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
                    None => {
                        ok = false;
                        break;
                    }
                }
            }

            if ok {
                max_lml = max(max_lml, lml);
                let prec = precedence_of(pattern.last());
                matched.push((lml, prec, pattern, route, params));
            }
        }

        // Filter by longest literal-match length
        matched.retain(|(l, _, _, _, _)| *l == max_lml);
        if matched.is_empty() {
            return None;
        }

        // Pick best by precedence order
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
    pub route: &'a Arc<T>,
    pub params: HashMap<&'a str, &'r str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RouteEntry {
    AsteriskForm,
    Standard(Vec<RouteSegment>),
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

pub fn parse_route(route_str: &str) -> RouteEntry {
    if route_str == "*" {
        RouteEntry::AsteriskForm
    } else {
        let segments = route_str
            .split('/')
            .filter(|x| !x.is_empty())
            .map(parse_route_segment)
            .collect();
        RouteEntry::Standard(segments)
    }
}

fn split_uri_into_parts(uri: &str) -> Vec<&str> {
    let trimmed = uri.split('?').next().unwrap_or(uri);
    trimmed.split('/').filter(|x| !x.is_empty()).collect()
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
