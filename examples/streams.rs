use std::io;
use std::io::{Read, Write};

use khttp::{Headers, Method::*, Server};

static INDEX_HTML: &[u8] = b"\
<body>
<h3>Welcome to streams!</h3>
<p>Available paths:</p>
<ul>
  <li>POST /echo - streams back your input</li>
  <li>POST /upper - streams back your input <b>in uppercase</b></li>
  <li>POST /rot13 - state of the art encryption</li>
</ul>
</body>
";

fn main() {
    let mut app = Server::builder("127.0.0.1:8080").unwrap();
    app.thread_count(20);

    app.route(Get, "/**", |_, r| {
        let mut headers = Headers::new();
        headers.add("Content-Type", b"text/html; charset=utf-8");
        r.ok(&headers, INDEX_HTML)
    });

    // streams received request body back to client
    app.route(Post, "/echo", |mut c, r| r.ok(&c.headers.clone(), c.body()));

    app.route(Post, "/upper", |mut c, r| {
        let response_stream = c
            .body()
            .map_bytes(|b| b.to_ascii_uppercase())
            .tee(io::stdout());
        r.ok(Headers::empty(), response_stream)
    });

    app.route(Post, "/rot13", |mut c, r| {
        r.ok(Headers::empty(), c.body().rot13())
    });

    app.build().serve_epoll().unwrap();
}

trait ReadExt: Read + Sized {
    fn map_bytes<F>(self, map_fn: F) -> MapBytes<Self, F>
    where
        F: Fn(u8) -> u8,
    {
        MapBytes {
            inner: self,
            map_fn,
        }
    }

    fn tee<W: Write>(self, sink: W) -> Tee<Self, W> {
        Tee { inner: self, sink }
    }

    fn rot13(self) -> MapBytes<Self, fn(u8) -> u8> {
        self.map_bytes(|b| match b {
            b'a'..=b'z' => (((b - b'a') + 13) % 26) + b'a',
            b'A'..=b'Z' => (((b - b'A') + 13) % 26) + b'A',
            _ => b,
        })
    }

    // fn gzip(self) -> flate2::read::GzEncoder<Self> {
    //     flate2::read::GzEncoder::new(self, flate2::Compression::default())
    // }
}

struct MapBytes<R, F> {
    inner: R,
    map_fn: F,
}

impl<R: Read, F: Fn(u8) -> u8> Read for MapBytes<R, F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        for b in &mut buf[..n] {
            *b = (self.map_fn)(*b);
        }
        Ok(n)
    }
}

impl<R: Read> ReadExt for R {}

struct Tee<R, W> {
    inner: R,
    sink: W,
}

impl<R: Read, W: Write> Read for Tee<R, W> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.sink.write_all(&buf[..n])?;
        self.sink.flush()?;
        Ok(n)
    }
}
