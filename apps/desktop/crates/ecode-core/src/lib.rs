//! eCode (#EXP-1612, spec #EXP-1613): git and CI as one library. This crate
//! is its pure core, a port of `packages/ecode/src/*.ts` replaying the same
//! fixtures: the forge's PR-tree model, checks, the pipeline file (and the
//! GitHub workflow reader into it), runner scheduling and remotes, and the
//! merge train's resolver (`train`, a port of merge-train.ts + the TS train
//! engine's strategies.ts). The enums, constants and rules are generated from
//! `packages/ecode/contract.json`; no I/O, no clock, nothing of Exponential.

#[path = "contract.generated.rs"]
mod contract;
mod js;

pub mod checks;
pub mod forge;
pub mod pipeline;
pub mod remote;
pub mod runner;
pub mod train;
pub mod workflow;

pub use checks::*;
pub use contract::*;
pub use forge::*;
pub use pipeline::*;
pub use remote::*;
pub use runner::*;
pub use train::*;
pub use workflow::*;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A refused op: the eCore error code and the contract reason.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    /// `not_found`, `invalid`, `conflict` or `forbidden`.
    pub code: String,
    pub reason: ForgeReason,
}

impl Failure {
    /// The failure a refusal for `reason` reports.
    pub fn new(reason: ForgeReason) -> Self {
        Self { code: reason.code().to_string(), reason }
    }
}

/// One op's outcome: the next state, the events it emitted and its result,
/// or a refusal (`failure`; the state unchanged, no events).
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome<S> {
    pub state: S,
    pub events: Vec<Value>,
    /// `Some` exactly when the op was not refused.
    pub result: Option<Value>,
    pub failure: Option<Failure>,
}

/// A call no fixture can describe: an unknown op or an argument of the wrong
/// kind where the TS reference would throw or store garbage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpError {
    UnknownOp(String),
    InvalidArg(&'static str),
    InvalidGiven(String),
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownOp(op) => write!(f, "unknown op `{op}`"),
            Self::InvalidArg(field) => write!(f, "invalid argument `{field}`"),
            Self::InvalidGiven(why) => write!(f, "invalid state: {why}"),
        }
    }
}

impl std::error::Error for OpError {}
