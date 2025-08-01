// src/lib.rs
mod body_reader;
mod http;
mod parser;
mod printer;
mod router;
mod server;
mod threadpool;

pub use body_reader::{BodyReader, ChunkedReader};
pub use http::{Headers, Method, RequestUri, Status};
pub use parser::{HttpParsingError, Parser, RequestParts, ResponseParts};
pub use printer::HttpPrinter;
pub use router::{HttpRouter, Router, RouterBuilder};
pub use server::{
    ConnectionMeta, PreRoutingAction, PreRoutingHookFn, RequestContext, ResponseHandle, RouteFn,
    Server, ServerBuilder, StreamSetupAction, StreamSetupFn,
};

#[cfg(feature = "client")]
mod client;
#[cfg(feature = "client")]
pub use client::{Client, ClientError, ClientResponseHandle};
