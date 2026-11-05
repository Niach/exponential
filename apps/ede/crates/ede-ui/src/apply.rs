//! The apply state machine (#EDE-8): what happens between a file landing in
//! the ui directory and the shell drawing it. Pure and byte-locked by
//! `packages/ede/fixtures/apply.json` (its `$comment` is the spec;
//! `tests/contract.rs` replays the cases); the TS reference is
//! `packages/ede/src/apply.ts`. The supervisor ([`crate::supervise`]) feeds
//! it what the watcher, git and the shell report and carries out the
//! effects.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::doc::Issue;

/// The thresholds (`rules` in the fixture).
pub const DEBOUNCE_MS: i64 = 120;
pub const FRAME_ACK_MS: i64 = 2000;
pub const CRASHES_TO_REVERT: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub commit: String,
    pub deadline: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentState {
    pub shown: String,
    pub commits: Vec<String>,
    pub pending: Option<Pending>,
    pub crashes: u32,
    pub rejected: Option<Vec<Issue>>,
    pub reverted: bool,
    /// The newest commit holds the factory document (at start and after a `factory` effect): never a suspect.
    #[serde(rename = "fromFactory")]
    pub from_factory: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApplyState {
    pub documents: BTreeMap<String, DocumentState>,
    /// How many commits the reducer has named (`c<n>`).
    pub commit_count: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The debounced watcher saw the file change and the runtime validated it.
    Edit { at: i64, name: String, valid: bool, issues: Vec<Issue> },
    /// The shell presented a frame of that commit.
    Frame { at: i64, name: String, commit: String },
    /// The shell process died.
    Crash { at: i64 },
    Tick { at: i64 },
}

impl Event {
    pub fn at(&self) -> i64 {
        match self {
            Event::Edit { at, .. } | Event::Frame { at, .. } | Event::Crash { at } | Event::Tick { at } => *at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Effect {
    Commit { name: String, id: String },
    Refuse { name: String, issues: Vec<Issue> },
    Reload { name: String, commit: String },
    Revert { name: String, from: String, to: String, id: String },
    Factory { name: String, id: String },
}

pub fn factory_commit(name: &str) -> String {
    format!("f-{name}")
}

impl ApplyState {
    /// Every factory document at its factory commit.
    pub fn with_factory(names: impl IntoIterator<Item = String>) -> ApplyState {
        let mut state = ApplyState::default();
        for name in names {
            state.documents.insert(name.clone(), DocumentState { shown: factory_commit(&name), commits: vec![factory_commit(&name)], pending: None, crashes: 0, rejected: None, reverted: false, from_factory: true });
        }
        state
    }

    fn next_commit(&mut self) -> String {
        self.commit_count += 1;
        format!("c{}", self.commit_count)
    }

    fn revert(&mut self, name: &str, from: String, to: String, effects: &mut Vec<Effect>) {
        let id = self.next_commit();
        let d = self.documents.get_mut(name).expect("a known document");
        d.commits.push(id.clone());
        d.shown = id.clone();
        d.pending = None;
        d.crashes = 0;
        d.reverted = true;
        effects.push(Effect::Revert { name: name.to_string(), from, to, id: id.clone() });
        effects.push(Effect::Reload { name: name.to_string(), commit: id });
    }
}

/// One step of the machine: the new state and the effects to carry out.
pub fn reduce(previous: &ApplyState, event: &Event) -> (ApplyState, Vec<Effect>) {
    let mut state = previous.clone();
    let mut effects = Vec::new();
    let at = event.at();

    // Rule 0: a commit nobody drew in time rolls back.
    let names: Vec<String> = state.documents.keys().cloned().collect();
    for name in &names {
        let d = &state.documents[name];
        if let Some(p) = &d.pending {
            if p.deadline <= at {
                let (from, to) = (p.commit.clone(), d.shown.clone());
                state.revert(name, from, to, &mut effects);
            }
        }
    }

    match event {
        Event::Edit { name, valid, issues, .. } => {
            if !state.documents.contains_key(name) {
                effects.push(Effect::Refuse { name: name.clone(), issues: vec![Issue::new("file", format!("not a shell document (no factory copy): {name}"))] });
                return (state, effects);
            }
            if !valid {
                let d = state.documents.get_mut(name).unwrap();
                d.rejected = Some(issues.clone());
                effects.push(Effect::Refuse { name: name.clone(), issues: issues.clone() });
                return (state, effects);
            }
            let id = state.next_commit();
            let d = state.documents.get_mut(name).unwrap();
            d.commits.push(id.clone());
            d.pending = Some(Pending { commit: id.clone(), deadline: at + FRAME_ACK_MS });
            d.crashes = 0;
            d.reverted = false;
            d.from_factory = false;
            d.rejected = None;
            effects.push(Effect::Commit { name: name.clone(), id: id.clone() });
            effects.push(Effect::Reload { name: name.clone(), commit: id });
        }
        Event::Frame { name, commit, .. } => {
            if let Some(d) = state.documents.get_mut(name) {
                if d.pending.as_ref().is_some_and(|p| &p.commit == commit) {
                    d.shown = commit.clone();
                    d.pending = None;
                }
            }
        }
        Event::Crash { .. } => {
            for name in &names {
                let d = state.documents.get_mut(name).unwrap();
                if d.from_factory {
                    continue;
                }
                let newest = d.commits.last().cloned().unwrap_or_default();
                d.crashes += 1;
                if d.crashes < CRASHES_TO_REVERT {
                    continue;
                }
                let (reverted, previous) = (d.reverted, d.commits[d.commits.len() - 2].clone());
                if !reverted {
                    state.revert(name, newest, previous, &mut effects);
                } else {
                    let id = state.next_commit();
                    let d = state.documents.get_mut(name).unwrap();
                    d.commits.push(id.clone());
                    d.shown = id.clone();
                    d.pending = None;
                    d.crashes = 0;
                    d.reverted = false;
                    d.from_factory = true;
                    effects.push(Effect::Factory { name: name.clone(), id: id.clone() });
                    effects.push(Effect::Reload { name: name.clone(), commit: id });
                }
            }
        }
        Event::Tick { .. } => {}
    }
    (state, effects)
}
