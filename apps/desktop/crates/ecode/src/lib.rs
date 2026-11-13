//! eCode on devices (#EXP-1615, epic #EXP-1612): git and CI as one library,
//! server-free. `ecode-core` is the pure half (the spec's model, replayed from
//! `packages/ecode/fixtures`); this crate puts the I/O behind five traits so
//! a host plugs in its own transport and storage:
//!
//! - [`Remote`]: where objects go (credentials, fetch, push);
//! - [`Forge`]: the review unit, every op of `forge.json`;
//! - [`Ci`]: pipelines, runs, jobs and logs (feature `runner`: [`run_job`]
//!   runs one job behind a [`Sandbox`], #EXP-1617);
//! - [`Store`]: review and run objects; [`Store::update_ref`] is the ONE
//!   serialization point (compare-and-swap);
//! - [`SecretSource`]: injects secrets into a process, never returns them.
//!
//! [`LocalRepo`] (feature `repo`) is the local git on gitoxide. Providers sit
//! behind features: `local` (a [`LocalForge`] over a file [`Store`] plus
//! [`LocalRepo`]), `github` (REST, a token or the device flow, polling) and
//! `exponential` (credentials from the server's `repositories.credentials`).

pub use ecode_core as core;

mod ci;
mod credentials;
mod error;
mod forge;
mod git;
mod remote;
mod secrets;
mod store;

#[cfg(feature = "repo")]
mod repo;

#[cfg(feature = "local")]
mod local;

#[cfg(feature = "runner")]
mod runner;

#[cfg(feature = "github")]
pub mod github;

#[cfg(feature = "exponential")]
pub mod exponential;

#[cfg(any(feature = "github", feature = "exponential"))]
mod http;

pub use ci::*;
pub use credentials::*;
pub use error::*;
pub use forge::*;
pub use git::{git_available, GitCommand};
pub use remote::*;
pub use secrets::*;
pub use store::*;

#[cfg(feature = "repo")]
pub use repo::*;

#[cfg(feature = "local")]
pub use local::*;

#[cfg(feature = "runner")]
pub use runner::*;
