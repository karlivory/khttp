// src/lib.rs
pub mod client;
pub mod common;
pub mod http_parser;
pub mod http_printer;
pub mod router;
pub mod server;
pub mod threadpool;

// TODO: make cli a feature flag
// #[cfg(feature = "cli")]
// pub mod cli;
