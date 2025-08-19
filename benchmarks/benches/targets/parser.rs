use crate::{BenchId, CriterionBench};
use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, BenchmarkId};
use khttp::Headers;

pub const PARSER_BENCHES: &[CriterionBench] = &[
    CriterionBench {
        id: BenchId::new("parser", "simple"),
        run: bench_parser_simple,
    },
    CriterionBench {
        id: BenchId::new("httparse", "simple"),
        run: bench_httparse_simple,
    },
    CriterionBench {
        id: BenchId::new("parser", "long"),
        run: bench_parser_long,
    },
    CriterionBench {
        id: BenchId::new("httparse", "long"),
        run: bench_httparse_long,
    },
    CriterionBench {
        id: BenchId::new("parser", "response"),
        run: bench_parser_response,
    },
    CriterionBench {
        id: BenchId::new("httparse", "response"),
        run: bench_httparse_response,
    },
];

// ---------------------------------------------------------------------
// benchmark functions
// ---------------------------------------------------------------------

fn bench_parser_response(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.bench_function(BenchmarkId::new("parser:response", ""), |b| {
        let raw = make_simple_response();
        b.iter(|| {
            khttp::Response::parse(std::hint::black_box(&raw[..])).unwrap();
        });
    });
}

fn bench_httparse_response(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.bench_function(BenchmarkId::new("httparse:response", ""), |b| {
        let raw = make_simple_response();
        b.iter(|| {
            let mut headers = [httparse::EMPTY_HEADER; 25];
            let mut resp = httparse::Response::new(&mut headers);
            let _ = resp.parse(std::hint::black_box(&raw[..])).unwrap();

            httparse_alloc_response(resp);
        });
    });
}

fn bench_parser_simple(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.bench_function(BenchmarkId::new("parser:simple", ""), |b| {
        let raw = make_simple_request();
        b.iter(|| {
            khttp::Request::parse(std::hint::black_box(&raw[..])).unwrap();
        });
    });
}

fn bench_httparse_simple(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.bench_function(BenchmarkId::new("httparse:simple", ""), |b| {
        let raw = make_simple_request();
        b.iter(|| {
            let mut headers = [httparse::EMPTY_HEADER; 25];
            let mut req = httparse::Request::new(&mut headers);
            let _ = req.parse(std::hint::black_box(&raw[..])).unwrap();

            // (make the allocations, to keep the parser comparison apples-to-apples)
            httparse_alloc_request(req);
        });
    });
}

fn bench_parser_long(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.bench_function(BenchmarkId::new("parser:long", ""), |b| {
        let raw = make_long_request();
        b.iter(|| {
            khttp::Request::parse(std::hint::black_box(&raw[..])).unwrap();
        });
    });
}

fn bench_httparse_long(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.bench_function(BenchmarkId::new("httparse:long", ""), |b| {
        let raw = make_long_request();
        b.iter(|| {
            let mut headers = [httparse::EMPTY_HEADER; 25];
            let mut req = httparse::Request::new(&mut headers);
            let _ = req.parse(std::hint::black_box(&raw[..])).unwrap();

            // (make the allocations, to keep the parser comparison apples-to-apples)
            httparse_alloc_request(req);
        });
    });
}

// ---------------------------------------------------------------------
// utils
// ---------------------------------------------------------------------

fn make_simple_request() -> Vec<u8> {
    b"GET /foo/bar HTTP/1.1\r\nHost: example.com\r\n\r\n".to_vec()
}

fn make_simple_response() -> Vec<u8> {
    b"HTTP/1.1 200 OK\r\nHost: example.com\r\n\r\n".to_vec()
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
    let mut headers = Headers::new();
    for header in req.headers {
        if header.name.is_empty() {
            break;
        }
        headers.add(header.name, header.value);
    }
    // method?
}
