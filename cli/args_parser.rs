// cli/args_parser.rs

use std::env::Args;

use khttp::common::HttpMethod;

pub struct ArgsParser {}

impl ArgsParser {
    pub fn parse(mut args: Args) -> Result<MainOp, ArgsError> {
        // skip first arg
        args.next();
        parse_main_op(args)
    }
}

fn parse_main_op(mut args: Args) -> Result<MainOp, ArgsError> {
    let op = get_required_arg(&mut args)?;

    match op.as_str() {
        "server" => Ok(MainOp::Server(parse_server_op(args)?)),
        x => Ok(MainOp::Client(parse_client_op(x, args)?)),
    }
}

fn parse_server_op(mut args: Args) -> Result<ServerOp, ArgsError> {
    let arg = get_required_arg(&mut args)?;
    match arg.as_str() {
        "echo" => Ok(ServerOp::Echo(parse_server_opt_args(&mut args)?)),
        "sleep" => Ok(ServerOp::Sleep(parse_server_opt_args(&mut args)?)),
        _ => Err(ArgsError::InvalidArgs("unknown command".to_string())),
    }
}

fn parse_client_op(method: &str, mut args: Args) -> Result<ClientOp, ArgsError> {
    let method = HttpMethod::from(method.to_uppercase().as_str());
    if let HttpMethod::Custom(_) = method {
        return Err(ArgsError::InvalidArgs("invalid method".to_string()));
    }

    let address = get_required_arg(&mut args)?;
    let (host, port, uri) = parse_address(address)?;
    let opt_args = parse_client_opt_args(&mut args)?;

    Ok(ClientOp {
        method,
        host,
        port,
        uri,
        opt_args,
    })
}

fn get_required_arg(args: &mut Args) -> Result<String, ArgsError> {
    let arg = args.next();
    if arg.is_none() {
        return Err(ArgsError::InvalidArgs("foo".to_string()));
    }
    Ok(arg.unwrap())
}

fn parse_server_opt_args(args: &mut Args) -> Result<Vec<ServerOpArg>, ArgsError> {
    let mut opt_args = Vec::<ServerOpArg>::new();
    loop {
        let arg = args.next();
        if arg.is_none() {
            return Ok(opt_args);
        }
        let arg = arg.unwrap();

        if arg == "-v" || arg == "--verbose" {
            opt_args.push(ServerOpArg::Verbose);
            continue;
        }

        if arg == "-p" || arg == "--port" {
            let port = get_required_arg(args)?;
            let port = port
                .parse()
                .map_err(|_| ArgsError::InvalidArgs("invalid port".to_string()))?;
            opt_args.push(ServerOpArg::Port(port));
            continue;
        }

        if arg == "-b" || arg == "--bind" {
            let address = get_required_arg(args)?;
            opt_args.push(ServerOpArg::BindAddress(address));
            continue;
        }

        if arg == "-t" || arg == "--thread-count" {
            let thread_count = get_required_arg(args)?;
            let thread_count = thread_count
                .parse()
                .map_err(|_| ArgsError::InvalidArgs("invalid thread count".to_string()))?;
            opt_args.push(ServerOpArg::ThreadCount(thread_count));
            continue;
        }
    }
}

fn parse_client_opt_args(args: &mut Args) -> Result<Vec<ClientOpArg>, ArgsError> {
    let mut opt_args = Vec::<ClientOpArg>::new();
    loop {
        let arg = args.next();
        if arg.is_none() {
            return Ok(opt_args);
        }
        let arg = arg.unwrap();

        if arg == "-v" || arg == "--verbose" {
            opt_args.push(ClientOpArg::Verbose);
            continue;
        }

        if arg == "-d" || arg == "--data" {
            // next arg is body
            let body = get_required_arg(args)?;
            opt_args.push(ClientOpArg::Body(body));
            continue;
        }

        if arg == "-H" || arg == "--header" {
            // next arg is header
            let header = get_required_arg(args)?;
            let h: Vec<&str> = header.splitn(2, ":").collect();
            opt_args.push(ClientOpArg::Header((h[0].to_string(), h[1].to_string())));
            continue;
        }
    }
}

fn parse_address(address: String) -> Result<(String, u16, String), ArgsError> {
    // return address, port, uri
    let mut port: u16 = 80;
    let mut uri: String = String::from("/");

    let parts: Vec<&str> = address.splitn(2, "/").collect();
    if parts.len() > 1 {
        uri = parts[1].to_string();
    }

    let host_parts: Vec<&str> = parts[0].splitn(2, ":").collect();
    if host_parts.len() > 2 {
        return Err(ArgsError::InvalidArgs(String::from(
            "invalid address: too many :",
        )));
    }
    if host_parts.len() == 2 {
        port = host_parts[1]
            .parse()
            .map_err(|_| ArgsError::InvalidArgs("invalid port".to_string()))?;
    }

    Ok((host_parts[0].to_string(), port, uri))
}

#[derive(Debug)]
pub enum ArgsError {
    InvalidArgs(String),
}

#[derive(Debug)]
pub enum MainOp {
    Server(ServerOp),
    Client(ClientOp),
}

#[derive(Debug)]
pub enum ServerOp {
    Echo(Vec<ServerOpArg>),
    Sleep(Vec<ServerOpArg>),
}

#[derive(Debug)]
pub enum ServerOpArg {
    Port(u16),
    BindAddress(String),
    ThreadCount(usize),
    Verbose,
}

#[derive(Debug)]
pub struct ClientOp {
    pub method: HttpMethod,
    pub host: String,
    pub port: u16,
    pub uri: String,
    pub opt_args: Vec<ClientOpArg>,
}

#[derive(Debug)]
pub enum ClientOpArg {
    Header((String, String)), // -H or --header
    Body(String),             // -d or --data
    Verbose,                  // -v or --verbose
}
