//! Candidate gathering for str0m (which has no STUN/TURN client of its own).
//!
//! - host: one std `UdpSocket` per usable IPv4 interface (bound to the interface address, so the
//!   `destination` of a received datagram equals the host candidate address str0m knows).
//!   `PEER_CORE_HOST_IPS=127.0.0.1,...` overrides the interface list (tests, pinning).
//! - srflx: ONE hand-rolled RFC 5389 Binding request sent over each host socket to the STUN
//!   server BEFORE the socket is handed to the I/O loop (≈60 LOC, no `stun` crate needed). The
//!   mapped address becomes `Candidate::server_reflexive(mapped, host_addr)`.
//! - relay: the webrtc-rs `turn` crate on a dedicated current-thread tokio runtime. Its relayed
//!   `Conn` is a SECOND "socket" for str0m: str0m transmits with `source == relayed_addr` are sent
//!   through the relay conn; datagrams from the relay conn are fed as `destination = relayed_addr`.
use std::net::{IpAddr, Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::link::PeerError;

/// A datagram from any of our sockets, destined for the I/O loop.
pub(crate) struct NetIn {
    pub source: SocketAddr,
    pub destination: SocketAddr,
    pub data: Vec<u8>,
}

pub(crate) fn host_ips() -> Vec<IpAddr> {
    if let Ok(list) = std::env::var("PEER_CORE_HOST_IPS") {
        return list.split(',').filter_map(|s| s.trim().parse().ok()).collect();
    }
    let mut out: Vec<IpAddr> = Vec::new();
    if let Ok(ifs) = if_addrs::get_if_addrs() {
        // VPN/tunnel interfaces (Tailscale utun/tun, WireGuard, IPsec, PPP) are NOT ICE host
        // candidates: a pair over them would measure the VPN's own traversal, not ours.
        // PEER_CORE_ALLOW_VPN=1 opts back in.
        let allow_vpn = std::env::var("PEER_CORE_ALLOW_VPN").map(|v| v == "1").unwrap_or(false);
        for i in ifs {
            let n = i.name.as_str();
            if !allow_vpn && ["utun", "tun", "ipsec", "ppp", "wg", "tailscale"].iter().any(|p| n.starts_with(p)) {
                continue;
            }
            let ip = i.ip();
            match ip {
                IpAddr::V4(v4) => {
                    if v4.is_loopback() || v4.is_link_local() || v4.is_unspecified() {
                        continue;
                    }
                }
                IpAddr::V6(_) => continue, // spike: IPv4 only
            }
            if !out.contains(&ip) {
                out.push(ip);
            }
        }
    }
    if out.is_empty() {
        out.push(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    out
}

pub(crate) fn resolve(hostport: &str) -> Result<SocketAddr, PeerError> {
    hostport
        .to_socket_addrs()
        .map_err(|e| PeerError::Gathering(format!("resolve {hostport}: {e}")))?
        .find(|a| a.is_ipv4())
        .ok_or_else(|| PeerError::Gathering(format!("resolve {hostport}: no ipv4")))
}

const MAGIC: u32 = 0x2112_A442;

/// One STUN Binding request over `sock` → the XOR-MAPPED-ADDRESS. Blocking, `timeout` total,
/// two tries. Must run before the socket's reader thread starts (it reads the socket itself).
pub(crate) fn stun_binding(sock: &UdpSocket, server: SocketAddr, timeout: Duration) -> Option<SocketAddr> {
    let mut txid = [0u8; 12];
    for chunk in txid.chunks_mut(4) {
        chunk.copy_from_slice(&rand_u32().to_le_bytes());
    }
    let mut req = Vec::with_capacity(20);
    req.extend_from_slice(&0x0001u16.to_be_bytes());
    req.extend_from_slice(&0u16.to_be_bytes());
    req.extend_from_slice(&MAGIC.to_be_bytes());
    req.extend_from_slice(&txid);
    let prev = sock.read_timeout().ok().flatten();
    let per_try = timeout / 2;
    let _ = sock.set_read_timeout(Some(Duration::from_millis(100)));
    let mut buf = [0u8; 1500];
    let mut result = None;
    'tries: for _ in 0..2 {
        if sock.send_to(&req, server).is_err() {
            break;
        }
        let start = std::time::Instant::now();
        while start.elapsed() < per_try {
            let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
            if from != server || n < 20 || buf[8..20] != txid {
                continue;
            }
            result = parse_mapped(&buf[..n]);
            break 'tries;
        }
    }
    let _ = sock.set_read_timeout(prev);
    result
}

fn parse_mapped(msg: &[u8]) -> Option<SocketAddr> {
    if u16::from_be_bytes([msg[0], msg[1]]) != 0x0101 {
        return None;
    }
    let len = u16::from_be_bytes([msg[2], msg[3]]) as usize;
    let mut i = 20;
    let end = (20 + len).min(msg.len());
    let mut plain = None;
    while i + 4 <= end {
        let ty = u16::from_be_bytes([msg[i], msg[i + 1]]);
        let alen = u16::from_be_bytes([msg[i + 2], msg[i + 3]]) as usize;
        let v = &msg[i + 4..(i + 4 + alen).min(end)];
        if v.len() >= 8 && v[1] == 0x01 {
            let port = u16::from_be_bytes([v[2], v[3]]);
            let ip = [v[4], v[5], v[6], v[7]];
            if ty == 0x0020 {
                let port = port ^ (MAGIC >> 16) as u16;
                let m = MAGIC.to_be_bytes();
                let ip = Ipv4Addr::new(ip[0] ^ m[0], ip[1] ^ m[1], ip[2] ^ m[2], ip[3] ^ m[3]);
                return Some(SocketAddr::new(IpAddr::V4(ip), port));
            } else if ty == 0x0001 {
                plain = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(ip)), port));
            }
        }
        i += 4 + ((alen + 3) & !3);
    }
    plain
}

