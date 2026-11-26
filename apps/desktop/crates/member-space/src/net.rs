//! The space's way out (see the crate doc): which addresses a space may
//! reach, and (Linux) the user-space end of its TUN device.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// The space's own address, its resolver, and the instance's stand-in (an
/// instance the host reaches on an address the space may not).
pub const SPACE_ADDR: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 1);
pub const DNS_ADDR: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 2);
pub const INSTANCE_ADDR: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 3);

/// One of the host's own addresses and the prefix of its link (a /32 or
/// /128 has no neighbours).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Own {
    pub ip: IpAddr,
    pub prefix: u8,
}

impl Own {
    fn covers(&self, ip: IpAddr) -> bool {
        match (self.ip, ip) {
            (IpAddr::V4(own), IpAddr::V4(ip)) => {
                let shift = 32 - u32::from(self.prefix.min(32));
                (u64::from(u32::from(own)) >> shift) == (u64::from(u32::from(ip)) >> shift)
            }
            (IpAddr::V6(own), IpAddr::V6(ip)) => {
                let shift = 128 - u32::from(self.prefix.min(128));
                u128::from(own).checked_shr(shift) == u128::from(ip).checked_shr(shift)
            }
            _ => false,
        }
    }
}

/// Cloud metadata and host services that answer on addresses no other rule
/// covers (169.254.169.254, 100.100.100.200, 192.0.0.192 and fd00:ec2::254
/// lie in ranges that are never reachable anyway).
const METADATA: [Ipv4Addr; 1] = [Ipv4Addr::new(168, 63, 129, 16)];

/// Whether a space may open a connection to `ip`: public unicast addresses
/// that are neither the host's own nor on one of its links (public ones
/// included), nor a cloud's metadata service. The resolver and the instance
/// are decided before this ([`stack`]): the instance's exception is ONE
/// address and port, the ones the host itself resolves its URL to, TCP only.
pub fn allowed(ip: IpAddr, own: &[Own]) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    };
    if own.iter().any(|own| own.covers(ip)) || matches!(ip, IpAddr::V4(v4) if METADATA.contains(&v4)) {
        return false;
    }
    match ip {
        IpAddr::V4(v4) => public_v4(v4),
        IpAddr::V6(v6) => public_v6(v6),
    }
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..128).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..32).contains(&b))
        || (a == 192 && b == 168)
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 192 && b == 88 && c == 99)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

/// Global unicast (`2000::/3`) without the ranges that embed or tunnel an
/// IPv4 address (6to4, Teredo) and documentation.
fn public_v6(ip: Ipv6Addr) -> bool {
    let s = ip.segments();
    (s[0] & 0xe000) == 0x2000 && s[0] != 0x2002 && !(s[0] == 0x2001 && (s[1] == 0 || s[1] == 0xdb8))
}

// ── DNS ───────────────────────────────────────────────────────────────────
//
// A space asks the host's resolver, which may know names only the host's
// network has. It is asked for addresses alone, of names that are not
// local ones, and an answer with an address a space may not reach is none.

/// What becomes of a space's DNS query.
#[derive(Debug, PartialEq, Eq)]
pub enum Query {
    Ask,
    /// Answered here with this response code (no records).
    Refuse(u8),
}

const NXDOMAIN: u8 = 3;
const NOTIMP: u8 = 4;
const FORMERR: u8 = 1;

/// Reads one name at `at` (compression followed, bounded); the lowercase
/// labels and where the name ends in place.
fn dns_name(packet: &[u8], mut at: usize) -> Option<(Vec<String>, usize)> {
    let (mut labels, mut end, mut jumps) = (Vec::new(), None, 0);
    loop {
        let len = *packet.get(at)? as usize;
        match len {
            0 => return Some((labels, end.unwrap_or(at + 1))),
            1..=63 => {
                labels.push(String::from_utf8_lossy(packet.get(at + 1..at + 1 + len)?).to_ascii_lowercase());
                at += 1 + len;
            }
            0xc0..=0xff => {
                jumps += 1;
                if jumps > 16 {
                    return None;
                }
                end = end.or(Some(at + 2));
                at = (len & 0x3f) << 8 | *packet.get(at + 1)? as usize;
            }
            _ => return None,
        }
        if labels.len() > 128 {
            return None;
        }
    }
}

