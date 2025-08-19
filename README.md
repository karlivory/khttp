# khttp (Karl's HTTP)

**khttp** is a low-level, synchronous **HTTP/1.1** micro-framework for Rust - written from scratch with a focus on:

* Keeping things simple
* Low memory footprint
* Low overhead / high performance (see: [benchmarks.md](./benchmarks.md))
* Minimal dependencies (`libc`, `memchr`)

## Features:

* HTTP/1.1 **server** & optional **client** (`--features client`)
* Zero-copy(-ish) parsing with SIMD
* Router with params & wildcards: `/user/:id`, `/static/**`
* Automatic framing: `Content-Length` & `Transfer-Encoding: chunked`
* Automatic `date` header
* Custom epoll event loop on Linux (`--features epoll`)

## Sample usage (from: [examples/basics.rs](./examples/basics.rs))

```rust
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

        // Response body is just `impl Read` (here: &[u8]).
        res.ok(&headers, &b"Hello, World!"[..])
    });

    // POST route: uppercases request body
    app.route(Post, "/uppercase", |mut ctx, res| {
        let body = ctx.body().string().unwrap_or_default(); // ctx.body() is `impl Read`
        let response_body = body.to_ascii_uppercase();
        res.ok(Headers::empty(), response_body.as_bytes())
    });

    // Routing: named parameters and wildcards are supported
    app.route(Get, "/user/:id", |ctx, res| {
        let user_id: u64 = match ctx.params.get("id").unwrap().parse() {
            Ok(id) => id,
            Err(_) => return res.send(&Status::BAD_REQUEST, Headers::empty(), &[][..]),
        };
        res.ok(Headers::empty(), format!("{}", user_id).as_bytes())
    });
    app.route(Get, "/api/v1/*", |_, r| r.ok(Headers::empty(), &[][..]));
    app.route(Get, "/static/**", |_, r| r.ok(Headers::empty(), &[][..]));

    // Fine-tuning
    app.thread_count(20);
    app.fallback_route(|_, r| r.send(&Status::NOT_FOUND, Headers::empty(), &b"404"[..]));
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

    // `serve_epoll` is available via the "epoll" feature
    app.build().serve_epoll().unwrap();
}
```

See other [examples](./examples) for:

* [`custom_router.rs`](./examples/custom_router.rs): custom `match`-based hard-coded router,
* [`streams.rs`](./examples/streams.rs): mapping streams from request body to response,
* [`static_file_server.rs`](./examples/static_file_server.rs): serving static files (a la `python -m http.server`),
* [`reverse_proxy.rs`](./examples/reverse_proxy.rs): simple reverse proxy using server & client,
* [`framework.rs`](./examples/framework.rs): middleware and DI by extending ServerBuilder.

## License

[MIT](LICENSE).
