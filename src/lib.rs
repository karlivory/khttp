// src/lib.rs
pub mod body_reader;
pub mod common;
pub mod parser;
pub mod printer;
pub mod router;
pub mod server;
pub mod threadpool;

#[cfg(feature = "client")]
pub mod client;