fn rand_u32() -> u32 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    h.finish() as u32
}

/// Start a reader thread for a host socket. It exits when `stop` flips or the loop's receiver is
/// gone.
pub(crate) fn spawn_reader(
    sock: std::sync::Arc<UdpSocket>,
    tx: Sender<crate::io::Msg>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    let local = sock.local_addr().expect("bound socket");
    std::thread::Builder::new()
        .name(format!("peer-udp-{local}"))
        .spawn(move || {
            let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));
            let mut buf = vec![0u8; 2048];
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                match sock.recv_from(&mut buf) {
                    Ok((n, source)) => {
                        let m = crate::io::Msg::Net(NetIn { source, destination: local, data: buf[..n].to_vec() });
                        if tx.send(m).is_err() {
                            break;
                        }
                    }
                    Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                    Err(_) => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        })
        .expect("spawn reader");
}

/// A live TURN allocation. Dropping `tx` (the link closing) ends the runtime thread, which
/// closes the relay conn (Refresh lifetime 0) and the client.
#[cfg(feature = "turn")]
pub(crate) struct TurnAlloc {
    pub relayed: SocketAddr,
    pub local: SocketAddr,
    pub mapped: Option<SocketAddr>,
    pub tx: tokio::sync::mpsc::UnboundedSender<(Vec<u8>, SocketAddr)>,
}

