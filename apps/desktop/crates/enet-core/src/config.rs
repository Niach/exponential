use iroh::{EndpointId, RelayMap, RelayMode, RelayUrl};

use crate::error::{NetError, Result};

/// Exponential's own relay; every node uses it unless configured otherwise.
pub const DEFAULT_RELAY_URL: &str = "https://relay.exponential.at";

/// Which relays the node homes on and dials through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayChoice {
    /// Our own relay(s), by URL (default: [`DEFAULT_RELAY_URL`]).
    Custom(Vec<String>),
    /// No relay at all: direct addresses only (tests, LAN). Not usable in a
    /// browser, which can only reach peers through a relay.
    Disabled,
}

impl Default for RelayChoice {
    fn default() -> Self {
        Self::Custom(vec![DEFAULT_RELAY_URL.to_string()])
    }
}

/// How a [`crate::Node`] is built.
#[derive(Debug, Clone, Default)]
pub struct NetConfig {
    pub relay: RelayChoice,
    /// Also use number 0's public relays (OFF by default: our relay only).
    pub n0_relays: bool,
    /// Auth token presented to the relay(s), if they require one.
    pub relay_auth_token: Option<String>,
    /// Native only: the UDP socket address to bind (default: any, port 0).
    pub bind_addr: Option<std::net::SocketAddr>,
    /// With the `fs-store` feature: persist blobs in this directory
    /// (default: in memory).
    pub blob_dir: Option<std::path::PathBuf>,
    /// Peers allowed to connect from the start (see [`crate::Access`]).
    pub allowed_peers: Vec<EndpointId>,
    /// Accept every peer (pairing windows, public nodes). Default false.
    pub allow_all: bool,
}

impl NetConfig {
    /// Direct addresses only, bound to localhost: what the tests use.
    pub fn local_only() -> Self {
        Self {
            relay: RelayChoice::Disabled,
            bind_addr: Some(([127, 0, 0, 1], 0).into()),
            ..Self::default()
        }
    }

    pub(crate) fn relay_mode(&self) -> Result<RelayMode> {
        let map = match &self.relay {
            RelayChoice::Disabled if !self.n0_relays => return Ok(RelayMode::Disabled),
            RelayChoice::Disabled => RelayMap::empty(),
            RelayChoice::Custom(urls) => {
                let urls = urls
                    .iter()
                    .map(|u| u.parse::<RelayUrl>().map_err(|e| NetError::Invalid(format!("relay url {u}: {e}"))))
                    .collect::<Result<Vec<_>>>()?;
                RelayMap::from_iter(urls)
            }
        };
        if self.n0_relays {
            map.extend(&iroh::endpoint::default_relay_mode().relay_map());
        }
        let map = match &self.relay_auth_token {
            Some(t) => map.with_auth_token(t.clone()),
            None => map,
        };
        if map.is_empty() {
            return Ok(RelayMode::Disabled);
        }
        Ok(RelayMode::Custom(map))
    }
}
