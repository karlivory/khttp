// cli/argparse.rs
//
// responsibility: parsing args into a MainOp enum (?)
//
// I want stuff like
// khttp server echo
// khttp server upper 8080
//
// khttp get google.com
// khttp post localhost:8080 -dfoobar

use std::env::Args;

use khttp::common::HttpMethod;

pub struct ArgsParser {}

impl ArgsParser {
    pub fn parse(args: Args) -> MainOp {
        todo!();
    }
}

pub enum MainOp {
    Server(ServerOp),
    Client(ClientOp),
}

pub enum ServerOp {
    Echo,
    Upper,
}

pub struct ClientOp {
    method: HttpMethod,
    address: String,
}
