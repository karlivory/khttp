use crate::BenchId;
use khttp::{Headers, Method, ServerBuilder, Status};
use std::{
    io::{BufRead, BufReader},
    net::TcpListener,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

const BENCH_CONNECTIONS: u32 = 512;
const BENCH_DURATION_SEC: usize = 6;
const BENCH_THREADS: usize = 14;

pub const SERVER_BENCHES: &[ServerBench] = &[
    ServerBench {
        id: BenchId::new("server", "minimal"),
        path: "/",
        sut: ServerUnderTest::Khttp(build_khttp_minimal),
    },
    ServerBench {
        id: BenchId::new("server", "longheader"),
        path: "/",
        sut: ServerUnderTest::Khttp(build_khttp_longheader),
    },
    ServerBench {
        id: BenchId::new("server", "medium"),
        path: "/",
        sut: ServerUnderTest::Khttp(build_khttp_medium),
    },
    ServerBench {
        id: BenchId::new("server", "heavy"),
        path: "/",
        sut: ServerUnderTest::Khttp(build_khttp_heavy),
    },
    ServerBench {
        id: BenchId::new("server", "routing"),
        path: "/foo/bar/baz",
        sut: ServerUnderTest::Khttp(build_khttp_routing),
    },
    ServerBench {
        id: BenchId::new("server", "chunked"),
        path: "/chunked",
        sut: ServerUnderTest::Khttp(build_khttp_chunked),
    },
    ServerBench {
        id: BenchId::new("axum", "minimal"),
        path: "/",
        sut: ServerUnderTest::Axum(build_axum_minimal),
    },
    ServerBench {
        id: BenchId::new("axum", "longheader"),
        path: "/",
        sut: ServerUnderTest::Axum(build_axum_longheader),
    },
    ServerBench {
        id: BenchId::new("axum", "medium"),
        path: "/",
        sut: ServerUnderTest::Axum(build_axum_medium),
    },
    ServerBench {
        id: BenchId::new("axum", "heavy"),
        path: "/",
        sut: ServerUnderTest::Axum(build_axum_heavy),
    },
    ServerBench {
        id: BenchId::new("axum", "routing"),
        path: "/foo/bar/baz",
        sut: ServerUnderTest::Axum(build_axum_routing),
    },
    ServerBench {
        id: BenchId::new("axum", "chunked"),
        path: "/chunked",
        sut: ServerUnderTest::Axum(build_axum_chunked),
    },
];

// ---------------------------------------------------------------------
// khttp
// ---------------------------------------------------------------------

fn build_khttp_minimal() -> khttp::Server {
    let mut app = get_khttp_app();
    app.route(Method::Get, "/", |_, res| {
        res.ok(&get_base_headers(), "Hello, World!".as_bytes())
    });
    app.build()
}

fn build_khttp_longheader() -> khttp::Server {
    let mut app = get_khttp_app();
    app.route(Method::Get, "/", |_, res| {
        let mut headers = get_base_headers();
        for i in 0..50 {
            let n = format!("value{i}");
            let a = "hello".repeat(10);
            headers.add(n, a.into_bytes());
        }
        res.ok(&headers, "hey".as_bytes())
    });
    app.build()
}

fn build_khttp_medium() -> khttp::Server {
    let mut app = get_khttp_app();
    app.route(Method::Get, "/", |_, res| {
        let msg = medium_body();
        let mut headers = get_base_headers();
        headers.set_content_length(Some(msg.len() as u64));
        res.ok(&headers, msg)
    });
    app.build()
}

fn build_khttp_heavy() -> khttp::Server {
    let mut app = get_khttp_app();
    app.route(Method::Get, "/", |_, res| {
        let msg = heavy_body();
        let mut headers = get_base_headers();
        headers.set_content_length(Some(msg.len() as u64));
        res.ok(&headers, msg)
    });
    app.build()
}

fn build_khttp_routing() -> khttp::Server {
    let mut app = get_khttp_app();
    for a in 0..10 {
        for b in 0..10 {
            for c in 0..10 {
                let p = format!("/foo-{a}/bar-{b}/baz-{c}");
                app.route(Method::Get, &p, |_, res| res.ok(&Headers::new(), &[][..]));
            }
        }
    }
    app.route(Method::Get, "/foo/bar/baz", |_, res| {
        res.ok(&Headers::new(), &[][..])
    });
    app.build()
}

fn build_khttp_chunked() -> khttp::Server {
    let mut app = get_khttp_app();
    app.route(Method::Get, "/chunked", |_, res| {
        let msg = medium_body();
        let mut headers = get_base_headers();
        headers.set_transfer_encoding_chunked();
        res.send(&Status::of(200), &headers, msg)
    });
    app.build()
}

// ---------------------------------------------------------------------
// axum
// ---------------------------------------------------------------------

fn build_axum_minimal(router: axum::Router) -> axum::Router {
    use axum::routing::get;
    router.route("/", get(|| async { "Hello, World!" }))
}

fn build_axum_longheader(router: axum::Router) -> axum::Router {
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
}

fn build_axum_medium(router: axum::Router) -> axum::Router {
    use axum::routing::get;
    router.route("/", get(|| async { medium_body() }))
}

fn build_axum_heavy(router: axum::Router) -> axum::Router {
    use axum::routing::get;
    router.route("/", get(|| async { heavy_body() }))
}

fn build_axum_routing(mut router: axum::Router) -> axum::Router {
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
}

fn build_axum_chunked(router: axum::Router) -> axum::Router {
    use axum::body::Body;
    use axum::routing::get;
    use bytes::Bytes;
    use futures_util::stream;

    router.route(
        "/chunked",
        get(|| async {
            // Stream a single chunk (parity with khttp's chunked send).
            let msg = medium_body();
            let stream = stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(msg))]);
            Body::from_stream(stream)
        }),
    )
}