fn u16_at(packet: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*packet.get(at)?, *packet.get(at + 1)?]))
}

/// Names a network keeps for itself.
fn local_name(labels: &[String]) -> bool {
    const LOCAL: [&str; 14] = [
        "local", "lan", "internal", "intranet", "private", "home", "corp", "localdomain", "localhost", "test", "invalid",
        "onion", "box", "fritz",
    ];
    let last = labels.last().map(String::as_str).unwrap_or_default();
    labels.len() < 2 || LOCAL.contains(&last) || (last == "arpa")
}

pub fn dns_query(packet: &[u8], instance: Option<&str>) -> Query {
    let one_question = u16_at(packet, 4) == Some(1);
    let Some((labels, end)) = dns_name(packet, 12).filter(|_| one_question) else { return Query::Refuse(FORMERR) };
    let its_instance = instance.is_some_and(|host| labels.join(".") == host.to_ascii_lowercase());
    if local_name(&labels) && !its_instance {
        return Query::Refuse(NXDOMAIN);
    }
    match u16_at(packet, end) {
        Some(1 | 28) => Query::Ask,
        Some(_) => Query::Refuse(NOTIMP),
        None => Query::Refuse(FORMERR),
    }
}

/// The query as an answer without records.
pub fn dns_refusal(query: &[u8], code: u8) -> Vec<u8> {
    let mut out = query.get(..12).map(<[u8]>::to_vec).unwrap_or_default();
    if out.len() == 12 {
        out[2] = 0x80 | (out[2] & 0x79);
        out[3] = 0x80 | code;
        for byte in &mut out[6..12] {
            *byte = 0;
        }
        // The question is repeated when it can be read.
        match dns_name(query, 12) {
            Some((_, end)) if query.len() >= end + 4 => out.extend_from_slice(&query[12..end + 4]),
            _ => out[5] = 0,
        }
    }
    out
}

/// Whether every address of an answer is one `reach` allows (an answer that
/// cannot be read is not).
pub fn dns_answer_ok(packet: &[u8], reach: &dyn Fn(IpAddr) -> bool) -> bool {
    let read = || -> Option<bool> {
        let (questions, answers) = (u16_at(packet, 4)?, u16_at(packet, 6)?);
        let mut at = 12;
        for _ in 0..questions {
            at = dns_name(packet, at)?.1 + 4;
        }
        for _ in 0..answers {
            at = dns_name(packet, at)?.1;
            let (kind, len) = (u16_at(packet, at)?, usize::from(u16_at(packet, at + 8)?));
            let data = packet.get(at + 10..at + 10 + len)?;
            let ip = match (kind, len) {
                (1, 4) => Some(IpAddr::from(<[u8; 4]>::try_from(data).ok()?)),
                (28, 16) => Some(IpAddr::from(<[u8; 16]>::try_from(data).ok()?)),
                (1 | 28, _) => return None,
                _ => None,
            };
            if ip.is_some_and(|ip| !reach(ip)) {
                return Some(false);
            }
            at += 10 + len;
        }
        Some(true)
    };
    read().unwrap_or(false)
}

/// The first nameserver of a `resolv.conf`.
pub fn nameserver(resolv_conf: &str) -> Option<IpAddr> {
    resolv_conf.lines().find_map(|line| {
        let mut words = line.split_whitespace();
        (words.next() == Some("nameserver")).then(|| words.next()?.split('%').next()?.parse().ok())?
    })
}

