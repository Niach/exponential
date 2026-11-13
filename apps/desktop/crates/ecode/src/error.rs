//! One error type for every trait: a contract refusal (the spec's
//! `{code, reason}`) or a failure of the I/O underneath.

use std::fmt;

use ecode_core::{Failure, ForgeReason};

/// What went wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The op was refused by the spec (`forge.json`): nothing changed.
    Refused(Failure),
    /// A git command or a gix call failed. `detail` is already redacted.
    Git { op: String, detail: String },
    /// The Store failed (I/O) or a compare-and-swap lost.
    Store(StoreError),
    /// An HTTP provider failed (`status` 0 = no response). Redacted.
    Http { status: u16, detail: String },
    /// The provider does not offer this (its capability flag is off).
    Unsupported(&'static str),
    /// A caller-side mistake no fixture describes (a bad name, a bad path).
    Invalid(String),
}

impl Error {
    /// A refusal for `reason`.
    pub fn refused(reason: ForgeReason) -> Self {
        Self::Refused(Failure::new(reason))
    }

    /// The contract reason, when the op was refused.
    pub fn reason(&self) -> Option<ForgeReason> {
        match self {
            Self::Refused(failure) => Some(failure.reason),
            _ => None,
        }
    }

    pub(crate) fn git(op: impl Into<String>, detail: impl fmt::Display) -> Self {
        Self::Git { op: op.into(), detail: detail.to_string() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(failure) => write!(f, "refused: {} ({})", failure.reason.as_str(), failure.code),
            Self::Git { op, detail } => write!(f, "{op}: {detail}"),
            Self::Store(error) => write!(f, "store: {error}"),
            Self::Http { status, detail } => write!(f, "http {status}: {detail}"),
            Self::Unsupported(what) => write!(f, "unsupported by this provider: {what}"),
            Self::Invalid(why) => write!(f, "invalid: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// A Store failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// `update_ref` lost: the ref holds `actual`, not the expected value.
    Conflict { name: String, actual: Option<String> },
    /// A ref or object name the store refuses.
    InvalidName(String),
    /// The backing storage failed.
    Io(String),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict { name, actual } => {
                write!(f, "ref `{name}` moved (now {})", actual.as_deref().unwrap_or("absent"))
            }
            Self::InvalidName(name) => write!(f, "invalid name `{name}`"),
            Self::Io(detail) => f.write_str(detail),
        }
    }
}

impl std::error::Error for StoreError {}

impl From<StoreError> for Error {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// Every trait's result.
pub type Result<T, E = Error> = std::result::Result<T, E>;
