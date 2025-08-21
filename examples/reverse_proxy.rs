use khttp::{Client, Headers, RequestContext, ResponseHandle, Server, Status};

fn main() {
    let mut args = std::env::args().skip(1); // skip program name
    let host = args
        .next()
        .unwrap_or_else(|| exit("missing argument: host"));
    let port = args
        .next()
        .unwrap_or_else(|| exit("missing argument: port"))
        .parse::<u16>()
        .unwrap_or_else(|_| exit("invalid port"));

    // route all paths and methods to proxy handler
    let mut app = Server::builder("127.0.0.1:9000").unwrap();
    app.fallback_route(move |c, r| proxy(&host, port, c, r));
    app.build().serve().unwrap();
}

fn proxy(
    host: &str,
    port: u16,
    ctx: RequestContext,
    res: &mut ResponseHandle,
) -> std::io::Result<()> {
    let (method, uri, mut headers, _, _, _, request_body) = ctx.into_parts();
    headers.replace("Host", host.as_bytes());

    let mut client = Client::new(format!("{}:{}", host, port));
    let response = match client.exchange(&method, uri.path_and_query(), &headers, request_body) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return res.sendr(&Status::SERVICE_UNAVAILABLE, Headers::empty(), &[][..]);
        }
    };

    let (status, headers, response_body) = response.into_parts();
    res.sendr(&status, &headers, response_body)
}

fn exit(message: &str) -> ! {
    eprintln!("ERROR! {}", message);
    eprintln!("usage: reverse_proxy [HOST] [PORT]");
    eprintln!("\nexamples:");
    eprintln!("   reverse_proxy httpbin.org 80");
    eprintln!("   reverse_proxy localhost 8080");
    std::process::exit(1);
}
