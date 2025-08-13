//! Bench harness for khttp.
//! Server benches -> `rewrk`
//! Parser benches -> Criterion
//!
//! Run from repo root:
//!   cargo bench --manifest-path benchmarks/Cargo.toml -- server:minimal
//!
//! Filters allowed: `server`, `server:*`, `server:name`, `parser`, `parser:*`, `parser:name`
//!
//! (inspired by axum's benches.rs)

use criterion::{BenchmarkId, Criterion};
use khttp::{Headers, Method, ResponseHandle, RouteFn, Router, Server, ServerBuilder, Status};
use std::{
    io::{self, BufRead, BufReader},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

struct ServerBench {
    full: &'static str,
    path: &'static str,
    kind: ServerKind,
}

enum ServerKind {
    Khttp(Box<dyn FnOnce() -> KhttpServer>),
    Axum(Box<dyn FnOnce(axum::Router) -> axum::Router>),
}

type KhttpServer = Server<Router<Box<RouteFn>>>;
type ParserBenchFn = dyn Fn(&mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>);

struct ParserBench {
    full: &'static str,
    bench: Box<ParserBenchFn>,
}

fn main() {
    let mut server_benches: Vec<ServerBench> = Vec::new();
    let mut parser_benches: Vec<ParserBench> = Vec::new();

    // khttp server benches
    server_benches.push(ServerBench {
        full: "server:minimal",
        path: "/",
        kind: ServerKind::Khttp(Box::new(|| {
            let mut app = get_khttp_app();
            app.route(Method::Get, "/", |_c, r| respond_hello(r));
            app.build()
        })),
    });

    server_benches.push(ServerBench {
        full: "server:longheader",
        path: "/",
        kind: ServerKind::Khttp(Box::new(|| {
            let mut app = get_khttp_app();
            app.route(Method::Get, "/", |_c, r| respond_longheader(r));
            app.build()
        })),
    });

    server_benches.push(ServerBench {
        full: "server:medium",
        path: "/",
        kind: ServerKind::Khttp(Box::new(|| {
            let mut app = get_khttp_app();
            app.route(Method::Get, "/", |_c, r| respond_medium(r));
            app.build()
        })),
    });

    server_benches.push(ServerBench {
        full: "server:heavy",
        path: "/",
        kind: ServerKind::Khttp(Box::new(|| {
            let mut app = get_khttp_app();
            app.route(Method::Get, "/", |_c, r| respond_heavy(r));
            app.build()
        })),
    });

    server_benches.push(ServerBench {
        full: "server:routing",
        path: "/foo/bar/baz",
        kind: ServerKind::Khttp(Box::new(|| {
            let mut app = get_khttp_app();
            for a in 0..10 {
                for b in 0..10 {
                    for c in 0..10 {
                        let p = format!("/foo-{a}/bar-{b}/baz-{c}");
                        app.route(Method::Get, &p, |_c, r| r.ok(&Headers::new(), &[][..]));
                    }
                }
            }
            app.route(Method::Get, "/foo/bar/baz", |_c, r| {
                r.ok(&Headers::new(), &[][..])
            });
            app.build()
        })),
    });

    // axum comparisons
    server_benches.push(ServerBench {
        full: "axum:minimal",
        path: "/",
        kind: ServerKind::Axum(Box::new(|router| {
            use axum::routing::get;
            router.route("/", get(|| async { "Hello, World!" }))
        })),
    });

    server_benches.push(ServerBench {
        full: "axum:longheader",
        path: "/",
        kind: ServerKind::Axum(Box::new(|router| {
            use axum::{http::HeaderMap, routing::get};

            router.route(
                "/",
                get(|| async {
                    let mut headers = HeaderMap::new();
                    for i in 0..50 {
                        let name = format!("value{}", i);
                        let value = "hello".repeat(10);
                        headers.insert(
                            name.parse::<axum::http::HeaderName>().unwrap(),
                            value.parse().unwrap(),
                        );
                    }
                    (headers, "hey")
                }),
            )
        })),
    });

    server_benches.push(ServerBench {
        full: "axum:medium",
        path: "/",
        kind: ServerKind::Axum(Box::new(|router| {
            use axum::routing::get;
            router.route("/", get(|| async { medium_body() }))
        })),
    });

    server_benches.push(ServerBench {
        full: "axum:heavy",
        path: "/",
        kind: ServerKind::Axum(Box::new(|router| {
            use axum::routing::get;
            router.route("/", get(|| async { heavy_body() }))
        })),
    });

    server_benches.push(ServerBench {
        full: "axum:routing",
        path: "/foo/bar/baz",
        kind: ServerKind::Axum(Box::new(|mut router| {
            use axum::routing::get;
            for a in 0..10 {
                for b in 0..10 {
                    for c in 0..10 {
                        let p = format!("/foo-{a}/bar-{b}/baz-{c}");
                        router = router.route(&p, get(|| async { "" }));
                    }
                }
            }
            router.route("/foo/bar/baz", get(|| async { "" }))
        })),
    });

    server_benches.push(ServerBench {
        full: "server:chunked",
        path: "/chunked",
        kind: ServerKind::Khttp(Box::new(|| {
            let mut app = get_khttp_app();
            app.route(Method::Get, "/chunked", |_c, r| respond_chunked(r));
            app.build()
        })),
    });

    // axum: chunked
    server_benches.push(ServerBench {
        full: "axum:chunked",
        path: "/chunked",
        kind: ServerKind::Axum(Box::new(|router| {
            use axum::body::Body;
            use axum::routing::get;
            use bytes::Bytes;
            use futures_util::stream;

            router.route(
                "/chunked",
                get(|| async {
                    // TODO: is this correct?
                    let msg = medium_body();
                    let stream = stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(msg))]);
                    Body::from_stream(stream)
                }),
            )
        })),
    });

    parser_benches.push(ParserBench {
        full: "parser:response",
        bench: Box::new(|group| {
            let raw = b"HTTP/1.1 200 OK\r\nHost: example.com\r\n\r\n";
            group.bench_function(BenchmarkId::new("parser:response", "GET /foo/bar"), |b| {
                b.iter(|| {
                    khttp::Response::parse(std::hint::black_box(&raw[..])).unwrap();
                });
            });
        }),
    });

    parser_benches.push(ParserBench {
        full: "httparse:response",
        bench: Box::new(|group| {
            let raw = b"HTTP/1.1 200 OK\r\nHost: example.com\r\n\r\n";
            group.bench_function(BenchmarkId::new("httparse:response", "GET /foo/bar"), |b| {
                b.iter(|| {
                    let mut headers = [httparse::EMPTY_HEADER; 25];
                    let mut resp = httparse::Response::new(&mut headers);
                    let _ = resp.parse(std::hint::black_box(&raw[..])).unwrap();

                    httparse_alloc_response(resp);
                });
            });
        }),
    });

    parser_benches.push(ParserBench {
        full: "parser:simple",
        bench: Box::new(|group| {
            let raw = b"GET /foo/bar HTTP/1.1\r\nHost: example.com\r\n\r\n";
            group.bench_function(BenchmarkId::new("parser:simple", "GET /foo/bar"), |b| {
                b.iter(|| {
                    khttp::Request::parse(std::hint::black_box(&raw[..])).unwrap();
                });
            });
        }),
    });

    parser_benches.push(ParserBench {
        full: "httparse:simple",
        bench: Box::new(|group| {
            let raw = b"GET /foo/bar HTTP/1.1\r\nHost: example.com\r\n\r\n";
            group.bench_function(BenchmarkId::new("httparse:simple", "GET /foo/bar"), |b| {
                b.iter(|| {
                    let mut headers = [httparse::EMPTY_HEADER; 25];
                    let mut req = httparse::Request::new(&mut headers);
                    let _ = req.parse(std::hint::black_box(&raw[..])).unwrap();

                    // (make the allocations, to keep the parser comparison apples-to-apples)
                    httparse_alloc_request(req);
                });
            });
        }),
    });

    parser_benches.push(ParserBench {
        full: "parser:long",
        bench: Box::new(|group| {
            let raw = make_long_request();
            group.bench_function(BenchmarkId::new("long", "GET /long"), |b| {
                b.iter(|| {
                    khttp::Request::parse(std::hint::black_box(&raw[..])).unwrap();
                });
            });
        }),
    });

    parser_benches.push(ParserBench {
        full: "httparse:long",
        bench: Box::new(|group| {
            let raw = make_long_request();
            group.bench_function(BenchmarkId::new("long", "GET /long"), |b| {
                b.iter(|| {
                    let mut headers = [httparse::EMPTY_HEADER; 25];
                    let mut req = httparse::Request::new(&mut headers);
                    let _ = req.parse(std::hint::black_box(&raw[..])).unwrap();
                    assert!(req.path.is_some());
                    assert!(req.version.is_some());

                    // (make the allocations, to keep the parser comparison apples-to-apples)
                    httparse_alloc_request(req);
                });
            });
        }),
    });

    parser_benches.push(ParserBench {
        full: "date:uncached",
        bench: Box::new(|group| {
            group.bench_function(BenchmarkId::new("long", "GET /long"), |b| {
                b.iter(|| {
                    let d = khttp::date::get_date_now_uncached();
                    std::hint::black_box(d); // prevent optimization
                });
            });
        }),
    });

    parser_benches.push(ParserBench {
        full: "date:httpdate",
        bench: Box::new(|group| {
            group.bench_function(BenchmarkId::new("long", "GET /long"), |b| {
                b.iter(|| {
                    let s = httpdate::fmt_http_date(std::time::SystemTime::now());
                    std::hint::black_box(&s);
                });
            });
        }),
    });

    // -------- filter & run --------
    let filters = user_filters();
    let available: Vec<&str> = server_benches
        .iter()
        .map(|b| b.full)
        .chain(parser_benches.iter().map(|b| b.full))
        .collect();

    let mut ran_any = false;

    // server benches
    if !server_benches.is_empty() {
        ensure_rewrk();
        for sb in server_benches {
            let (g, n) = split_group(sb.full);
            if should_run(&filters, g, n) {
                ran_any = true;
                run_server_bench(sb);
            }
        }
    }

    // parser benches (Criterion)
    let selected: Vec<_> = parser_benches
        .into_iter()
        .filter(|pb| {
            let (g, n) = split_group(pb.full);
            should_run(&filters, g, n)
        })
        .collect();

    if !selected.is_empty() {
        ran_any = true;
        let mut crit = Criterion::default();
        let mut group = crit.benchmark_group("parser");
        for pb in selected {
            eprintln!("Running {}", pb.full);
            (pb.bench)(&mut group);
        }
        group.finish();
        crit.final_summary();
    }

    if !ran_any && !filters.is_empty() {
        eprintln!("No benchmarks matched your filter(s): {:?}", filters);
        eprintln!("Available benches:");
        for n in available {
            eprintln!("{n}");
        }
        std::process::exit(1);
    }
}

