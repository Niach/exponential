//! `Forge`: the review unit. Every op of `packages/ecode/fixtures/forge.json`
//! (its `$comment` is the spec), with the same refusals; events are read by
//! polling ([`Forge::events`]), the one mechanism a server-free host has
//! (no webhooks without a server, #RUST-9).

use std::collections::BTreeMap;

use ecode_core::{supports, Capability, Failure, Mergeability, Provider, PullRequest};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// What a push lands on the branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PushSource {
    /// An existing commit.
    Sha(String),
    /// A new commit on `parent` (`None` = a root) with per-path changes:
    /// bytes, or `None` = removed.
    Commit { parent: Option<String>, changes: BTreeMap<String, Option<Vec<u8>>>, message: String },
}

/// `pr.open`'s arguments.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpenPr {
    pub head: String,
    /// ANY branch: a PR's base need not be the default branch.
    pub base: String,
    pub title: String,
    pub draft: bool,
}

/// `pr.mergeability`'s result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergeCheck {
    pub mergeability: Mergeability,
    /// The paths both sides changed differently, in path order.
    pub conflicts: Vec<String>,
}

/// How a path changed (`pr.files`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    Added,
    Modified,
    Removed,
}

/// One changed path.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub change: Change,
}

/// One landed PR of an ordered merge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Merged {
    pub number: i64,
    pub sha: String,
}

/// The first refusal of an ordered merge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergeFailed {
    pub number: i64,
    pub failure: Failure,
}

/// `pr.merge_ordered`'s result: what landed, and where it stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderedMerge {
    pub merged: Vec<Merged>,
    pub failed: Option<MergeFailed>,
}

/// What a forge emits (contract `forgeEvent`; the fixtures' `events`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Pushed {
        branch: String,
        sha: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        forced: bool,
    },
    BranchDeleted {
        branch: String,
    },
    PrOpened {
        number: i64,
    },
    PrRetargeted {
        number: i64,
        from: String,
        to: String,
    },
    PrMerged {
        number: i64,
        sha: String,
    },
    PrClosed {
        number: i64,
    },
    ChecksChanged {
        sha: String,
    },
}

/// A page of events and the cursor to poll from next.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Events {
    pub events: Vec<Event>,
    pub cursor: String,
}

/// The review unit: every `forge.json` op.
pub trait Forge {
    /// Which capability table applies (`forge.json` `capabilities`).
    fn provider(&self) -> Provider;

    /// Whether the provider offers `capability`.
    fn capabilities(&self, capability: Capability) -> bool {
        supports(self.provider(), capability)
    }

    /// The branch's tip, `None` = no such branch.
    fn branch(&self, name: &str) -> Result<Option<String>>;
    /// A PR by number.
    fn pr(&self, number: i64) -> Result<Option<PullRequest>>;

    /// `push`: fast-forward unless `force`.
    fn push(&self, branch: &str, source: PushSource, force: bool) -> Result<String>;
    /// `pr.open`: returns the new PR's number.
    fn open(&self, pr: OpenPr) -> Result<i64>;
    /// `pr.list_by_base`: the live PRs on `base`, by number.
    fn list_by_base(&self, base: &str) -> Result<Vec<i64>>;
    /// `pr.retarget`: never closes a loop.
    fn retarget(&self, number: i64, base: &str) -> Result<()>;
    /// `pr.mergeability`.
    fn mergeability(&self, number: i64) -> Result<MergeCheck>;
    /// `pr.squash_merge`: one commit on the base; the children move onto it,
    /// the head branch goes. `expected_head` guards against a moved head.
    fn squash_merge(&self, number: i64, expected_head: Option<&str>) -> Result<String>;
    /// `pr.close`: the branch and the children stay.
    fn close(&self, number: i64) -> Result<()>;
    /// `pr.files`: the head against the merge base, in path order.
    fn files(&self, number: i64) -> Result<Vec<FileChange>>;
    /// `branch.rebase_onto`: replays the commits after `upstream` onto `onto`.
    fn rebase_onto(&self, branch: &str, onto: &str, upstream: &str) -> Result<String>;
    /// Events after `cursor` (`None` = from the start).
    fn events(&self, cursor: Option<&str>) -> Result<Events>;

    /// `pr.merge_ordered`: squash-merges in order, each its own transaction,
    /// stopping at the first refusal (earlier merges stay landed).
    fn merge_ordered(&self, numbers: &[i64]) -> Result<OrderedMerge> {
        for (i, n) in numbers.iter().enumerate() {
            if numbers[..i].contains(n) {
                return Err(Error::refused(ecode_core::ForgeReason::Duplicate));
            }
        }
        for &number in numbers {
            match self.pr(number)? {
                None => return Err(Error::refused(ecode_core::ForgeReason::Pr)),
                Some(pr) if !pr.state.is_live() => return Err(Error::refused(ecode_core::ForgeReason::NotOpen)),
                Some(_) => {}
            }
        }
        let mut merged = vec![];
        for &number in numbers {
            match self.squash_merge(number, None) {
                Ok(sha) => merged.push(Merged { number, sha }),
                Err(Error::Refused(failure)) => {
                    return Ok(OrderedMerge { merged, failed: Some(MergeFailed { number, failure }) })
                }
                Err(other) => return Err(other),
            }
        }
        Ok(OrderedMerge { merged, failed: None })
    }
}

/// Would basing a PR with head `head` on `base` close a loop of live PRs?
/// (`forge.json`: `pr.open` and `pr.retarget` refuse `cycle`.)
pub fn closes_loop(prs: &[PullRequest], head: &str, base: &str) -> bool {
    let mut cursor = base;
    for _ in 0..=prs.len() {
        if cursor == head {
            return true;
        }
        match prs.iter().find(|pr| pr.state.is_live() && pr.head == cursor) {
            Some(parent) => cursor = &parent.base,
            None => return false,
        }
    }
    true
}
