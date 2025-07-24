// src/lib.rs
pub mod body_reader;
pub mod common;
pub mod http_parser;
pub mod http_printer;
pub mod router;
pub mod server;
pub mod threadpool;

#[cfg(feature = "client")]
pub mod client;
