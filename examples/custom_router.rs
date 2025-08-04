#![allow(clippy::borrowed_box)]

use khttp::{
    Headers, HttpRouter, Match,
    Method::{self, *},
    RouteFn, Server, Status,
};
use std::sync::OnceLock;

fn main() {
    Server::builder("127.0.0.1:8080")
        .unwrap()
        .build_with_router(CustomRouter)
        .serve_epoll()
        .unwrap();
}

// ---------------------------------------------------------------------
// custom HttpRouter
// ---------------------------------------------------------------------

struct CustomRouter;

impl HttpRouter for CustomRouter {
    type Route = Box<RouteFn>;

    fn match_route<'a, 'r>(&'a self, method: &Method, path: &'r str) -> Match<'a, 'r, Self::Route> {
        match (method, path) {
            (Get, "/hello") => Match::no_params(hello_world()),
            (Get, x) if x.starts_with("/user/") => Match::no_params(get_user()),
            _ => Match::no_params(not_found()),
        }
    }
}

// ---------------------------------------------------------------------
// routes
// ---------------------------------------------------------------------

fn hello_world() -> &'static Box<RouteFn> {
    static LOCK: OnceLock<Box<RouteFn>> = OnceLock::new();
    LOCK.get_or_init(|| Box::new(|_, res| res.ok(Headers::empty(), &b"Hello, World!"[..])))
}

fn get_user() -> &'static Box<RouteFn> {
    static LOCK: OnceLock<Box<RouteFn>> = OnceLock::new();
    LOCK.get_or_init(|| {
        Box::new(|ctx, res| {
            let user_id = ctx.uri.path().strip_prefix("/user/").unwrap();
            let user_id = match user_id.parse::<u64>() {
                Ok(id) => id,
                Err(_) => {
                    let body = format!("invalid id: {}", user_id);
                    return res.send(&Status::BAD_REQUEST, Headers::empty(), body.as_bytes());
                }
            };
            res.ok(Headers::empty(), format!("user {}\n", user_id).as_bytes())
        })
    })
}

fn not_found() -> &'static Box<RouteFn> {
    static LOCK: OnceLock<Box<RouteFn>> = OnceLock::new();
    LOCK.get_or_init(|| {
        Box::new(|_, res| {
            res.send(
                &Status::NOT_FOUND,
                Headers::empty(),
                &b"404 - not found"[..],
            )
        })
    })
}
