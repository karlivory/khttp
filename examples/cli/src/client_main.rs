use crate::args_parser::ClientOp;
use khttp::{Client, ClientError, Headers};
use std::io::{self, Cursor, Read};
use std::time::Duration;

pub fn run(op: ClientOp) {
    let address = format!("{}:{}", op.host, op.port);
    let mut client = Client::new(&address);

    let mut headers = Headers::new();

    headers.replace("host", op.host.as_bytes());
    headers.replace("user-agent", b"khttp-cli/0.1");
    headers.replace("accept", b"*/*");

    let body = op.body.unwrap_or_default();
    if !body.is_empty() && headers.get(Headers::CONTENT_TYPE).is_none() {
        headers.replace(Headers::CONTENT_TYPE, b"text/plain");
    }

    headers.set_content_length(Some(body.len() as u64));

    let reader: Box<dyn Read> = if op.stall > 0 {
        Box::new(StallingBodyReader::new(body.into_bytes(), op.stall))
    } else {
        Box::new(Cursor::new(body))
    };

    for (k, v) in &op.headers {
        headers.add(k, v.as_bytes());
    }

    match client.exchange(&op.method, &op.uri, &headers, reader) {
        Ok(mut response) => {
            if op.verbose {
                println!("{} {}", response.status.code, response.status.reason);
                println!("{}", response.headers);
            }
            let body = response.body().string().unwrap_or_default();
            print!("{}", body);
        }
        Err(e) => handle_error(e),
    }
}

fn handle_error(err: ClientError) {
    eprintln!("ERROR: {err}");
}

pub struct StallingBodyReader {
    head: u8,
    rest: Vec<u8>,
    stall_duration: Duration,
    state: State,
}

enum State {
    WriteFirst,
    Stalled,
    WriteRest,
    Done,
}

impl StallingBodyReader {
    pub fn new(body: Vec<u8>, stall_ms: u64) -> Self {
        let mut iter = body.into_iter();
        let head = iter.next().unwrap_or(b'\n'); // fallback if body empty
        let rest: Vec<u8> = iter.collect();

        Self {
            head,
            rest,
            stall_duration: Duration::from_millis(stall_ms),
            state: State::WriteFirst,
        }
    }
}

impl Read for StallingBodyReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.state {
            State::WriteFirst => {
                buf[0] = self.head;
                self.state = State::Stalled;
                Ok(1)
            }
            State::Stalled => {
                std::thread::sleep(self.stall_duration);
                self.state = State::WriteRest;
                self.read(buf) // fallthrough
            }
            State::WriteRest => {
                if self.rest.is_empty() {
                    self.state = State::Done;
                    return Ok(0);
                }
                let n = self.rest.len().min(buf.len());
                buf[..n].copy_from_slice(&self.rest[..n]);
                self.rest.drain(..n);
                Ok(n)
            }
            State::Done => Ok(0),
        }
    }
}
