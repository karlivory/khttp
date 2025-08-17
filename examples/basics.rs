use khttp::{Headers, Method::*, PreRoutingAction, Server, Status};

fn main() {
    let mut app = Server::builder("127.0.0.1:8080").unwrap();

    // GET route: hello world!
    app.route(Get, "/", |ctx, res| {
        println!(
            "Request: method={}, query={}, headers=\n{}",
            ctx.method,
            ctx.uri.query().unwrap_or(""),
            ctx.headers
        );

        let mut headers = Headers::new();
        headers.add("key", b"value");
        headers.add(String::from("owned"), b"value".to_vec());

        // Response body is just `impl Read` (here: &[u8]).
        res.ok(&headers, &b"Hello, World!"[..])
    });

    // POST route: uppercases request body
    app.route(Post, "/uppercase", |mut ctx, res| {
        // ctx.body() implements Read
        let body = ctx.body().string().unwrap_or_default();
        let response_body = body.to_ascii_uppercase();
        // "date" and "content-length" headers are automatically added
        res.ok(Headers::empty(), response_body.as_bytes())
    });

    // Routing: named parameter
    app.route(Get, "/user/:id", |ctx, res| {
        let user_id: u64 = match ctx.params.get("id").unwrap().parse() {
            Ok(id) => id,
            Err(_) => return res.send(&Status::BAD_REQUEST, Headers::empty(), &[][..]),
        };
        res.ok(Headers::empty(), format!("{}", user_id).as_bytes())
    });
    // Routing: wildcard (matches one segment)
    app.route(Get, "/api/v1/*", |_, r| r.ok(Headers::empty(), &[][..]));
    // Routing: double-wildcard (matches any number of segments)
    app.route(Get, "/static/**", |_, r| r.ok(Headers::empty(), &[][..]));

    // Fallback_route
    app.fallback_route(|_, r| r.send(&Status::NOT_FOUND, Headers::empty(), &b"404"[..]));

    // Fine-tuning
    app.thread_count(20);
    app.max_request_head_size(16 * 1024);
    app.pre_routing_hook(|req, res, conn| {
        if conn.index() > 100 {
            let _ = res.send(&Status::of(429), Headers::close(), std::io::empty());
            return PreRoutingAction::Drop;
        }
        if req.http_version == 0 {
            let _ = res.send(&Status::of(505), Headers::close(), std::io::empty());
            return PreRoutingAction::Drop;
        }
        if matches!(req.method, Custom(_)) {
            let _ = res.send(&Status::of(405), Headers::close(), std::io::empty());
            return PreRoutingAction::Drop;
        }
        PreRoutingAction::Proceed
    });

    // `serve_epoll` is enabled via "epoll" feature
    app.build().serve_epoll().unwrap();
}