// ---------------------------------------------------------------------
// types & utils
// ---------------------------------------------------------------------

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

pub enum ServerUnderTest {
    Khttp(fn() -> khttp::Server),
    Axum(fn(axum::Router) -> axum::Router),
}

pub struct ServerBench {
    pub id: BenchId,
    pub path: &'static str,
    pub sut: ServerUnderTest,
}

pub fn ensure_rewrk() {
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

pub fn run_server_bench(sb: &ServerBench) {
    let port = match sb.sut {
        ServerUnderTest::Khttp(make_srv) => {
            let srv = make_srv();
            let port = srv.bind_addrs().first().map(|a| a.port()).unwrap();
            thread::spawn(move || srv.serve_epoll());
            port
        }
        ServerUnderTest::Axum(build_router) => {
            let router = build_router(axum::Router::new());
            let port = get_free_port();

            thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async move {
                    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{port}"))
                        .await
                        .unwrap();
                    axum::serve(listener, router).await.unwrap();
                });
            });

            port
        }
    };

    // Wait until server is ready
    thread::sleep(Duration::from_millis(20));

    let mut cmd = Command::new("rewrk");
    cmd.stdout(Stdio::piped());
    cmd.arg("--host")
        .arg(format!("http://127.0.0.1:{port}{}", sb.path));
    cmd.args(["--connections", &BENCH_CONNECTIONS.to_string()]);
    cmd.args(["--threads", &BENCH_THREADS.to_string()]);
    cmd.args(["--duration", &format!("{}s", BENCH_DURATION_SEC)]);

    eprintln!("Running {} on port {}", sb.id, port);

    let mut child = cmd.spawn().expect("failed to spawn rewrk");
    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        match line {
            Ok(l) => println!("  {}", l),
            Err(e) => eprintln!("  [read error] {}", e),
        }
    }

    let status = child.wait().expect("wait rewrk");
    if !status.success() {
        panic!("rewrk exited with non-zero status");
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
    khttp::Server::builder(format!("127.0.0.1:{port}")).unwrap()
}

fn get_base_headers() -> Headers<'static> {
    // for fairness: same headers that axum responds with
    let mut headers = Headers::new();
    headers.add(Headers::CONTENT_TYPE, b"text/plain; charset=utf-8");
    headers
}
