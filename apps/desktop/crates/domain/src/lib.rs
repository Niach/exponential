//! `domain` — canonical enum values + row structs (masterplan-v3 §3.1).
//!
//! `src/contract.generated.rs` (emitted from `@exp/domain-contract`, COMMITTED)
//! carries the canonical enum value lists and per-value consts. This crate
//! layers on top (§5.1/§5.5):
//!
//! * [`enums`] — typed enums with tolerant-unknown deserialization + the
//!   board display orders (locked to the generated contract by test);
//! * [`rows`] — the 16 hand-written shape row structs mirroring
//!   `packages/db-schema`, hydrated from the sync store's snake_case JSON;
//! * [`hydrate`] — the tolerant `string → native` serde helpers (§5.5 — NOT
//!   `BoolFromInt`; Electric booleans surface in many forms);
//! * [`options`] — the status/priority icon+color option tables mirroring
//!   `apps/web/src/lib/domain.ts` (§4.7; presentation as data — glyph names +
//!   color tokens — so this crate stays gpui-free);
//! * [`statuses`] — EXP-314 per-team custom statuses: category, tint, glyph
//!   (the pie-clock parity table) and the resolve chain every surface renders
//!   through;
//! * [`board`] — `apps/web/src/lib/board-view.ts` grouping/sorting;
//! * [`relations`] — EXP-736 relation picks, per-side labels and the two
//!   timeline phrases (locked to the generated contract's label slices);
//! * [`pr_stack`] — EXP-897 PR STACKS: the `pr_base_branch` edge, the
//!   bottom-up chain, the nesting rule and the stack copy (web
//!   `lib/pr-stack.ts`'s twin);
//! * [`pr_graph`] — EXP-897 §4: stack + batch + session tree for ONE subject,
//!   the model behind the work header's badge and its overlay;
//! * [`batch_run`] — EXP-876 `coding_sessions.batch_issue_ids`: which issues
//!   a batch run covers and the `EXP-874 +2` name every list shows for it
//!   (web `lib/batch-run.ts`'s twin);
//! * [`session_results`] — EXP-879 `coding_sessions.results`: the tolerant
//!   reader, the topic grouping and the equal-height tile geometry behind
//!   the Results face (web `lib/session-results.ts`'s twin);
//! * [`session_tree`] — EXP-818 `nest_sessions`: the parent/child nesting
//!   every session list renders (web `lib/session-tree.ts` twin);
//! * [`image_message`] — the steer/start image-message shape (EXP-511/825),
//!   `lib/steer-image-message.ts`'s byte-identical twin;
//! * [`issue_search`] — EXP-892's ONE issue-search engine (rank + server-hit
//!   merge), byte-locked ×4 by
//!   `packages/domain-contract/fixtures/issue-search.json`;
//! * [`diff`] — EXP-895's ONE diff model + parser (`git diff`, bare steer
//!   sections, GitHub patches), byte-locked ×4 by
//!   `packages/domain-contract/fixtures/diff/`.
//!
//! gpui-free — headless-testable.

pub mod contract {
    include!("contract.generated.rs");
}

pub mod batch_run;
pub mod board;
pub mod client_version;
pub mod diff;
pub mod enums;
pub mod hydrate;
pub mod image_message;
pub mod issue_search;
pub mod options;
pub mod pr_graph;
pub mod pr_stack;
pub mod relations;
pub mod rows;
pub mod session_results;
pub mod session_tree;
pub mod statuses;

pub use enums::{IssuePriority, IssueStatus};
pub use rows::member_fallback_label;
