use thiserror::Error;

/// Every eNet failure, flattened to strings at the iroh boundary so the
/// facades (UniFFI, wasm-bindgen) can carry them as-is.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum NetError {
    #[error("bind failed: {0}")]
    Bind(String),
    #[error("connect to {peer} failed: {reason}")]
    Connect { peer: String, reason: String },
    #[error("transport: {0}")]
    Transport(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("invalid ticket: {0}")]
    Ticket(String),
    #[error("bad signature")]
    BadSignature,
    #[error("crypto: {0}")]
    Crypto(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("refused by peer: {0}")]
    Refused(String),
    #[error("timed out: {0}")]
    Timeout(String),
    #[error("node is shut down")]
    Closed,
}

pub type Result<T, E = NetError> = std::result::Result<T, E>;

pub(crate) fn transport(e: impl std::fmt::Display) -> NetError {
    NetError::Transport(e.to_string())
}