fn run_server_bench(sb: ServerBench) {
    let connections = 500u32;
    let threads = 14u32;
    let duration_secs = 6;

    let port = match sb.kind {
        ServerKind::Khttp(make_srv) => {
            let srv = make_srv();
            let port = srv.bind_addrs().first().map(|a| a.port()).unwrap();
            thread::spawn(move || srv.serve_epoll());
            port
        }
        ServerKind::Axum(build_router) => {
            use axum::Router;
            use tokio::runtime::Runtime;

            // bind using std to get a free port
            let std_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind axum");
            std_listener.set_nonblocking(true).unwrap();
            let port = std_listener.local_addr().unwrap().port();

            let router = build_router(Router::new());

            thread::spawn(move || {
                let rt = Runtime::new().unwrap();
                rt.block_on(async move {
                    // convert to tokio listener
                    let listener = tokio::net::TcpListener::from_std(std_listener).unwrap();
                    axum::serve(listener, router).await.unwrap();
                });
            });

            port
        }
    };

    // wait for server to be active
    thread::sleep(Duration::from_millis(10));

    let mut cmd = Command::new("rewrk");
    cmd.stdout(Stdio::piped());
    cmd.arg("--host")
        .arg(format!("http://127.0.0.1:{port}{}", sb.path));
    cmd.args(["--connections", &connections.to_string()]);
    cmd.args(["--threads", &threads.to_string()]);
    cmd.args(["--duration", &format!("{}s", duration_secs)]);

    eprintln!("Running {} on port {}", sb.full, port);

    let mut child = cmd.spawn().expect("failed to spawn rewrk");
    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        println!("  {}", line.unwrap());
    }

    let status = child.wait().expect("wait rewrk");
    if !status.success() {
        panic!("rewrk exited with non-zero status");
    }
}

