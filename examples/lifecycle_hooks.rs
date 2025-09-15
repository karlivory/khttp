use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    net::{IpAddr, SocketAddr},
    os::fd::AsRawFd,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::Instant,
};

use khttp::{ConnectionSetupAction, Headers, Method::*, PreRoutingAction, Server, Status};

fn main() {
    let mut app = Server::builder("0.0.0.0:8080").unwrap();

    let peer_table_arc = Arc::new(RwLock::new(PeerTable::default()));
    let conn_table_arc = Arc::new(RwLock::new(ConnectionTable::default()));
    let ip_blacklist_arc = Arc::new(RwLock::new(IpBlacklist::default()));

    // ---------------------------------------------------------------------
    // lifecycle hooks
    // ---------------------------------------------------------------------

    let peer_table = peer_table_arc.clone();
    let conn_table = conn_table_arc.clone();
    let ip_blacklist = ip_blacklist_arc.clone();
    app.connection_setup_hook(move |connection| {
        let (stream, peer_addr) = match connection {
            Ok(conn) => conn,
            Err(_) => return ConnectionSetupAction::Drop,
        };
        let ip = peer_addr.ip();

        // if ip is in black-list, drop the connection
        {
            let lock = ip_blacklist.read().unwrap();
            if lock.blacklist.contains(&ip) {
                return ConnectionSetupAction::Drop; // socket gets closed
            }
        }

        // update peer table: add new peer if not present, increment connection counters
        {
            let mut lock = peer_table.write().unwrap();
            let peer = lock.peers.entry(ip).or_default();
            peer.total_connections += 1;
            peer.active_connections += 1;
        }

        // update connection table
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

        // update connection table: increment request counter
        {
            let lock = conn_table.read().unwrap();
            lock.connections
                .get(&fd)
                .map(|conn| conn.request_count.fetch_add(1, Ordering::Relaxed));
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

        // update connection table: remove the connection
        let conn_info = {
            let mut lock = conn_table.write().unwrap();
            lock.connections.remove(&fd)
        };

        // update peer table: decrement active connection counter
        if let Some(conn_info) = conn_info {
            let mut lock = peer_table.write().unwrap();
            lock.peers
                .entry(conn_info.peer_addr.ip())
                .and_modify(|x| x.active_connections = x.active_connections.saturating_sub(1));
        }
    });

    // ---------------------------------------------------------------------
    // routes
    // ---------------------------------------------------------------------

    let ip_black_list = ip_blacklist_arc.clone();
    app.route(Post, "/block/:ip", move |ctx, res| {
        let ip = match ctx.params.get("ip").unwrap().parse() {
            Ok(ip) => ip,
            Err(_) => return res.send(&Status::BAD_REQUEST, Headers::close(), "invalid ip"),
        };
        let added = {
            let mut lock = ip_black_list.write().unwrap();
            lock.blacklist.insert(ip)
        };
        let response = if added {
            format!("Added {ip} to blacklist.")
        } else {
            format!("{ip} already in blacklist!")
        };
        res.ok(Headers::empty(), response)
    });

    let ip_black_list = ip_blacklist_arc.clone();
    app.route(Post, "/allow/:ip", move |ctx, res| {
        let ip = match ctx.params.get("ip").unwrap().parse() {
            Ok(ip) => ip,
            Err(_) => return res.send(&Status::BAD_REQUEST, Headers::close(), "invalid ip"),
        };
        let removed = {
            let mut lock = ip_black_list.write().unwrap();
            lock.blacklist.remove(&ip)
        };
        let response = if removed {
            format!("Removed {ip} from blacklist.")
        } else {
            format!("{ip} was not in blacklist!")
        };
        res.ok(Headers::empty(), response)
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

// ---------------------------------------------------------------------
// types & utils
// ---------------------------------------------------------------------

#[derive(Default)]
struct IpBlacklist {
    blacklist: HashSet<IpAddr>,
}

#[derive(Default)]
struct PeerTable {
    peers: HashMap<IpAddr, PeerInfo>,
}

impl PeerTable {
    fn print_to_string(&self, buf: &mut String) {
        for (ip, peer) in &self.peers {
            let _ = writeln!(buf, "peer (ip = {})", ip);
            let _ = writeln!(buf, "    active_connections: {}", peer.active_connections);
            let _ = writeln!(buf, "    total_connections: {}", peer.total_connections);
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
            let request_count = conn.request_count.load(Ordering::Relaxed);
            let _ = writeln!(buf, "stream (fd = {})", fd);
            let _ = writeln!(buf, "    peer_addr: {}", conn.peer_addr);
            let _ = writeln!(buf, "    request_count: {}", request_count);
            let _ = writeln!(buf, "    duration: {}ms", conn_duration);
        }
    }
}

struct ConnectionInfo {
    peer_addr: SocketAddr,
    request_count: AtomicU64,
    conn_start: Instant,
}

impl ConnectionInfo {
    fn new(peer_addr: SocketAddr) -> Self {
        ConnectionInfo {
            peer_addr,
            request_count: AtomicU64::new(0),
            conn_start: Instant::now(),
        }
    }
}
