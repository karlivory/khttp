# khttp (Karl's HTTP)

**khttp** is a low-level synchronous **HTTP/1.1** micro-framework for Rust, written from scratch with a focus on:

* Keeping things simple
* Low memory footprint
* Low overhead / high performance (see [benchmarks.md](./benchmarks.md))
* Minimal dependencies (`libc`, `memchr`)

## Features

* HTTP/1.1 **server** and **client** (`--features client`)
* Router with params and wildcards: `/user/:id`, `/static/**`
* Zero-copy, streamed requests/responses
* Hand-rolled zero-copy parsing with SIMD
* Automatic framing headers (`content-length` / `transfer-encoding: chunked`)
* Custom epoll event loop on Linux (`--features epoll`)
* Pluggable TCP connection lifecycle hooks

## Sample usage (from: [examples/basics.rs](./examples/basics.rs))

```rust
use khttp::{ConnectionSetupAction, Headers, Method::*, PreRoutingAction, Server, Status};
use std::{fs::File, io::BufReader, path::Path, time::Duration};

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

    // Lifecycle hook: called after a new TCP connection is accepted
    app.connection_setup_hook(|connection| match connection {
        Ok((tcp_stream, _peer)) => {
            let _ = tcp_stream.set_nodelay(true);
            let _ = tcp_stream.set_read_timeout(Some(Duration::from_secs(3)));
            ConnectionSetupAction::Proceed(tcp_stream)
        }
        Err(_) => ConnectionSetupAction::Drop,
    });

    // Lifecycle hook: called after a request is parsed, right before routing
    app.pre_routing_hook(|req, res| {
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

    // Lifecycle hook: called right before the TCP connection is dropped
    app.connection_teardown_hook(move |_stream, io_result| {
        if let Some(e) = io_result.err() {
            eprintln!("tcp socket error: {e}");
        }
    });

    // `serve_threaded` and `serve_epoll` are also available
    app.build().serve().unwrap();
}
```

See other [examples](./examples) for:

* [`streams.rs`](./examples/streams.rs): mapping streams from request body to response
* [`static_file_server.rs`](./examples/static_file_server.rs): serving static files (like `python -m http.server`)
* [`reverse_proxy.rs`](./examples/reverse_proxy.rs): simple reverse proxy using server and client
* [`framework.rs`](./examples/framework.rs): middleware and DI by extending ServerBuilder
* [`custom_router.rs`](./examples/custom_router.rs): custom `match`-based hard-coded router
* [`lifecycle_hooks.rs`](./examples/lifecycle_hooks.rs): tracking TCP connections and peers via lifecycle hooks

## License

[MIT](LICENSE).
