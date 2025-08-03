use khttp::{Client, Headers, RequestContext, ResponseHandle, Server, Status};

// hard-coded upstream
static UPSTREAM: &str = "httpbin.org";

fn main() {
    let mut app = Server::builder("127.0.0.1:8080").unwrap();

    // route all paths and methods to upstream
    app.fallback_route(proxy_handler);
    app.build().serve().unwrap();
}

fn proxy_handler(mut ctx: RequestContext, res: &mut ResponseHandle) -> std::io::Result<()> {
    let mut headers = ctx.headers.clone();
    headers.set("Host", UPSTREAM.as_bytes());

    let mut client = Client::new(&format!("{}:80", UPSTREAM));
    let mut response =
        match client.exchange(&ctx.method.clone(), ctx.uri.path(), &headers, ctx.body()) {
            Ok(r) => r,
            Err(e) => {
                dbg!(e);
                return res.send(&Status::SERVICE_UNAVAILABLE, Headers::empty(), &[][..]);
            }
        };

    res.send(
        &response.status.clone(),
        &response.headers.clone(),
        response.body(),
    )
}
