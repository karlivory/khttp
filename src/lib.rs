mod body_reader;
pub mod date;
mod http;
mod parser;
mod printer;
mod router;
mod server;
mod threadpool;

pub use body_reader::BodyReader;
pub use http::{Headers, Method, RequestUri, Status};
pub use parser::{HttpParsingError, Request};
pub use printer::HttpPrinter;
pub use router::{RouteParams, Router, RouterBuilder};
pub use server::{
    ConnectionSetupAction, PreRoutingAction, RequestContext, ResponseHandle, RouteFn, Server,
    ServerBuilder,
};

#[cfg(feature = "client")]
mod client;
#[cfg(feature = "client")]
pub use client::{Client, ClientError, ClientResponseHandle};
#[cfg(feature = "client")]
pub use parser::Response;
