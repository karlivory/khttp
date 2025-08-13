# khttp (Karl's HTTP)

**khttp** is a low-level, synchronous **HTTP/1.1** micro-framework for Rust - written from scratch with a focus on:

* Keeping things simple
* Low memory footprint
* Low overhead / high performance
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

    // route handlers are: dyn Fn(RequestContext, &mut ResponseHandle) -> io::Result<()>
    app.route(Get, "/", |ctx, res| {
        // Access the request via RequestContext
        println!(
            "Received request.\nmethod: {}\nquery: {}\nheaders:\n{}",
            ctx.method,
            ctx.uri.query().unwrap_or(""),
            ctx.headers
        );

        let mut headers = Headers::new();
        // Header names/values are Cow str and [u8] respectively,
        // so they support both owned values and references
        headers.add("key", b"value");
        headers.add(String::from("owned"), b"value".to_vec());

        // Send the response via ResponseHandle.
        // Response body is of type `impl Read` (here: response is sent as &[u8]).
        res.ok(&headers, &b"Hello, World!"[..])
    });

    app.route(Post, "/uppercase", |mut ctx, res| {
        // ctx.body() implements Read but it also has convenience methods ::string() and ::vec()
        let body = ctx.body().string().unwrap_or_default();
        let response_body = body.to_ascii_uppercase();
        // "date" and "content-length" headers are added automatically
        res.ok(Headers::empty(), response_body.as_bytes())
    });

    // Routing supports named parameters
    app.route(Get, "/user/:id", |ctx, res| {
        let user_id: u64 = match ctx.params.get("id").unwrap().parse() {
            Ok(id) => id,
            Err(_) => {
                return res.send(&Status::BAD_REQUEST, Headers::empty(), &b"invalid id"[..]);
            }
        };
        res.ok(Headers::empty(), format!("{}", user_id).as_bytes())
    });

    // All other requests are handled via the fallback_route
    app.fallback_route(|_, res| res.send(&Status::NOT_FOUND, Headers::empty(), &b"not found"[..]));

    // Fine-tuning
    app.thread_count(20);
    app.max_request_head_size(16 * 1024);
    // Pre-routing-hooks runs after request-head is parsed, right before routing
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
