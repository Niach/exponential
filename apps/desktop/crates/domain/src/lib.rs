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
//! * [`tree_guides`] — EXP-965 `guides_for`: the elbow/tee/pass-through
//!   connector a nested list draws over that nesting, ×4;
//! * [`issue_nesting`] — EXP-980 `nest_issue_rows`: sub-issues follow their
//!   parent in EVERY issue list, byte-locked ×4 by
//!   `packages/domain-contract/fixtures/issue-nesting.json`;
//! * [`issue_graph`] — EXP-980's `blocks` graph (per-row counts, a picked
//!   set's outside blockers, the laid-out mini-graph), byte-locked ×4 by
//!   `fixtures/issue-graph.json`;
//! * [`issue_rail`] — EXP-998's blocks RAIL: the git-graph lanes an issue
//!   list draws between its visible rows (web `lib/issue-rail.ts` twin);
//! * [`placeholder_status`] — EXP-630's roster badge: which members are
//!   "invited, not joined" (and whether their link still works) read off the
//!   synced invites (web `lib/placeholder-status.ts`'s twin);
//! * [`image_message`] — the steer/start image-message shape (EXP-511/825),
//!   `lib/steer-image-message.ts`'s byte-identical twin;
//! * [`issue_estimate`] — EXP-630 story points on the TEAM's scale (labels,
//!   picker ladder, the `estimate_changed` phrase), byte-locked ×4 by
//!   `fixtures/issue-estimate.json`;
//! * [`issue_search`] — EXP-892's ONE issue-search engine (rank + server-hit
//!   merge), byte-locked ×4 by
//!   `packages/domain-contract/fixtures/issue-search.json`;
//! * [`diff`] — EXP-895's ONE diff model + parser (`git diff`, bare steer
//!   sections, GitHub patches), byte-locked ×4 by
//!   `packages/domain-contract/fixtures/diff/`;
//! * [`diff_tree`] — EXP-916's file TREE beside a Changes surface (compact
//!   folders, dirs before files, the flat query result), byte-locked ×4 by
//!   `fixtures/diff/tree.json`;
//! * [`edit_card`] — EXP-916's edited-files card and the ONE rule that decides
//!   which tool calls form it, byte-locked ×4 by
//!   `fixtures/feed/edit-cards.json`;
//! * [`activity_fold`] — EXP-900's READ-TIME activity fold (a run of same
//!   actor/field/issue changes collapses to its net effect; "Show all" on the
//!   timeline header returns the raw rows, EXP-468), byte-locked ×4 by
//!   `fixtures/activity-fold.json`;
//! * [`entity_preview`] — EXP-920's entity-chip rule over a settled
//!   Exponential tool row's `preview.refs` (icon concept, noun, clamped
//!   label, `list` grouping), byte-locked ×4 by `fixtures/entity-chip.json`;
//! * [`workflow_view`] — EXP-981's workflow bands, shape line, node captions
//!   and node edges (the server owns the `wave`/`lane` geometry), byte-locked
//!   ×4 by `fixtures/workflow-view.json`.
//!
//! gpui-free — headless-testable.

pub mod contract {
    include!("contract.generated.rs");
}

pub mod activity_fold;
pub mod batch_run;
pub mod board;
pub mod client_version;
pub mod diff;
pub mod diff_tree;
pub mod edit_card;
pub mod entity_preview;
pub mod enums;
pub mod hydrate;
pub mod image_message;
pub mod issue_estimate;
pub mod issue_graph;
pub mod issue_nesting;
pub mod issue_rail;
pub mod issue_search;
pub mod options;
pub mod placeholder_status;
pub mod pr_graph;
pub mod pr_stack;
pub mod relations;
pub mod rows;
pub mod session_results;
pub mod session_tree;
pub mod statuses;
pub mod tree_guides;
// EXP-1072: a workflow's final PR — identifier, picker label, review key.
pub mod workflow_final_pr;
pub mod workflow_view;
// EXP-1082: the open questions of a workflow's live runs (EXP-1065).
pub mod workflow_questions;

pub use enums::{IssuePriority, IssueStatus};
pub use rows::member_fallback_label;
