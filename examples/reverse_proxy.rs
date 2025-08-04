use khttp::{Client, Headers, RequestContext, ResponseHandle, Server, Status};

// hard-coded upstream
static UPSTREAM: &str = "httpbin.org";
static UPSTREAM_PORT: usize = 80;

fn main() {
    let mut app = Server::builder("127.0.0.1:9000").unwrap();

    // route all paths and methods to proxy handler
    app.fallback_route(proxy_handler);
    app.build().serve().unwrap();
}

fn proxy_handler(ctx: RequestContext, res: &mut ResponseHandle) -> std::io::Result<()> {
    let (method, uri, mut headers, _, _, _, request_body) = ctx.into_parts();
    headers.set("Host", UPSTREAM.as_bytes());

    let mut client = Client::new(&format!("{}:{}", UPSTREAM, UPSTREAM_PORT));
    let response = match client.exchange(&method, uri.path(), &headers, request_body) {
        Ok(r) => r,
        Err(e) => {
            dbg!(e);
            return res.send(&Status::SERVICE_UNAVAILABLE, Headers::empty(), &[][..]);
        }
    };

    let (status, headers, response_body) = response.into_parts();
    res.send(&status, &headers, response_body)
}