fn split_group(full: &'static str) -> (&'static str, &'static str) {
    if let Some(i) = full.find(':') {
        (&full[..i], &full[i + 1..])
    } else {
        ("", full)
    }
}

fn user_filters() -> Vec<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(pos) = args.iter().position(|a| a == "--") {
        args[pos + 1..].to_vec()
    } else {
        args
    }
}

fn should_run(filters: &[String], group: &str, name: &str) -> bool {
    if filters.is_empty() {
        return true;
    }
    filters.iter().any(|t| match_token(t, group, name))
}

fn match_token(token: &str, group: &str, name: &str) -> bool {
    if let Some(i) = token.find(':') {
        let (tg, tn) = token.split_at(i);
        let tn = &tn[1..];
        tg == group && (tn == "*" || tn == name)
    } else {
        token == group
    }
}

fn get_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("no free port?")
        .local_addr()
        .unwrap()
        .port() // listener is dropped
}

fn get_khttp_app() -> ServerBuilder {
    let port = get_free_port();
    Server::builder(format!("127.0.0.1:{port}")).unwrap()
}

fn get_base_headers() -> Headers<'static> {
    // for fairness: same headers that axum responds with
    let mut headers = Headers::new();
    headers.add(Headers::CONTENT_TYPE, b"text/plain; charset=utf-8");
    headers
}