#[cfg(feature = "turn")]
pub(crate) fn start_turn(
    server: SocketAddr,
    local_ip: IpAddr,
    user: String,
    pass: String,
    realm: String,
    net_tx: Sender<crate::io::Msg>,
    timeout: Duration,
) -> Result<TurnAlloc, PeerError> {
    use webrtc_util::Conn;
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<TurnAlloc, String>>(1);
    std::thread::Builder::new()
        .name("peer-turn".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = ready_tx.send(Err(format!("tokio: {e}")));
                    return;
                }
            };
            rt.block_on(async move {
                let bind = SocketAddr::new(local_ip, 0);
                let sock = match tokio::net::UdpSocket::bind(bind).await {
                    Ok(s) => std::sync::Arc::new(s),
                    Err(e) => {
                        let _ = ready_tx.send(Err(format!("bind {bind}: {e}")));
                        return;
                    }
                };
                let local = sock.local_addr().unwrap_or(bind);
                let cfg = turn::client::ClientConfig {
                    stun_serv_addr: server.to_string(),
                    turn_serv_addr: server.to_string(),
                    username: user,
                    password: pass,
                    realm,
                    software: "exp-vapp3".into(),
                    rto_in_ms: 0,
                    conn: sock,
                    vnet: None,
                };
                let client = match turn::client::Client::new(cfg).await {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = ready_tx.send(Err(format!("turn client: {e}")));
                        return;
                    }
                };
                if let Err(e) = client.listen().await {
                    let _ = ready_tx.send(Err(format!("turn listen: {e}")));
                    return;
                }
                let mapped = client.send_binding_request().await.ok();
                let relay = match client.allocate().await {
                    Ok(r) => std::sync::Arc::new(r),
                    Err(e) => {
                        let _ = ready_tx.send(Err(format!("turn allocate: {e}")));
                        let _ = client.close().await;
                        return;
                    }
                };
                let relayed = match relay.local_addr() {
                    Ok(a) => a,
                    Err(e) => {
                        let _ = ready_tx.send(Err(format!("relayed addr: {e}")));
                        return;
                    }
                };
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(Vec<u8>, SocketAddr)>();
                let _ = ready_tx.send(Ok(TurnAlloc { relayed, local, mapped, tx }));
                let reader = {
                    let relay = relay.clone();
                    tokio::spawn(async move {
                        let mut buf = vec![0u8; 2048];
                        loop {
                            match relay.recv_from(&mut buf).await {
                                Ok((n, from)) => {
                                    let m = crate::io::Msg::Net(NetIn { source: from, destination: relayed, data: buf[..n].to_vec() });
                                    if net_tx.send(m).is_err() {
                                        break;
                                    }
                                }
                                Err(e) => {
                                    tracing::debug!("turn relay recv ended: {e}");
                                    break;
                                }
                            }
                        }
                    })
                };
                // GOTCHA: turn's RelayConn::send_to holds its internal lock across a
                // CreatePermission transaction, and deletes + retries the permission on EVERY send
                // after a 403 (coturn forbids e.g. its own/CGNAT addresses). ICE checks to such a
                // candidate then stall all relayed data. Negative-cache the peer IP for 10 s.
                let mut blocked: std::collections::HashMap<IpAddr, std::time::Instant> = Default::default();
                // Optional pacing experiment: PEER_CORE_TURN_PACE_KBPMS = KiB allowed per ms.
                let pace: Option<usize> = std::env::var("PEER_CORE_TURN_PACE_KBPMS").ok().and_then(|v| v.parse().ok()).map(|k: usize| k * 1024);
                let mut window = (tokio::time::Instant::now(), 0usize);
                while let Some((data, dst)) = rx.recv().await {
                    if let Some(budget) = pace {
                        if window.0.elapsed() >= Duration::from_millis(1) {
                            window = (tokio::time::Instant::now(), 0);
                        } else if window.1 + data.len() > budget {
                            tokio::time::sleep_until(window.0 + Duration::from_millis(1)).await;
                            window = (tokio::time::Instant::now(), 0);
                        }
                        window.1 += data.len();
                    }
                    if blocked.get(&dst.ip()).is_some_and(|t| *t > std::time::Instant::now()) {
                        continue;
                    }
                    let t = std::time::Instant::now();
                    let r = relay.send_to(&data, dst).await;
                    let el = t.elapsed();
                    if el > Duration::from_millis(2) {
                        tracing::debug!("turn relay send_to {dst} took {el:?} ({} bytes)", data.len());
                    }
                    if let Err(e) = r {
                        tracing::debug!("turn relay send_to {dst}: {e} (ignoring this peer ip for 10s)");
                        blocked.insert(dst.ip(), std::time::Instant::now() + Duration::from_secs(10));
                    }
                }
                let _ = relay.close().await;
                let _ = client.close().await;
                reader.abort();
            });
        })
        .map_err(|e| PeerError::Gathering(format!("spawn turn thread: {e}")))?;
    match ready_rx.recv_timeout(timeout) {
        Ok(Ok(a)) => Ok(a),
        Ok(Err(e)) => Err(PeerError::Gathering(e)),
        Err(_) => Err(PeerError::Gathering("turn allocate timed out".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_xor_mapped_address() {
        // Binding success, XOR-MAPPED-ADDRESS 192.0.2.1:32853 (RFC 5769 style).
        let mut m = vec![0x01, 0x01, 0x00, 0x0c];
        m.extend_from_slice(&MAGIC.to_be_bytes());
        m.extend_from_slice(&[0u8; 12]);
        m.extend_from_slice(&[0x00, 0x20, 0x00, 0x08, 0x00, 0x01]);
        let port: u16 = 32853 ^ 0x2112;
        m.extend_from_slice(&port.to_be_bytes());
        let ip = [192u8 ^ 0x21, 0 ^ 0x12, 2 ^ 0xA4, 1 ^ 0x42];
        m.extend_from_slice(&ip);
        assert_eq!(parse_mapped(&m), Some("192.0.2.1:32853".parse().unwrap()));
    }
}
