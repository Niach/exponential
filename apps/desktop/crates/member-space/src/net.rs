//! The space's way out (see the crate doc): which addresses a space may
//! reach, and (Linux) the user-space end of its TUN device.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// The space's own address, its resolver, and the instance's stand-in (an
/// instance the host reaches on an address the space may not).
pub const SPACE_ADDR: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 1);
pub const DNS_ADDR: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 2);
pub const INSTANCE_ADDR: Ipv4Addr = Ipv4Addr::new(198, 18, 0, 3);

/// Whether a space may open a connection to `ip`: public unicast addresses
/// that are not one of the host's own. (The instance and the resolver are
/// decided before this.)
pub fn allowed(ip: IpAddr, own: &[IpAddr]) -> bool {
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
        v4 => v4,
    };
    if own.contains(&ip) {
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
        dns: Option<IpAddr>,
        instance: Option<(String, u16)>,
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

        async fn route(&self, dst: SocketAddr, udp: bool) -> Route {
            if dst.ip() == IpAddr::V4(DNS_ADDR) {
                return match self.dns {
                    Some(dns) if dst.port() == 53 => Route::To(vec![SocketAddr::new(dns, 53)]),
                    _ => Route::Refused,
                };
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
            if allowed(dst.ip(), &own_addresses()) { Route::To(vec![dst]) } else { Route::Refused }
        }
    }

    /// Every address of the host itself, now.
    fn own_addresses() -> Vec<IpAddr> {
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
                            out.push(IpAddr::V4(Ipv4Addr::from(u32::from_be(v4.sin_addr.s_addr))));
                        }
                        libc::AF_INET6 => {
                            let v6 = &*(addr.cast::<libc::sockaddr_in6>());
                            out.push(IpAddr::V6(Ipv6Addr::from(v6.sin6_addr.s6_addr)));
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
    pub async fn serve(tun: OwnedFd, instance: Option<(String, u16)>) -> io::Result<()> {
        let flags = unsafe { libc::fcntl(tun.as_raw_fd(), libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(tun.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let dns = std::fs::read_to_string("/etc/resolv.conf").ok().and_then(|text| nameserver(&text));
        let policy = Arc::new(Policy { dns, instance });
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
        let own: Vec<IpAddr> = vec!["93.184.216.34".parse().unwrap(), "2a01:4f8::7".parse().unwrap()];
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
            // The host itself, at its public addresses.
            ("93.184.216.34", false),
            ("::ffff:93.184.216.34", false),
            ("2a01:4f8::7", false),
        ] {
            assert_eq!(allowed(ip.parse().unwrap(), &own), want, "{ip}");
        }
    }

    #[test]
    fn the_first_nameserver() {
        assert_eq!(nameserver("# x\nsearch lan\nnameserver 127.0.0.53\nnameserver 1.1.1.1\n"), Some("127.0.0.53".parse().unwrap()));
        assert_eq!(nameserver("nameserver fe80::1%eth0"), Some("fe80::1".parse().unwrap()));
        assert_eq!(nameserver("options edns0"), None);
    }
}
