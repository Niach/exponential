//! `enet1…` tickets: how a node is shared as a QR code or a link.

use std::{
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    str::FromStr,
};

use iroh::{EndpointAddr, EndpointId, RelayUrl, TransportAddr};

use crate::{
    error::{NetError, Result},
    util::Reader,
};

/// The textual prefix; the trailing `1` is the format version.
pub const TICKET_PREFIX: &str = "enet1";

/// A node's address card: its endpoint id, home relay(s) and direct
/// addresses. Display = `enet1` + lowercase unpadded RFC 4648 base32 of
/// ```text
/// [32] endpoint id (ed25519 public key)
/// u8   relay count, then per relay: u8 len, UTF-8 URL
/// u8   addr count,  then per addr:  u8 family (4|6), 4|16 IP bytes, u16 BE port
/// ```
/// Parsing is case-insensitive, so the all-uppercase form fits a QR code's
/// alphanumeric mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub endpoint_id: EndpointId,
    pub relays: Vec<RelayUrl>,
    pub addrs: Vec<SocketAddr>,
}

impl Ticket {
    pub fn new(endpoint_id: EndpointId) -> Self {
        Self { endpoint_id, relays: Vec::new(), addrs: Vec::new() }
    }

    pub fn from_addr(addr: &EndpointAddr) -> Self {
        Self {
            endpoint_id: addr.id,
            relays: addr.relay_urls().cloned().collect(),
            addrs: addr.ip_addrs().copied().collect(),
        }
    }

    pub fn to_addr(&self) -> EndpointAddr {
        EndpointAddr::from_parts(
            self.endpoint_id,
            self.relays
                .iter()
                .cloned()
                .map(TransportAddr::Relay)
                .chain(self.addrs.iter().copied().map(TransportAddr::Ip)),
        )
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        out.extend_from_slice(self.endpoint_id.as_bytes());
        let relays: Vec<String> = self
            .relays
            .iter()
            .map(|r| r.to_string())
            .filter(|s| s.len() <= u8::MAX as usize)
            .take(u8::MAX as usize)
            .collect();
        out.push(relays.len() as u8);
        for r in &relays {
            out.push(r.len() as u8);
            out.extend_from_slice(r.as_bytes());
        }
        let addrs = &self.addrs[..self.addrs.len().min(u8::MAX as usize)];
        out.push(addrs.len() as u8);
        for a in addrs {
            match a.ip() {
                IpAddr::V4(ip) => {
                    out.push(4);
                    out.extend_from_slice(&ip.octets());
                }
                IpAddr::V6(ip) => {
                    out.push(6);
                    out.extend_from_slice(&ip.octets());
                }
            }
            out.extend_from_slice(&a.port().to_be_bytes());
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let bad = |m: &str| NetError::Ticket(m.to_string());
        let mut r = Reader::new(bytes);
        let id = EndpointId::from_bytes(&r.array::<32>()?).map_err(|_| bad("bad endpoint id"))?;
        let mut relays = Vec::new();
        for _ in 0..r.u8()? {
            let len = r.u8()? as usize;
            let s = std::str::from_utf8(r.take(len)?).map_err(|_| bad("relay url not UTF-8"))?;
            relays.push(RelayUrl::from_str(s).map_err(|_| bad("bad relay url"))?);
        }
        let mut addrs = Vec::new();
        for _ in 0..r.u8()? {
            let ip = match r.u8()? {
                4 => IpAddr::V4(Ipv4Addr::from(r.array::<4>()?)),
                6 => IpAddr::V6(Ipv6Addr::from(r.array::<16>()?)),
                _ => return Err(bad("bad address family")),
            };
            addrs.push(SocketAddr::new(ip, r.u16()?));
        }
        if !r.is_empty() {
            return Err(bad("trailing bytes"));
        }
        Ok(Self { endpoint_id: id, relays, addrs })
    }
}

impl fmt::Display for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let body = data_encoding::BASE32_NOPAD.encode(&self.to_bytes()).to_ascii_lowercase();
        write!(f, "{TICKET_PREFIX}{body}")
    }
}

impl FromStr for Ticket {
    type Err = NetError;
    fn from_str(s: &str) -> Result<Self> {
        let s = s.trim();
        if s.len() < TICKET_PREFIX.len() || !s[..TICKET_PREFIX.len()].eq_ignore_ascii_case(TICKET_PREFIX) {
            return Err(NetError::Ticket(format!("missing the `{TICKET_PREFIX}` prefix")));
        }
        let body = s[TICKET_PREFIX.len()..].to_ascii_uppercase();
        let bytes = data_encoding::BASE32_NOPAD
            .decode(body.as_bytes())
            .map_err(|e| NetError::Ticket(e.to_string()))?;
        Self::from_bytes(&bytes)
    }
}
