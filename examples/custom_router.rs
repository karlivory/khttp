use khttp::{Headers, Method::*, RequestContext, ResponseHandle, Server, Status};
use std::io;

fn main() -> io::Result<()> {
    let mut app = Server::builder("127.0.0.1:8080").unwrap();
    app.fallback_route(router);
    app.build().serve_epoll()
}

// ---------------------------------------------------------------------
// single route entrypoint
// ---------------------------------------------------------------------

fn router(ctx: RequestContext, res: &mut ResponseHandle) -> io::Result<()> {
    let handler_fn = match (&ctx.method, ctx.uri.path()) {
        (Get, "/hello") => hello_world,
        (Get, x) if x.starts_with("/user/") => get_user,
        _ => not_found,
    };

    handler_fn(ctx, res)
}

// ---------------------------------------------------------------------
// route handlers
// ---------------------------------------------------------------------

fn hello_world(_ctx: RequestContext, res: &mut ResponseHandle) -> io::Result<()> {
    res.ok(Headers::empty(), b"Hello, World!")
}

fn get_user(ctx: RequestContext, res: &mut ResponseHandle) -> io::Result<()> {
    let user_id = ctx.uri.path().strip_prefix("/user/").unwrap();
    let user_id = match user_id.parse::<u64>() {
        Ok(id) => id,
        Err(_) => {
            let body = format!("invalid id: {}", user_id);
            return res.send(&Status::BAD_REQUEST, Headers::empty(), body.as_bytes());
        }
    };
    res.ok(Headers::empty(), format!("user {}\n", user_id).as_bytes())
}

fn not_found(_ctx: RequestContext, res: &mut ResponseHandle) -> io::Result<()> {
    res.send(&Status::NOT_FOUND, Headers::empty(), b"not found")
}