fn respond_hello(res: &mut ResponseHandle) -> io::Result<()> {
    res.ok(&get_base_headers(), "Hello, World!".as_bytes())
}

fn respond_longheader(res: &mut ResponseHandle) -> io::Result<()> {
    let mut headers = get_base_headers();
    for i in 0..50 {
        let n = format!("value{i}");
        let a = "hello".repeat(10);
        headers.add(n, a.into_bytes());
    }
    res.ok(&headers, "hey".as_bytes())
}

fn medium_body() -> &'static [u8] {
    use std::sync::OnceLock;
    static LOCK: OnceLock<Vec<u8>> = OnceLock::new();
    LOCK.get_or_init(|| b"Hello, World!".repeat(10_000))
        .as_slice()
}

fn heavy_body() -> &'static [u8] {
    use std::sync::OnceLock;
    static LOCK: OnceLock<Vec<u8>> = OnceLock::new();
    LOCK.get_or_init(|| b"Hello, World!".repeat(100_000))
        .as_slice()
}

fn respond_medium(res: &mut ResponseHandle) -> io::Result<()> {
    let msg = medium_body();
    let mut headers = get_base_headers();
    headers.set_content_length(Some(msg.len() as u64));
    res.ok(&headers, msg)
}

fn respond_heavy(res: &mut ResponseHandle) -> io::Result<()> {
    let msg = heavy_body();
    let mut headers = get_base_headers();
    headers.set_content_length(Some(msg.len() as u64));
    res.ok(&headers, msg)
}

fn respond_chunked(res: &mut ResponseHandle) -> io::Result<()> {
    let msg = medium_body();
    let mut headers = get_base_headers();
    headers.set_transfer_encoding_chunked();
    res.send(&Status::of(200), &headers, msg)
}

fn make_long_request() -> Vec<u8> {
    let mut buf = b"GET https://example.com/really/long/path/that/keeps/going/on/and/on?".to_vec();

    // Append ~200 bytes worth of query parameters
    for i in 1..=20 {
        let param = format!("k{}=value{}&", i, i); // e.g., k1=value1&
        buf.extend_from_slice(param.as_bytes());
    }

    // Remove the trailing '&' (optional)
    if let Some(last) = buf.last() {
        if *last == b'&' {
            buf.pop();
        }
    }

    buf.extend_from_slice(
        b" HTTP/1.1\r\n\
Host: example.com\r\n\
User-Agent: bench\r\n\
Accept: */*\r\n",
    );

    for i in 1..=20 {
        let header = format!("X-Custom-Header-{}: value_with_some_length_{}\r\n", i, i);
        buf.extend_from_slice(header.as_bytes());
    }

    buf.extend_from_slice(b"\r\n");
    buf
}

fn ensure_rewrk() {
    let status = Command::new("rewrk")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("failed to run rewrk");
    if !status.success() {
        panic!("rewrk is not installed. See https://github.com/lnx-search/rewrk");
    }
}

fn httparse_alloc_response(resp: httparse::Response) {
    let mut headers = Headers::new();
    for header in resp.headers {
        if header.name.is_empty() {
            break;
        }
        headers.add(header.name, header.value);
    }
    // let status = Status::borrowed(resp.code.unwrap(), resp.reason.unwrap());
}

fn httparse_alloc_request(req: httparse::Request) {
    // let full_uri = req.path.unwrap();
    // let http_version = req.version.unwrap();
    let mut headers = Headers::new();
    for header in req.headers {
        if header.name.is_empty() {
            break;
        }
        headers.add(header.name, header.value);
    }

    // parse method
    // let _method = match req.method.unwrap().as_bytes() {
    //     b"GET" => Method::Get,
    //     b"POST" => Method::Post,
    //     b"HEAD" => Method::Head,
    //     b"PUT" => Method::Put,
    //     b"PATCH" => Method::Patch,
    //     b"DELETE" => Method::Delete,
    //     b"OPTIONS" => Method::Options,
    //     b"TRACE" => Method::Trace,
    //     _ => {
    //         unimplemented!();
    //     }
    // };
}
