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
use khttp::{
    common::{HttpHeaders, HttpMethod, HttpStatus},
    http_parser::HttpRequestParser,
    router::DefaultRouter,
    server::{App, HttpServer, HttpServerBuilder, RouteFn},
};
use std::{
    env,
    io::{BufRead, BufReader},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

// -------- Types --------
type KhttpServer = HttpServer<DefaultRouter<Box<RouteFn>>>;

struct ServerBench {
    full: &'static str,
    path: &'static str,
    kind: ServerKind,
}

enum ServerKind {
    Khttp(Box<dyn FnOnce() -> KhttpServer>),
    Axum(Box<dyn FnOnce(axum::Router) -> axum::Router>),
}

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
            app.map_route(HttpMethod::Get, "/", |_c, r| respond_hello(r));
            app.build()
        })),
    });

    server_benches.push(ServerBench {
        full: "server:heavy",
        path: "/a/b/c",
        kind: ServerKind::Khttp(Box::new(|| {
            let mut app = get_khttp_app();
            app.map_route(HttpMethod::Get, "/a/b/c", |_c, r| respond_heavy(r));
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
                        app.map_route(HttpMethod::Get, &p, |_c, r| {
                            r.ok(HttpHeaders::new(), &[][..]);
                        });
                    }
                }
            }
            app.map_route(HttpMethod::Get, "/foo/bar/baz", |_c, r| {
                r.ok(HttpHeaders::new(), &[][..]);
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
        full: "axum:heavy",
        path: "/a/b/c",
        kind: ServerKind::Axum(Box::new(|router| {
            use axum::routing::get;
            router.route("/a/b/c", get(|| async { heavy_body() }))
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
            app.map_route(HttpMethod::Get, "/chunked", |_c, r| respond_chunked(r));
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
                    let msg = heavy_body();
                    let stream = stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(msg))]);
                    Body::from_stream(stream)
                }),
            )
        })),
    });

    parser_benches.push(ParserBench {
        full: "parser:complex",
        bench: Box::new(|group| {
            let raw = b"GET /foo/bar HTTP/1.1\r\nHost: example.com\r\n\r\n";
            group.bench_function(BenchmarkId::new("complex", "GET /foo/bar"), |b| {
                b.iter(|| {
                    let _ = HttpRequestParser::new(std::hint::black_box(&raw[..]))
                        .parse()
                        .unwrap();
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
    let connections = 10u32;
    let threads = 10u32;
    let duration_secs = if on_ci() { 1 } else { 10 };

    let port = match sb.kind {
        ServerKind::Khttp(make_srv) => {
            let srv = make_srv();
            let port = *srv.port();
            thread::spawn(move || srv.serve());
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
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let p = listener.local_addr().unwrap().port();
    drop(listener);
    p
}

fn get_khttp_app() -> HttpServerBuilder<DefaultRouter<Box<RouteFn>>> {
    App::new("127.0.0.1", get_free_port())
}

fn respond_hello(res: &mut khttp::server::ResponseHandle) {
    let msg = "Hello, World!";
    res.ok(HttpHeaders::new(), msg.as_bytes());
}

use std::sync::OnceLock;

static HEAVY: OnceLock<Vec<u8>> = OnceLock::new();

fn heavy_body() -> &'static [u8] {
    HEAVY
        .get_or_init(|| b"Hello, World!".repeat(100_000))
        .as_slice()
}

fn respond_heavy(res: &mut khttp::server::ResponseHandle) {
    let msg = heavy_body();
    let mut headers = HttpHeaders::new();
    headers.set_content_length(msg.len() as u64);
    res.ok(HttpHeaders::new(), msg);
}

fn respond_chunked(res: &mut khttp::server::ResponseHandle) {
    let msg = heavy_body();
    res.send_chunked(&HttpStatus::of(200), HttpHeaders::new(), msg);
}

fn ensure_rewrk() {
    if on_ci() {
        install_rewrk();
        return;
    }
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

fn install_rewrk() {
    println!("installing rewrk");
    let status = Command::new("cargo")
        .args([
            "install",
            "rewrk",
            "--git",
            "https://github.com/ChillFish8/rewrk.git",
        ])
        .status()
        .expect("failed to install rewrk");
    if !status.success() {
        panic!("failed to install rewrk");
    }
}

fn on_ci() -> bool {
    env::var("GITHUB_ACTIONS").is_ok()
}
