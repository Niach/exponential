//! eCode's checks (`fixtures/checks.json` is the spec): the rollup over a
//! PR's check rows and the merge gate the required checks give.

use serde::{Deserialize, Serialize};

use crate::{CheckConclusion, ChecksState, MergeGate};

/// One CI job's result at a PR's head.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRow {
    pub name: String,
    /// A contract `CheckStatus` wire value.
    pub status: String,
    /// A contract `CheckConclusion` wire value, null until completed.
    pub conclusion: Option<String>,
}

impl CheckRow {
    fn completed(&self) -> bool {
        self.status == "completed"
    }

    /// Completed with a red conclusion.
    pub fn is_failing(&self) -> bool {
        self.completed()
            && self.conclusion.as_deref().and_then(CheckConclusion::parse).is_some_and(CheckConclusion::is_failing)
    }
}

/// none (no rows) · failure (any red; wins over pending) · pending (any not completed) · success.
pub fn rollup(checks: &[CheckRow]) -> ChecksState {
    if checks.is_empty() {
        ChecksState::None
    } else if checks.iter().any(CheckRow::is_failing) {
        ChecksState::Failure
    } else if checks.iter().any(|check| !check.completed()) {
        ChecksState::Pending
    } else {
        ChecksState::Success
    }
}

/// The merge gate and the names behind it, in row order (missing in required order).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateResult {
    pub gate: MergeGate,
    pub failing: Vec<String>,
    pub pending: Vec<String>,
    pub missing: Vec<String>,
}

/// The merge gate over the REQUIRED names (none named = every row gates).
/// A required name without a row is missing (= waiting).
pub fn merge_gate(checks: &[CheckRow], required: &[String]) -> GateResult {
    let mut names: Vec<&String> = vec![];
    for name in required {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let gated: Vec<&CheckRow> =
        checks.iter().filter(|check| names.is_empty() || names.contains(&&check.name)).collect();
    let failing: Vec<String> = gated.iter().filter(|c| c.is_failing()).map(|c| c.name.clone()).collect();
    let pending: Vec<String> = gated.iter().filter(|c| !c.completed()).map(|c| c.name.clone()).collect();
    let missing: Vec<String> =
        names.iter().filter(|name| !checks.iter().any(|c| &&c.name == *name)).map(|n| (*n).clone()).collect();
    let gate = if !failing.is_empty() {
        MergeGate::Blocked
    } else if names.is_empty() && gated.is_empty() {
        MergeGate::NoChecks
    } else if !pending.is_empty() || !missing.is_empty() {
        MergeGate::Waiting
    } else {
        MergeGate::Ready
    };
    GateResult { gate, failing, pending, missing }
}

/// A job's check name: its name, plus the matrix values in parentheses.
pub fn check_name<S: AsRef<str>>(name: &str, matrix: &[S]) -> String {
    if matrix.is_empty() {
        name.to_string()
    } else {
        let values: Vec<&str> = matrix.iter().map(AsRef::as_ref).collect();
        format!("{name} ({})", values.join(", "))
    }
}
