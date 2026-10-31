//! eMCP for Exponential's devices and spezl (#EXP-1492, epic VAPP-175), on
//! rmcp, the official Rust MCP SDK:
//!
//! - [`server`]: a loopback Streamable HTTP server (`127.0.0.1:<port>/mcp`,
//!   stateless, JSON answers) behind the [`server::ToolProvider`] trait, each
//!   run admitted by its own bearer token. codemode serves its tools
//!   through it (ePilot and ePilot screen next, through the release tag).
//! - [`client`]: a blocking client (headers resolved by the host) for the
//!   hosts' upstream calls: codemode's scripts, the CLI's and the IDE's
//!   calls into the instance's own `/api/mcp`.
//! - [`oauth`]: the same client authorized with OAuth, its credentials in a
//!   [`oauth::Vault`] the host implements (keychain or server-backed).
//! - [`hub`]: the LOCAL HUB (#EXP-1507): several MCP servers (upstreams
//!   and in-process providers) behind ONE loopback endpoint, each run
//!   granted a level per server and every call written as an `mcp_calls`
//!   audit row (`packages/emcp/fixtures/hub.json`).
//! - `codemode` (feature `codemode`): code mode, `exec` + `describe` over
//!   a run's other MCP servers (moved in from `crates/codemode`).
//! - [`contract`]: the eMCP contract of `packages/emcp` (exposes, access,
//!   audit, vault, upstream), replayed case by case from its fixtures: the
//!   app tool broker's rules, the same as the server broker's.
//!
//! Everything here is blocking from the host's side: the SDK runs on emcp's
//! own runtime threads, never under a host's executor.

pub mod client;
#[cfg(feature = "codemode")]
pub mod codemode;
pub mod contract;
pub mod hub;
pub mod oauth;
mod runtime;
pub mod server;

/// eCore, the types emcp's API speaks (#EXP-1517): a host names them
/// through emcp and needs no second pin.
pub use ecore;
pub use ecore::{DeliveryVerdict, ErrorCode, Failure, SessionId, Token};
pub use client::{Client, ClientError, Upstream};
pub use server::{Caller, RawRequest, RawResponse, RawRoute, Server, ServerInfo, ToolProvider};
