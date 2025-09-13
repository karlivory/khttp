mod args_parser;
mod client_main;
mod server_main;

use args_parser::{ArgsError, ArgsParser, MainOp};
use std::{env, process::exit};

fn main() {
    let args = env::args();

    if env::args().len() < 2 || env::args().any(|x| x == "-h" || x == "--help") {
        print_help();
        exit(0);
    }

    match ArgsParser::parse(args) {
        Err(args_err) => {
            match args_err {
                ArgsError::InvalidArgs(msg) => {
                    eprintln!("err: {}", msg);
                }
            }
            print_usage();
            exit(1);
        }
        Ok(MainOp::Server(op)) => server_main::run(op),
        Ok(MainOp::Client(op)) => client_main::run(op),
    }
}

fn print_usage() {
    println!("try 'khttp --help' for more information");
}

fn print_help() {
    println!("khttp - minimal synchronous HTTP/1.1 server + client");
    println!();
    println!("USAGE:");
    println!("  khttp [METHOD] <host[:port][/uri]> [options]");
    println!("  khttp server <subcommand> [options]");
    println!();
    println!("METHOD:");
    println!("  GET | POST | PUT | DELETE | ...");
    println!();
    println!("CLIENT OPTIONS:");
    println!("  -H, --header <key: value>     Add custom header");
    println!("  -d, --data <string>           Set request body");
    println!("  -v, --verbose                 Print response headers");
    println!("  --stall <ms>                  Stall writing after first byte");
    println!("  -h, --help                    Show this help");
    println!();
    println!("SERVER SUBCOMMANDS:");
    println!("  echo                          Echoes back POST body");
    println!("  sleep                         Delays response 3s");
    println!();
    println!("SERVER OPTIONS:");
    println!("  -b, --bind <address>          Default: 127.0.0.1");
    println!("  -p, --port <number>           Default: 8080");
    println!("  -t, --thread-count <ms>       Number of worker threads");
    println!("  --tcp-read-timeout <ms>       TCP socket read timeout");
    println!("  --tcp-write-timeout <ms>      TCP socket write timeout");
    println!("  --tcp-nodelay                 Enable TCP_NODELAY");
    println!("  -v, --verbose                 Verbose output");
    println!("  -h, --help                    Show this help");
}
