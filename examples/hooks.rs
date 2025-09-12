use std::{
    collections::HashMap,
    fmt::Write,
    net::{IpAddr, SocketAddr},
    os::fd::AsRawFd,
    sync::{Arc, RwLock},
    time::Instant,
};

use khttp::{ConnectionSetupAction, Headers, Method::*, PreRoutingAction, Server};

fn main() {
    let mut app = Server::builder("127.0.0.1:8080").unwrap();

    let peer_table_arc = Arc::new(RwLock::new(PeerTable::default()));
    let conn_table_arc = Arc::new(RwLock::new(ConnectionTable::default()));

    let peer_table = peer_table_arc.clone();
    let conn_table = conn_table_arc.clone();
    let ip_black_list: Vec<IpAddr> = vec!["1.2.3.4".parse().unwrap()];
    app.connection_setup_hook(move |connection| {
        let (stream, peer_addr) = match connection {
            Ok(conn) => conn,
            Err(_) => return ConnectionSetupAction::Drop,
        };

        if ip_black_list.contains(&peer_addr.ip()) {
            return ConnectionSetupAction::Drop; // socket gets closed
        }

        let ip = peer_addr.ip();
        {
            let mut lock = peer_table.write().unwrap();
            let peer = lock.peers.entry(ip).or_default();
            peer.total_connections += 1;
            peer.active_connections += 1;
        }

        let fd = stream.as_raw_fd();
        {
            let conn = ConnectionInfo::new(peer_addr);
            let mut lock = conn_table.write().unwrap();
            lock.connections.insert(fd, conn);
        }

        ConnectionSetupAction::Proceed(stream)
    });

    let conn_table = conn_table_arc.clone();
    app.pre_routing_hook(move |_req, res| {
        let fd = res.get_stream().as_raw_fd();
        {
            let mut lock = conn_table.write().unwrap();
            lock.connections
                .entry(fd)
                .and_modify(|conn| conn.request_count += 1);
        }

        PreRoutingAction::Proceed
    });

    let peer_table = peer_table_arc.clone();
    let conn_table = conn_table_arc.clone();
    app.connection_teardown_hook(move |stream, io_result| {
        if let Err(e) = io_result {
            eprintln!("socket err: {e}");
        };

        let fd = stream.as_raw_fd();
        let conn_info = {
            let mut lock = conn_table.write().unwrap();
            lock.connections.remove(&fd)
        };

        if let Some(conn_info) = conn_info {
            let mut lock = peer_table.write().unwrap();
            lock.peers
                .entry(conn_info.peer_addr.ip())
                .and_modify(|x| x.active_connections -= 1);
        }
    });

    let peer_table = peer_table_arc.clone();
    app.route(Get, "/peers", move |_, res| {
        let mut body = String::with_capacity(1024);
        {
            let lock = peer_table.read().unwrap();
            lock.print_to_string(&mut body);
        }
        res.ok(Headers::empty(), body)
    });

    let conn_table = conn_table_arc.clone();
    app.route(Get, "/connections", move |_, res| {
        let mut body = String::with_capacity(1024);
        {
            let lock = conn_table.read().unwrap();
            lock.print_to_string(&mut body);
        }
        res.ok(Headers::empty(), body)
    });

    app.route(Get, "/", |_, res| res.ok(Headers::empty(), "Hello, World!"));

    app.build().serve().unwrap();
}

#[derive(Default)]
struct PeerTable {
    peers: HashMap<IpAddr, PeerInfo>,
}

impl PeerTable {
    fn print_to_string(&self, buf: &mut String) {
        for (ip, peer) in &self.peers {
            writeln!(buf, "peer (ip = {})", ip).ok();
            writeln!(buf, "    active_connections: {}", peer.active_connections).ok();
            writeln!(buf, "    total_connections: {}", peer.total_connections).ok();
        }
    }
}

#[derive(Default)]
struct PeerInfo {
    total_connections: u64,
    active_connections: u64,
}

#[derive(Default)]
struct ConnectionTable {
    connections: HashMap<i32, ConnectionInfo>,
}

impl ConnectionTable {
    fn print_to_string(&self, buf: &mut String) {
        for (fd, conn) in &self.connections {
            let conn_duration = conn.conn_start.elapsed().as_millis();
            writeln!(buf, "stream (fd = {})", fd).ok();
            writeln!(buf, "    peer_addr: {}", conn.peer_addr).ok();
            writeln!(buf, "    request_count: {}", conn.request_count).ok();
            writeln!(buf, "    duration: {}ms", conn_duration).ok();
        }
    }
}

struct ConnectionInfo {
    peer_addr: SocketAddr,
    request_count: u64,
    conn_start: Instant,
}

impl ConnectionInfo {
    fn new(peer_addr: SocketAddr) -> Self {
        ConnectionInfo {
            peer_addr,
            request_count: 0,
            conn_start: Instant::now(),
        }
    }
}
