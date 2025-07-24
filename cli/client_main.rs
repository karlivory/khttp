use crate::args_parser::ClientOp;
use khttp::client::{Client, HttpClientError};
use khttp::common::HttpHeaders;
use std::io::Cursor;

pub fn run(op: ClientOp) {
    let address = format!("{}:{}", op.host, op.port);
    let client = Client::new(&address);

    let mut headers = HttpHeaders::new();

    // Set Host
    headers.add("Host", &op.host);

    // Set User-Agent
    headers.add("User-Agent", "khttp-cli/0.1");

    // Set Accept
    headers.add("Accept", "*/*");

    let body = op.body.unwrap_or_default();
    if !body.is_empty() {
        headers.add("Content-Type", "text/plain");
    }

    for (k, v) in op.headers {
        headers.add(&k, &v);
    }

    headers.set_content_length(body.len() as u64);

    match client.exchange(&op.method, &op.uri, headers, Cursor::new(body)) {
        Ok(mut response) => {
            if op.verbose {
                println!("{} {}", response.status.code, response.status.reason);
                for (k, vs) in response.headers.get_map() {
                    for v in vs {
                        println!("{}: {}", k, v);
                    }
                }
                println!();
            }
            let body = response.read_body_to_string().unwrap_or_default();
            print!("{}", body);
        }
        Err(e) => handle_error(e),
    }
}

fn handle_error(err: HttpClientError) {
    eprintln!("ERROR: {err}");
}
