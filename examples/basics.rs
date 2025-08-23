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

    // `serve_epoll` is available via the "epoll" feature
    app.build().serve_epoll().unwrap();
}

mod utils {
    pub fn get_mime(path: &std::path::Path) -> &'static [u8] {
        let extension = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => return b"text/plain; charset=utf-8",
        };

        match extension {
            "htm" | "html" => "text/html; charset=utf-8",
            "css" => "text/css; charset=utf-8",
            "js" => "application/javascript; charset=utf-8",
            "gif" => "image/gif",
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "pdf" => "application/pdf",
            "svg" => "image/svg+xml",
            "json" => "application/json; charset=utf-8",
            "txt" => "text/plain; charset=utf-8",
            _ => "text/plain; charset=utf-8",
        }
        .as_bytes()
    }
}
