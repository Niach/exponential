//! eCore (#EXP-1504): the plain types every ExponentialOS module shares.
//! Modules call each other's capabilities over MCP through eMCP; what they
//! pass is spelled here. The enums, constants and rules are generated from
//! `packages/ecore/contract.json` (the same file generates TS, Swift and
//! Kotlin); this file adds the Rust shapes around them.

#[path = "contract.generated.rs"]
mod contract;

pub use contract::*;

use serde::{Deserialize, Serialize};

/// An id that broke the id rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvalidId(pub String);

impl std::fmt::Display for InvalidId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid id `{}`", self.0)
    }
}

impl std::error::Error for InvalidId {}

macro_rules! id {
    ($name:ident, $kind:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub const KIND: IdKind = IdKind::$kind;

            pub fn new(id: impl Into<String>) -> Result<Self, InvalidId> {
                let id = id.into();
                if is_valid_id(&id) { Ok(Self(id)) } else { Err(InvalidId(id)) }
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = InvalidId;
            fn try_from(id: String) -> Result<Self, InvalidId> {
                Self::new(id)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> String {
                id.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id!(SessionId, Session, "A session (a thread, a child, an agent conversation).");
id!(RunId, Run, "One agent run inside a session.");
id!(SpaceId, Space, "An agent space on a device.");
id!(WindowId, Window, "A window in a space.");
id!(DeviceId, Device, "A device (a desktop, a phone, a daemon host).");

/// A grant's bearer token (the token rule). Its `Debug` hides the value.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Token(String);

impl Token {
    pub fn new(token: impl Into<String>) -> Result<Self, InvalidId> {
        let token = token.into();
        if is_valid_token(&token) { Ok(Self(token)) } else { Err(InvalidId(String::from("<token>"))) }
    }

    /// The token from an `Authorization` header.
    pub fn from_header(header: &str) -> Option<Self> {
        parse_bearer(header).map(|token| Self(token.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The `Authorization` header value.
    pub fn header(&self) -> String {
        bearer_header(&self.0)
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(…)")
    }
}

impl TryFrom<String> for Token {
    type Error = InvalidId;
    fn try_from(token: String) -> Result<Self, InvalidId> {
        Self::new(token)
    }
}

impl From<Token> for String {
    fn from(token: Token) -> String {
        token.0
    }
}

/// What a caller holds: a token naming a session (and run), the
/// capabilities it admits and when it ends (unix ms; `None` = until revoked).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Grant {
    pub token: Token,
    pub session_id: SessionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    pub capabilities: Vec<Capability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
}

impl Grant {
    pub fn admits(&self, capability: Capability, now: i64) -> bool {
        grant_admits(&self.capabilities, self.expires_at, now, capability)
    }
}

/// The one refusal/error shape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl Failure {
    /// The message clamped, `retryable` from the code.
    pub fn new(code: ErrorCode, message: &str) -> Self {
        Self { code, message: clamp_message(message), retryable: code.default_retryable() }
    }

    pub fn is_refusal(&self) -> bool {
        self.code.is_refusal()
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for Failure {}

/// The event envelope, the stable core of `wire`. Fields stay plain so an
/// envelope from a newer or broken peer still parses; [`Envelope::errors`]
/// says what is wrong with it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope<T = serde_json::Value> {
    pub v: i64,
    pub id: String,
    pub kind: String,
    pub at: i64,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub data: T,
}

impl<T> Envelope<T> {
    /// A current-version envelope.
    pub fn new(id: &str, kind: &str, at: i64, source: &str, session_id: Option<&SessionId>, data: T) -> Self {
        Self {
            v: ENVELOPE_VERSION as i64,
            id: id.to_string(),
            kind: kind.to_string(),
            at,
            source: source.to_string(),
            session_id: session_id.map(|id| id.as_str().to_string()),
            data,
        }
    }

    /// Every field that breaks a rule (empty = valid).
    pub fn errors(&self) -> Vec<&'static str> {
        envelope_errors(self.v, &self.id, &self.kind, self.at, &self.source, self.session_id.as_deref())
    }
}
