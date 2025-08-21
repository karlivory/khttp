use khttp::{Headers, Method::*, PreRoutingAction, Server, Status};

fn main() {
    let mut app = Server::builder("127.0.0.1:8080").unwrap();

    // GET route: hello world!
    app.route(Get, "/", |ctx, res| {
        println!(
            "method={}, uri={}, headers=\n{}",
            ctx.method, ctx.uri, ctx.headers
        );

        let mut headers = Headers::new();
        headers.add("key", b"value");
        headers.add("key", b"value");

        res.ok(&headers, "Hello, World!")
    });

    // POST route: uppercases request body
    app.route(Post, "/uppercase", |mut ctx, res| {
        let body = ctx.body().vec().unwrap_or_default(); // ctx.body() is `Read`
        let response_body = body.to_ascii_uppercase();
        res.ok(Headers::empty(), &response_body)
    });

    // Routing: named parameters and wildcards are supported
    app.route(Get, "/user/:id", |ctx, res| {
        let user_id: u64 = match ctx.params.get("id").unwrap().parse() {
            Ok(id) => id,
            Err(_) => return res.send0(&Status::BAD_REQUEST, Headers::empty()),
        };
        res.ok(Headers::empty(), format!("{}", user_id))
    });
    app.route(Get, "/api/v1/*", |_, r| r.ok(Headers::empty(), "api"));
    app.route(Get, "/static/**", |_, r| r.ok(Headers::empty(), "static"));

    // Fine-tuning
    app.thread_count(20);
    app.fallback_route(|_, r| r.send(&Status::NOT_FOUND, Headers::empty(), "404"));
    app.max_request_head_size(16 * 1024);
    app.pre_routing_hook(|req, res, conn| {
        if conn.index() > 100 {
            let _ = res.send0(&Status::of(429), Headers::close());
            return PreRoutingAction::Drop;
        }
        if req.http_version == 0 {
            let _ = res.send0(&Status::of(505), Headers::close());
            return PreRoutingAction::Drop;
        }
        if matches!(req.method, Custom(_)) {
            let _ = res.send0(&Status::of(405), Headers::close());
            return PreRoutingAction::Drop;
        }
        PreRoutingAction::Proceed
    });

    // `serve_epoll` is available via the "epoll" feature
    app.build().serve_epoll().unwrap();
}
