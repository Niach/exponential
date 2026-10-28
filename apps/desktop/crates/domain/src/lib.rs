//! `domain` — canonical enum values + row structs (masterplan-v3 §3.1).
//!
//! `src/contract.generated.rs` (emitted from `@exp/domain-contract`, COMMITTED)
//! carries the canonical enum value lists and per-value consts. This crate
//! layers on top (§5.1/§5.5):
//!
//! * [`enums`] — typed enums with tolerant-unknown deserialization + the
//!   board display orders (locked to the generated contract by test);
//! * [`rows`] — the 25 hand-written shape row structs mirroring
//!   `packages/db-schema`, hydrated from the sync store's snake_case JSON
//!   (EXP-1294: the session tree `SessionRow` + messages + pull requests);
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
//! * [`relations_view`] — EXP-1097's issue-detail relations view (the
//!   "Sub-issue of" line, the Sub-issues section, one foldable band per side),
//!   byte-locked ×4 by `fixtures/issue-relations-view.json`;
//! * [`pr_stack`]: the `pull_requests` base edge and the bottom-up chain
//!   (web `lib/pr-stack.ts`'s twin);
//! * [`pr_graph`]: blockers + batch + stack for ONE subject, the model
//!   behind the work header's related-work badge and its overlay;
//! * [`session_results`] — EXP-879 the Guide reader (`sessions.guide`): the
//!   tolerant reader, the topic grouping and the tile geometry (web
//!   `lib/session-results.ts`'s twin);
//! * [`session_tree`] — EXP-1294 Threads v2: the session card, the thread
//!   screen, the Threads page and the rail's Agents, byte-locked ×4 by
//!   `fixtures/{session-card,session-thread,threads-list}.json`;
//! * [`message_conversation`] — NEW-8: a message belongs to the session of
//!   its conversation (`conversations` shape), byte-locked ×4 by
//!   `fixtures/message-conversation.json`;
//! * [`session_title`] / [`session_kind`] — EXP-1341: THE display title of
//!   any session and the kind traits (`persistent`, `triggers`) every
//!   behaviour branches on, byte-locked ×4 by
//!   `fixtures/{session-title,session-kind}.json`;
//! * [`agent_settings`] — EXP-1294 Team settings → Agents (trigger summary,
//!   form validation), byte-locked ×4 by `fixtures/agent-settings.json`;
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
//! * [`issue_draft`] — EXP-1170's New issue page copy + autosave cadence,
//!   locked ×4 by `fixtures/issue-draft.json`;
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
//! * [`app_link`] — EXP-1188: what a clicked markdown link in agent prose
//!   opens (issue | run in-app, another instance page, external, ignored),
//!   byte-locked ×4 by `fixtures/app-link.json`.
//!
//! gpui-free — headless-testable.

pub mod contract {
    include!("contract.generated.rs");
}

pub mod activity_fold;
// THE ask cards' words (titles, plan options, a thread's waiting cards), fixture-locked ×4.
pub mod ask_dock;
// EXP-1294: Team settings → Agents (trigger summary, validation), fixture-locked ×4.
pub mod agent_settings;
// VAPP-110: Exponential apps (slugify, version label, list order), fixture-locked ×4.
pub mod apps;
// VAPP-120: typed host actions (`exponential.<noun>.<op>`), fixture-locked ×4.
pub mod app_actions;
// VAPP-121: an app as one JSON file (export name + file), fixture-locked ×4.
pub mod app_export;
// VAPP-122: app records (the `exp:records` query + value), fixture-locked ×4.
pub mod app_records;
// VAPP-119: the `exp:` binding sources (issues, boards, members, sessions,
// comments, attachments), fixture-locked ×4.
pub mod app_sources;
// EXP-1188: what a clicked link in agent prose opens, fixture-locked ×4.
pub mod app_link;
pub mod board;
// Thread 58cb32df: Chats (menus, dividers, the surface card's reply rule), fixture-locked ×4.
pub mod chats;
pub mod client_version;
// EXP-1121: the Start coding readiness checklist, fixture-locked ×4.
pub mod coding_readiness;
// EXP-1162: the detail chrome (title collapse + edge strips), fixture-locked ×4.
pub mod detail_chrome;
pub mod diff;
pub mod diff_tree;
// N-12: THE Dock (the cards over the composer, the pile, auto-open), fixture-locked ×4.
pub mod dock;
pub mod edit_card;
pub mod entity_preview;
pub mod enums;
pub mod guide_viewer;
pub mod hydrate;
pub mod image_message;
pub mod issue_draft;
pub mod issue_estimate;
pub mod issue_graph;
pub mod issue_nesting;
pub mod issue_rail;
pub mod issue_search;
// EXP-1248: THE list item (geometry, PR node states), fixture-locked ×4.
pub mod list_item;
// NEW-8: messages are keyed through their conversation, fixture-locked ×4.
pub mod message_conversation;
/// NEW-57: meeting mode (`meeting.json`): settings rows, channels, flush, message.
pub mod meeting;
/// NEW-53: exponentialTalk voice settings + spoken-reply rules (`voice.json`).
pub mod voice;
pub mod voice_call;
/// #NEW-79: the voice agent's tool policy and voice answers (`voice-policy.json`).
pub mod voice_policy;
pub mod options;
// EXP-1280: THE plan / question card (answer text, issues, uploads, tiles), fixture-locked ×4.
pub mod plan_card;
pub mod placeholder_status;
pub mod pr_graph;
pub mod pr_stack;
// EXP-1230: every confirm/choice prompt's words + roles, fixture-locked ×4.
pub mod prompts;
pub mod relations;
// EXP-1097: the issue detail's relations view (parent line, sub-issues, bands).
pub mod relations_view;
pub mod reporter_reply;
pub mod rows;
// NEW-2: THE schedule picker (cron <-> rows, labels, summary), fixture-locked ×4.
pub mod schedule_picker;
pub mod session_results;
// EXP-1341: ONE session model (kind traits) + THE session title, fixture-locked ×4.
pub mod session_kind;
pub mod session_title;
// EXP-1294: the session tree's views (card, thread, Threads page, Agents), fixture-locked ×4.
pub mod session_tree;
// N-12: THE stack geometry (the toasts and the Dock pile), fixture-locked ×4.
pub mod stack;
pub mod statuses;
pub mod stories;
pub mod tree_guides;
// NEW-59: the Work screen picker's options + copy, fixture-locked ×4.
pub mod work_screen;

pub use enums::{IssuePriority, IssueStatus};
pub use rows::member_fallback_label;
