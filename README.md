# khttp (Karl's HTTP)

**khttp** is a low-level synchronous **HTTP/1.1** micro-framework for Rust, written from scratch with a focus on:

* Keeping things simple
* Low memory footprint
* Low overhead / high performance (see [benchmarks.md](./benchmarks.md))
* Minimal dependencies (`libc`, `memchr`)

## Features

* HTTP/1.1 **server** & optional **client** (`--features client`)
* Router with params & wildcards: `/user/:id`, `/static/**`
* Zero-copy streamed requests/responses
* Hand-rolled zero-copy parsing with SIMD
* Automatic framing headers (`content-length` / `transfer-encoding: chunked`)
* Custom epoll event loop on Linux (`--features epoll`)

## Sample usage (from: [examples/basics.rs](./examples/basics.rs))

```rust
use khttp::{Headers, Method::*, PreRoutingAction, Server, Status};
use std::{fs::File, io::BufReader, path::Path};

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

        res.ok(&headers, "Hello, World!")
    });

    // POST route: uppercases request body
    app.route(Post, "/uppercase", |mut ctx, res| {
        let body = ctx.body().vec().unwrap_or_default(); // ctx.body() is `Read`
        let response_body = body.to_ascii_uppercase();
        res.ok(Headers::empty(), response_body)
    });

    // Routing: named parameters and wildcards are supported
    app.route(Get, "/user/:id", |ctx, res| {
        let user_id: u64 = match ctx.params.get("id").unwrap().parse() {
            Ok(id) => id,
            Err(_) => return res.send0(&Status::BAD_REQUEST, Headers::empty()),
        };
        res.ok(Headers::empty(), format!("{}", user_id))
    });

    // Matches a single path segment, e.g. /api/v1/health
    app.route(Get, "/api/v1/*", |_, res| res.ok(Headers::empty(), "api"));

    // Matches any number of path segments, e.g. /static/assets/main.js
    app.route(Get, "/static/**", |ctx, res| {
        let rel = ctx.uri.path().strip_prefix("/static/").unwrap_or("");
        let path = Path::new("static").join(rel);

        if !path.is_file() {
            return res.send(&Status::FORBIDDEN, Headers::empty(), "forbidden");
        }

        match File::open(&path) {
            Ok(file) => {
                let mut headers = Headers::new();
                headers.add(Headers::CONTENT_TYPE, utils::get_mime(&path));
                res.okr(&headers, BufReader::new(file)) // streamed response
            }
            Err(_) => res.send(&Status::NOT_FOUND, Headers::empty(), "404"),
        }
    });

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

    // `serve_epoll` is also available via the "epoll" feature
    app.build().serve().unwrap();
}
```

See other [examples](./examples) for:

* [`streams.rs`](./examples/streams.rs): mapping streams from request body to response
* [`static_file_server.rs`](./examples/static_file_server.rs): serving static files (a la `python -m http.server`)
* [`reverse_proxy.rs`](./examples/reverse_proxy.rs): simple reverse proxy using server & client
* [`framework.rs`](./examples/framework.rs): middleware and DI by extending ServerBuilder
* [`custom_router.rs`](./examples/custom_router.rs): custom `match`-based hard-coded router

## License

[MIT](LICENSE).