#[cfg(target_os = "linux")]
pub(crate) mod stack {
    use super::*;
    use std::io;
    use std::net::SocketAddr;
    use std::os::fd::{AsRawFd, OwnedFd};
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll};
    use std::time::Duration;

    use tokio::io::unix::AsyncFd;
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
    use tokio::net::{TcpStream, UdpSocket};
    use tokio::sync::Semaphore;

    pub const MTU: u16 = 1400;
    /// Open connections of one space.
    const MAX_FLOWS: usize = 2048;

    /// The TUN descriptor as a byte device: one read or write is one packet.
    struct Tun(AsyncFd<OwnedFd>);

    impl AsyncRead for Tun {
        fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
            loop {
                let mut guard = match self.0.poll_read_ready(cx) {
                    Poll::Ready(guard) => guard?,
                    Poll::Pending => return Poll::Pending,
                };
                let target = buf.initialize_unfilled();
                let got = guard.try_io(|fd| {
                    let n = unsafe { libc::read(fd.as_raw_fd(), target.as_mut_ptr().cast(), target.len()) };
                    if n < 0 { Err(io::Error::last_os_error()) } else { Ok(n as usize) }
                });
                match got {
                    Ok(Ok(n)) => {
                        buf.advance(n);
                        return Poll::Ready(Ok(()));
                    }
                    Ok(Err(err)) => return Poll::Ready(Err(err)),
                    Err(_) => continue,
                }
            }
        }
    }

    impl AsyncWrite for Tun {
        fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
            loop {
                let mut guard = match self.0.poll_write_ready(cx) {
                    Poll::Ready(guard) => guard?,
                    Poll::Pending => return Poll::Pending,
                };
                let put = guard.try_io(|fd| {
                    let n = unsafe { libc::write(fd.as_raw_fd(), buf.as_ptr().cast(), buf.len()) };
                    if n < 0 { Err(io::Error::last_os_error()) } else { Ok(n as usize) }
                });
                match put {
                    Ok(result) => return Poll::Ready(result),
                    Err(_) => continue,
                }
            }
        }
        fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    struct Policy {
        instance: Option<(String, u16)>,
        /// Nothing but these and the instance ([`crate::Spec::only`]).
        only: Option<Vec<(String, u16)>>,
        /// What [`Self::only`] resolved to on the host at the start.
        pinned: Vec<SocketAddr>,
    }

    /// Where a flow to `dst` goes on the host, or nowhere.
    enum Route {
        To(Vec<SocketAddr>),
        Refused,
    }

    impl Policy {
        async fn instance_addrs(&self) -> Vec<SocketAddr> {
            let Some((host, port)) = &self.instance else { return Vec::new() };
            match tokio::net::lookup_host((host.as_str(), *port)).await {
                Ok(addrs) => addrs.collect(),
                Err(_) => Vec::new(),
            }
        }

        /// A DNS query of the space, answered: asked of the host's resolver,
        /// its answer withheld when it names an address out of reach.
        async fn resolve(&self, query: &[u8]) -> Option<Vec<u8>> {
            // With `only`, the space's hosts file names what there is.
            let refusal = if self.only.is_some() {
                Some(3)
            } else {
                match dns_query(query, self.instance.as_ref().map(|(host, _)| host.as_str())) {
                    Query::Ask => None,
                    Query::Refuse(code) => Some(code),
                }
            };
            if let Some(code) = refusal {
                return Some(dns_refusal(query, code));
            }
            let server = std::fs::read_to_string("/etc/resolv.conf").ok().and_then(|text| nameserver(&text))?;
            let local: SocketAddr =
                if server.is_ipv4() { (Ipv4Addr::UNSPECIFIED, 0).into() } else { (Ipv6Addr::UNSPECIFIED, 0).into() };
            let socket = UdpSocket::bind(local).await.ok()?;
            socket.connect((server, 53)).await.ok()?;
            socket.send(query).await.ok()?;
            let mut answer = vec![0u8; 65536];
            let n = tokio::time::timeout(Duration::from_secs(10), socket.recv(&mut answer)).await.ok()?.ok()?;
            answer.truncate(n);
            let own = own_addresses();
            let instance: Vec<IpAddr> = self.instance_addrs().await.iter().map(SocketAddr::ip).collect();
            let ok = dns_answer_ok(&answer, &|ip| instance.contains(&ip) || allowed(ip, &own));
            Some(if ok { answer } else { dns_refusal(query, 3) })
        }

        async fn route(&self, dst: SocketAddr, udp: bool) -> Route {
            if dst.ip() == IpAddr::V4(DNS_ADDR) {
                // Read here, never forwarded as it is.
                return Route::Refused;
            }
            if !udp && self.instance.as_ref().is_some_and(|(_, port)| *port == dst.port()) {
                let addrs = self.instance_addrs().await;
                if dst.ip() == IpAddr::V4(INSTANCE_ADDR) && !addrs.is_empty() {
                    return Route::To(addrs);
                }
                if addrs.iter().any(|addr| addr.ip() == dst.ip()) {
                    return Route::To(vec![dst]);
                }
            }
            if self.only.is_some() {
                return if !udp && self.pinned.contains(&dst) { Route::To(vec![dst]) } else { Route::Refused };
            }
            if allowed(dst.ip(), &own_addresses()) { Route::To(vec![dst]) } else { Route::Refused }
        }
    }

    fn prefix(mask: *const libc::sockaddr) -> Option<u8> {
        unsafe {
            if mask.is_null() {
                return None;
            }
            match i32::from((*mask).sa_family) {
                libc::AF_INET => Some((*mask.cast::<libc::sockaddr_in>()).sin_addr.s_addr.count_ones() as u8),
                libc::AF_INET6 => {
                    Some((*mask.cast::<libc::sockaddr_in6>()).sin6_addr.s6_addr.iter().map(|byte| byte.count_ones()).sum::<u32>() as u8)
                }
                _ => None,
            }
        }
    }

    /// Every address of the host itself and the link it is on, now.
    fn own_addresses() -> Vec<Own> {
        let mut out = Vec::new();
        unsafe {
            let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
            if libc::getifaddrs(&mut list) != 0 {
                return out;
            }
            let mut at = list;
            while !at.is_null() {
                let addr = (*at).ifa_addr;
                if !addr.is_null() {
                    match i32::from((*addr).sa_family) {
                        libc::AF_INET => {
                            let v4 = &*(addr.cast::<libc::sockaddr_in>());
                            let ip = IpAddr::V4(Ipv4Addr::from(u32::from_be(v4.sin_addr.s_addr)));
                            out.push(Own { ip, prefix: prefix((*at).ifa_netmask).unwrap_or(32) });
                        }
                        libc::AF_INET6 => {
                            let v6 = &*(addr.cast::<libc::sockaddr_in6>());
                            let ip = IpAddr::V6(Ipv6Addr::from(v6.sin6_addr.s6_addr));
                            out.push(Own { ip, prefix: prefix((*at).ifa_netmask).unwrap_or(128) });
                        }
                        _ => {}
                    }
                }
                at = (*at).ifa_next;
            }
            libc::freeifaddrs(list);
        }
        out
    }

    /// Terminates the space's TUN device for as long as it lives.
    pub async fn serve(tun: OwnedFd, instance: Option<(String, u16)>, only: Option<(Vec<(String, u16)>, Vec<SocketAddr>)>) -> io::Result<()> {
        let flags = unsafe { libc::fcntl(tun.as_raw_fd(), libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(tun.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let (only, pinned) = only.map_or((None, Vec::new()), |(only, pinned)| (Some(only), pinned));
        let policy = Arc::new(Policy { instance, only, pinned });
        let flows = Arc::new(Semaphore::new(MAX_FLOWS));
        let mut config = ipstack::IpStackConfig::default();
        config.mtu_unchecked(MTU);
        config.udp_timeout(Duration::from_secs(120));
        let mut stack = ipstack::IpStack::new(config, Tun(AsyncFd::new(tun)?));
        loop {
            let stream = match stack.accept().await {
                Ok(stream) => stream,
                Err(_) => return Ok(()),
            };
            // Over the cap: the flow is dropped (a reset, a lost datagram).
            let Ok(permit) = Arc::clone(&flows).try_acquire_owned() else { continue };
            let policy = Arc::clone(&policy);
            match stream {
                ipstack::IpStackStream::Tcp(mut tcp) => {
                    tokio::spawn(async move {
                        let _permit = permit;
                        let Route::To(addrs) = policy.route(tcp.peer_addr(), false).await else {
                            let _ = tcp.shutdown().await;
                            return;
                        };
                        let connect = TcpStream::connect(addrs.as_slice());
                        if let Ok(Ok(mut out)) = tokio::time::timeout(Duration::from_secs(20), connect).await {
                            let _ = out.set_nodelay(true);
                            let _ = tokio::io::copy_bidirectional(&mut tcp, &mut out).await;
                        }
                        let _ = tcp.shutdown().await;
                    });
                }
                ipstack::IpStackStream::Udp(mut udp) => {
                    tokio::spawn(async move {
                        let _permit = permit;
                        if udp.peer_addr() == SocketAddr::from((DNS_ADDR, 53)) {
                            let mut query = vec![0u8; 65536];
                            while let Ok(n) = udp.read(&mut query).await {
                                if n == 0 {
                                    return;
                                }
                                if let Some(answer) = policy.resolve(&query[..n]).await {
                                    if udp.write_all(&answer).await.is_err() {
                                        return;
                                    }
                                }
                            }
                            return;
                        }
                        let Route::To(addrs) = policy.route(udp.peer_addr(), true).await else { return };
                        let Some(to) = addrs.first().copied() else { return };
                        let local: SocketAddr =
                            if to.is_ipv4() { (Ipv4Addr::UNSPECIFIED, 0).into() } else { (Ipv6Addr::UNSPECIFIED, 0).into() };
                        let Ok(socket) = UdpSocket::bind(local).await else { return };
                        if socket.connect(to).await.is_err() {
                            return;
                        }
                        let (mut up, mut down) = (vec![0u8; 65536], vec![0u8; 65536]);
                        loop {
                            tokio::select! {
                                got = udp.read(&mut up) => match got {
                                    Ok(n) if n > 0 => { let _ = socket.send(&up[..n]).await; }
                                    _ => return,
                                },
                                got = socket.recv(&mut down) => match got {
                                    Ok(n) => if udp.write_all(&down[..n]).await.is_err() { return },
                                    // An ICMP refusal of an earlier datagram.
                                    Err(err) if err.kind() == io::ErrorKind::ConnectionRefused => {}
                                    Err(_) => return,
                                },
                            }
                        }
                    });
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_public_addresses_are_reachable() {
        let at = |ip: &str, prefix| Own { ip: ip.parse().unwrap(), prefix };
        let own = [at("93.184.216.34", 28), at("2a01:4f8::7", 64), at("5.6.7.8", 32), at("127.0.0.1", 8)];
        for (ip, want) in [
            ("1.1.1.1", true),
            ("140.82.121.4", true),
            ("2606:4700::1111", true),
            ("127.0.0.1", false),
            ("127.255.0.9", false),
            ("0.0.0.0", false),
            ("10.0.0.1", false),
            ("172.17.0.1", false),
            ("172.32.0.1", true),
            ("192.168.1.1", false),
            ("100.64.0.1", false),
            ("100.128.0.1", true),
            ("169.254.169.254", false),
            ("198.18.0.9", false),
            ("224.0.0.251", false),
            ("255.255.255.255", false),
            ("::1", false),
            ("::", false),
            ("fe80::1", false),
            ("fd00::1", false),
            ("ff02::fb", false),
            ("::ffff:127.0.0.1", false),
            ("::ffff:192.168.0.1", false),
            ("::ffff:1.1.1.1", true),
            ("::127.0.0.1", false),
            ("64:ff9b::7f00:1", false),
            ("2002:7f00:1::", false),
            // Cloud metadata.
            ("168.63.129.16", false),
            ("100.100.100.200", false),
            ("192.0.0.192", false),
            ("fd00:ec2::254", false),
            // Neighbours on the host's own public links.
            ("93.184.216.33", false),
            ("93.184.216.47", false),
            ("93.184.216.48", true),
            ("2a01:4f8::1", false),
            ("2a01:4f8:0:1::1", true),
            ("5.6.7.8", false),
            ("5.6.7.9", true),
            // The host itself, at its public addresses.
            ("93.184.216.34", false),
            ("::ffff:93.184.216.34", false),
            ("2a01:4f8::7", false),
        ] {
            assert_eq!(allowed(ip.parse().unwrap(), &own), want, "{ip}");
        }
    }

    fn query(name: &str, kind: u16) -> Vec<u8> {
        let mut out = vec![0x12, 0x34, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        for label in name.split('.') {
            out.push(label.len() as u8);
            out.extend(label.as_bytes());
        }
        out.push(0);
        out.extend(kind.to_be_bytes());
        out.extend([0, 1]);
        out
    }

    fn answer(name: &str, records: &[(u16, &[u8])]) -> Vec<u8> {
        let mut out = query(name, 1);
        out[2] = 0x81;
        out[7] = records.len() as u8;
        for (kind, data) in records {
            out.extend([0xc0, 12]);
            out.extend(kind.to_be_bytes());
            out.extend([0, 1, 0, 0, 0, 60]);
            out.extend((data.len() as u16).to_be_bytes());
            out.extend(*data);
        }
        out
    }

    #[test]
    fn dns_answers_public_addresses_of_public_names_alone() {
        for (name, kind, want) in [
            ("github.com", 1, Query::Ask),
            ("github.com", 28, Query::Ask),
            ("github.com", 16, Query::Refuse(NOTIMP)),
            ("github.com", 255, Query::Refuse(NOTIMP)),
            ("nas", 1, Query::Refuse(NXDOMAIN)),
            ("nas.lan", 1, Query::Refuse(NXDOMAIN)),
            ("db.corp", 28, Query::Refuse(NXDOMAIN)),
            ("NAS.LOCAL", 1, Query::Refuse(NXDOMAIN)),
            ("1.0.168.192.in-addr.arpa", 12, Query::Refuse(NXDOMAIN)),
            ("exp.lan", 1, Query::Ask),
        ] {
            assert_eq!(dns_query(&query(name, kind), Some("exp.lan")), want, "{name} {kind}");
        }
        assert_eq!(dns_query(&[1, 2, 3], None), Query::Refuse(FORMERR));
        // A pointer to itself.
        let mut looped = query("a.b", 1);
        looped[12] = 0xc0;
        looped[13] = 12;
        assert_eq!(dns_query(&looped, None), Query::Refuse(FORMERR));

        let reach = |ip: IpAddr| allowed(ip, &[]);
        assert!(dns_answer_ok(&answer("x.com", &[(5, &[0xc0, 12]), (1, &[1, 1, 1, 1])]), &reach));
        assert!(!dns_answer_ok(&answer("x.com", &[(1, &[1, 1, 1, 1]), (1, &[192, 168, 1, 5])]), &reach));
        let mut mapped = [0u8; 16];
        mapped[10..].copy_from_slice(&[0xff, 0xff, 10, 0, 0, 1]);
        assert!(!dns_answer_ok(&answer("x.com", &[(28, &mapped)]), &reach));
        assert!(!dns_answer_ok(&answer("x.com", &[(1, &[1, 1])]), &reach));
        let mut cut = answer("x.com", &[(1, &[1, 1, 1, 1])]);
        cut.truncate(cut.len() - 2);
        assert!(!dns_answer_ok(&cut, &reach));

        let refusal = dns_refusal(&query("nas.lan", 1), NXDOMAIN);
        assert_eq!((refusal[2] & 0x80, refusal[3] & 0xf, refusal[7], refusal.len()), (0x80, 3, 0, query("nas.lan", 1).len()));
        assert_eq!(dns_refusal(&[1], 3), Vec::<u8>::new());
    }

    #[test]
    fn the_first_nameserver() {
        assert_eq!(nameserver("# x\nsearch lan\nnameserver 127.0.0.53\nnameserver 1.1.1.1\n"), Some("127.0.0.53".parse().unwrap()));
        assert_eq!(nameserver("nameserver fe80::1%eth0"), Some("fe80::1".parse().unwrap()));
        assert_eq!(nameserver("options edns0"), None);
    }
}
