// src/router.rs
//
// responsibilities:
//  * parsing/storing a collection of route functions (route_fn) with their methods + patterns
//  * provide .match_route(...) fn to match method+uri to corresponding route_fn

use std::{collections::HashMap, sync::Arc};

use crate::common::{HttpHeaders, HttpMethod, HttpRequest, HttpResponse, HttpStatus};

pub trait AppRouter {
    fn new() -> Self;
    fn handle(&self, request: HttpRequest) -> HttpResponse;
    fn get_http_parsing_error_response(&self) -> &HttpResponse;
    fn clone(&self) -> Self;
}

// below is just a layman's little router implementation ;^)

pub struct DefaultRouter {
    routes: HashMap<HttpMethod, Vec<Arc<AppRoute>>>,
    fallback_handler: Arc<Box<RouteFn>>,
    http_parsing_error_response: HttpResponse,
}

pub struct AppRoute {
    path: String, // TODO: this will be a Vec<RouteSegment> or something like that
    handler: Box<RouteFn>,
}
pub type RouteFn = dyn Fn(HttpRequest) -> HttpResponse + Send + Sync + 'static;

impl DefaultRouter {
    pub fn map_route<F>(&mut self, method: HttpMethod, path: &str, route_fn: F)
    where
        F: Fn(HttpRequest) -> HttpResponse + Send + Sync + 'static,
    {
        if !self.routes.contains_key(&method) {
            self.routes.insert(method.clone(), Vec::new());
        }
        self.routes
            .get_mut(&method)
            .unwrap()
            .push(Arc::new(AppRoute {
                path: path.to_string(),
                handler: Box::new(route_fn),
            }));
    }

    pub fn unmap_route(&mut self, method: HttpMethod, path: &str) -> Option<Arc<AppRoute>> {
        if !self.routes.contains_key(&method) {
            return None;
        }

        // TODO: is there a more idiomatic way of doing this?
        for (i, route) in self.routes.get(&method).unwrap().iter().enumerate() {
            if route.path == path {
                return Some(self.routes.get_mut(&method).unwrap().remove(i));
            }
        }
        None
    }

    pub fn map_fallback_handler<F>(&mut self, route_fn: F)
    where
        F: Fn(HttpRequest) -> HttpResponse + Send + Sync + 'static,
    {
        self.fallback_handler = Arc::new(Box::new(route_fn));
    }
}

impl AppRouter for DefaultRouter {
    fn new() -> Self {
        Self {
            routes: Default::default(),
            fallback_handler: Arc::new(Box::new(default_404_handler)),
            http_parsing_error_response: default_http_parsing_error_response(),
        }
    }

    fn handle(&self, request: HttpRequest) -> HttpResponse {
        let routes = self.routes.get(&request.method);

        let handler = match routes {
            Some(r) => r
                .iter()
                .find(|r| r.path == request.uri)
                .map(|x| &x.handler)
                .unwrap_or(&self.fallback_handler),
            None => &self.fallback_handler,
        };
        let mut response = (handler)(request);

        // TODO: maybe not the best place for this?
        if let Some(ref body) = response.body {
            response.headers.set_content_length(body.len());
        }
        response
    }

    fn get_http_parsing_error_response(&self) -> &HttpResponse {
        &self.http_parsing_error_response
    }

    fn clone(&self) -> Self {
        Self {
            routes: self.routes.clone(),
            fallback_handler: self.fallback_handler.clone(),
            http_parsing_error_response: self.http_parsing_error_response.clone(),
        }
    }
}

fn default_404_handler(_request: HttpRequest) -> HttpResponse {
    HttpResponse {
        status: HttpStatus::of(404),
        headers: HttpHeaders::new(),
        body: None,
    }
}

fn default_http_parsing_error_response() -> HttpResponse {
    HttpResponse {
        body: None,
        headers: Default::default(),
        status: HttpStatus::of(500),
    }
}
