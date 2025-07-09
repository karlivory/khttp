// src/router.rs
//
// responsibilities:
//  * parsing/storing a collection of Routes with their methods + patterns
//  * provide fn match_route(...) to match method+uri to corresponding Route

use std::{collections::HashMap, hash::Hash, sync::Arc};

use crate::common::{HttpMethod, HttpRequest, HttpResponse};

pub type RouteFn = dyn Fn(HttpRequest) -> HttpResponse + Send + Sync + 'static;

pub trait AppRouter {
    type Route;

    fn new() -> Self;
    fn match_route(&self, method: &HttpMethod, path: &str) -> Option<&Arc<Self::Route>>;
    fn add_route(&mut self, method: &HttpMethod, path: &str, route: Self::Route);
    fn remove_route(&mut self, method: &HttpMethod, path: &str) -> Option<Arc<Self::Route>>;
    fn clone(&self) -> Self;
}

pub struct DefaultRouter<T> {
    routes: HashMap<HttpMethod, HashMap<Vec<RouteSegment>, Arc<T>>>,
}

// #[allow(clippy::derived_hash_with_manual_eq)]
#[derive(Debug, Clone, Eq)]
pub enum RouteSegment {
    Literal(String),
    Wildcard,
    DoubleWildcard,
}

impl Hash for RouteSegment {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
    }
}

impl PartialEq for RouteSegment {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Literal(l0), Self::Literal(r0)) => l0 == r0,
            (Self::Wildcard, Self::Wildcard) => true,
            (Self::DoubleWildcard, Self::DoubleWildcard) => true,
            _ => false,
        }
    }
}

impl<T> AppRouter for DefaultRouter<T> {
    type Route = T;

    fn new() -> Self {
        Self {
            routes: Default::default(),
        }
    }

    fn add_route(&mut self, method: &HttpMethod, path: &str, route: T) {
        if !self.routes.contains_key(method) {
            self.routes.insert(method.clone(), HashMap::new());
        }
        self.routes
            .get_mut(method)
            .unwrap()
            .insert(parse_route(path), Arc::new(route));
    }

    fn remove_route(&mut self, method: &HttpMethod, path: &str) -> Option<Arc<T>> {
        if !self.routes.contains_key(method) {
            return None;
        }
        self.routes
            .get_mut(method)
            .unwrap()
            .remove(&parse_route(path))
    }

    fn clone(&self) -> Self {
        Self {
            routes: self.routes.clone(),
        }
    }

    fn match_route(&self, method: &HttpMethod, uri: &str) -> Option<&Arc<Self::Route>> {
        let uri_parts = split_uri_into_parts(uri);
        let routes = self.routes.get(method).unwrap();

        let mut matched_routes: Vec<(&Vec<RouteSegment>, &Arc<Self::Route>)> = Vec::new();
        for (path, route) in routes.iter() {
            let mut matching = true;
            let n = usize::max(uri_parts.len(), path.len());
            for i in 0..n {
                let uri_part = uri_parts.get(i);
                let segment_part = path.get(i);

                if segment_part == Some(&RouteSegment::DoubleWildcard) {
                    break; // double-wildcard matches until the end of the route
                }

                if segment_part == Some(&RouteSegment::Wildcard) && uri_part.is_some() {
                    continue; // wildcard matches a single route segment
                }

                // route length mismatch
                if segment_part.is_none() || uri_part.is_none() {
                    matching = false;
                    break;
                }

                if let Some(RouteSegment::Literal(x)) = segment_part {
                    if x.as_str() == *uri_part.unwrap() {
                        continue;
                    }
                }

                matching = false;
                break;
            }
            if matching {
                matched_routes.push((path, route));
            }
        }

        match matched_routes.len() {
            0 => None,
            1 => Some(matched_routes[0].1),
            _ => Some(get_route_with_precedence(matched_routes)),
        }
    }
}

// if request uri is /route/<...> and there are multiple matched routes, e.g.:
//  * /route/foo (literal)
//  * /route/*   (wildcard)
//  * /route/**  (double-wildcard)
//
// then precedence goes in the order literal > wildcard > double-wildcard
fn get_route_with_precedence<'a, T>(matched_routes: Vec<(&'a Vec<RouteSegment>, &'a T)>) -> &'a T {
    for (path, route) in matched_routes.iter() {
        if let Some(RouteSegment::Literal(_)) = path.last() {
            return route;
        }
    }
    for (path, route) in matched_routes.iter() {
        if let Some(RouteSegment::Wildcard) = path.last() {
            return route;
        }
    }
    for (path, route) in matched_routes.iter() {
        if let Some(RouteSegment::DoubleWildcard) = path.last() {
            return route;
        }
    }
    unreachable!();
}

fn split_uri_into_parts(uri: &str) -> Vec<&str> {
    let mut uri = uri;
    if uri.contains("?") {
        // can this be simplified??
        uri = uri.split("?").take(1).collect::<Vec<&str>>()[0];
    }
    uri.split("/").filter(|x| !x.is_empty()).collect()
}

fn parse_route_segment(s: &str) -> RouteSegment {
    match s {
        "*" => RouteSegment::Wildcard,
        "**" => RouteSegment::DoubleWildcard,
        x => RouteSegment::Literal(x.to_string()),
    }
}

pub fn parse_route(route_str: &str) -> Vec<RouteSegment> {
    route_str
        .split("/")
        .filter(|x| !x.is_empty())
        .map(parse_route_segment)
        .collect()
}
