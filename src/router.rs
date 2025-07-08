// src/router.rs
//
// responsibilities:
//  * parsing/storing a collection of Routes with their methods + patterns
//  * provide fn match_route(...) to match method+uri to corresponding Route

use std::{collections::HashMap, sync::Arc};

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
    routes: HashMap<HttpMethod, HashMap<String, Arc<T>>>,
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
            .insert(path.to_string(), Arc::new(route));
    }

    fn remove_route(&mut self, method: &HttpMethod, path: &str) -> Option<Arc<T>> {
        if !self.routes.contains_key(method) {
            return None;
        }
        self.routes.get_mut(method).unwrap().remove(path)
    }

    fn clone(&self) -> Self {
        Self {
            routes: self.routes.clone(),
        }
    }

    fn match_route(&self, method: &HttpMethod, uri: &str) -> Option<&Arc<Self::Route>> {
        let routes = self.routes.get(method)?;
        routes.get(&uri.to_string())
    }
}
