use khttp::common::HttpMethod;
use std::env::Args;

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
    Echo(ServerConfig),
    Sleep(ServerConfig),
}

#[derive(Debug)]
pub struct ServerConfig {
    pub port: u16,
    pub bind: String,
    pub thread_count: Option<usize>,
    pub verbose: bool,
}

#[derive(Debug)]
pub struct ClientOp {
    pub method: HttpMethod,
    pub host: String,
    pub port: u16,
    pub uri: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub verbose: bool,
}

pub struct ArgsParser;

impl ArgsParser {
    pub fn parse(mut args: Args) -> Result<MainOp, ArgsError> {
        args.next(); // skip binary name
        let op = get_required(&mut args)?;
        match op.as_str() {
            "server" => Self::parse_server(args),
            method => Self::parse_client(method, args),
        }
    }

    fn parse_server(mut args: Args) -> Result<MainOp, ArgsError> {
        let sub = get_required(&mut args)?;
        let mut config = ServerConfig {
            port: 8080,
            bind: "127.0.0.1".to_string(),
            thread_count: None,
            verbose: false,
        };
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-p" | "--port" => config.port = parse_u16(&mut args, "port")?,
                "-b" | "--bind" => config.bind = get_required(&mut args)?,
                "-t" | "--thread-count" => {
                    config.thread_count = Some(parse_usize(&mut args, "thread count")?)
                }
                "-v" | "--verbose" => config.verbose = true,
                _ => return Err(ArgsError::InvalidArgs(format!("Unknown flag: {arg}"))),
            }
        }
        match sub.as_str() {
            "echo" => Ok(MainOp::Server(ServerOp::Echo(config))),
            "sleep" => Ok(MainOp::Server(ServerOp::Sleep(config))),
            _ => Err(ArgsError::InvalidArgs("Unknown server command".to_string())),
        }
    }

    fn parse_client(method: &str, mut args: Args) -> Result<MainOp, ArgsError> {
        let method = HttpMethod::from(method.to_uppercase().as_str());
        if let HttpMethod::Custom(_) = method {
            return Err(ArgsError::InvalidArgs("Invalid HTTP method".to_string()));
        }
        let addr = get_required(&mut args)?;
        let (host, port, uri) = parse_address(addr)?;
        let mut headers = vec![];
        let mut body = None;
        let mut verbose = false;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-H" | "--header" => {
                    let header = get_required(&mut args)?;
                    let parts: Vec<&str> = header.splitn(2, ":").collect();
                    if parts.len() != 2 {
                        return Err(ArgsError::InvalidArgs("Malformed header".to_string()));
                    }
                    headers.push((parts[0].trim().to_string(), parts[1].trim().to_string()));
                }
                "-d" | "--data" => body = Some(get_required(&mut args)?),
                "-v" | "--verbose" => verbose = true,
                _ => return Err(ArgsError::InvalidArgs(format!("Unknown flag: {arg}"))),
            }
        }
        Ok(MainOp::Client(ClientOp {
            method,
            host,
            port,
            uri,
            headers,
            body,
            verbose,
        }))
    }
}

fn get_required(args: &mut Args) -> Result<String, ArgsError> {
    args.next()
        .ok_or_else(|| ArgsError::InvalidArgs("missing argument".into()))
}

fn parse_u16(args: &mut Args, label: &str) -> Result<u16, ArgsError> {
    get_required(args)?
        .parse()
        .map_err(|_| ArgsError::InvalidArgs(format!("invalid {label}")))
}

fn parse_usize(args: &mut Args, label: &str) -> Result<usize, ArgsError> {
    get_required(args)?
        .parse()
        .map_err(|_| ArgsError::InvalidArgs(format!("invalid {label}")))
}

fn parse_address(address: String) -> Result<(String, u16, String), ArgsError> {
    let mut port = 80;
    let mut uri = "/".to_string();
    let parts: Vec<&str> = address.splitn(2, '/').collect();
    if parts.len() > 1 {
        uri = format!("/{}", parts[1]);
    }
    let host_parts: Vec<&str> = parts[0].splitn(2, ':').collect();
    if host_parts.len() == 2 {
        port = host_parts[1]
            .parse()
            .map_err(|_| ArgsError::InvalidArgs("invalid port".to_string()))?;
    }
    Ok((host_parts[0].to_string(), port, uri))
}
