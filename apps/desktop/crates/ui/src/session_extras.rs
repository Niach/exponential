//! EXP-746 — the LOCAL-only half of a session transcript.
//!
//! An in-process ACP run sees strictly more than the relay does: the exact
//! patch of every edit and a command's output. Neither may ride the wire raw
//! — patch bodies and command strings are precisely what the redactor exists
//! to keep off it — so they arrive as [`engine::LocalFeedEvent`]s and are
//! rendered HERE, hung off the feed rows the matching
//! [`steer::ActivityEvent`]s appended. (EXP-791: the agent's plan and its
//! thoughts used to be pinned here too; the plan now lives in the answer card
//! and the transcript, and thoughts are the transcript's.)
//!
//! EXP-786: an edit card is cut to the SAME caps the wire applies
//! ([`steer::truncate_unified_diff`] at `TOOL_DIFF_MAX_LINES`/`_BYTES`), so a
//! local and a remote viewer of one run show the same prefix of one edit; the
//! header says how many lines were dropped. A remote viewer renders the
//! wire's own per-call diff through the same card ([`render_wire_diff`]).
//!
//! The join is the tricky part and is worth stating once: the wire `tool`
//! event carries no id (`FeedKind::Tool` is `{name, detail, subagent_id}`),
//! and a `FeedItemId` is a local sequence number the engine cannot know. So
//! the engine tags each activity with the ACP `tool_call_id` it belongs to,
//! the drain records `feed item → tool call` as the row lands, and the extras
//! key off the FEED ITEM. Nothing tries to match on titles or text.
//!
//! Two deliberate departures from the dock's diff surface:
//!
//! * an edit card renders its own hunk rather than hosting a
//!   [`crate::diff::DiffView`]. A DiffView is an entity with its own scroll
//!   and file list; one per edit in a long run is a lot of state for a
//!   single-hunk card, and the run's FULL diff already has a DiffView in the
//!   session screen's Changes rail.
//! * the hunk is built by the shared [`steer::unified_diff`] (ACP hands over
//!   `old_text`/`new_text`, the scm model speaks unified diffs) and read back
//!   through `coding::scm::parse_unified_diff` — the exact bytes a remote
//!   viewer gets, so the two cards cannot drift.

use std::collections::HashMap;
use std::path::PathBuf;

use gpui::{
    div, prelude::FluentBuilder as _, AnyElement, App, ClickEvent, InteractiveElement as _,
    IntoElement, ParentElement, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use coding::scm::{DiffFile, DiffLine, DiffLineKind};
use steer::feed::FeedItemId;
use steer::{truncate_unified_diff, unified_diff, TOOL_DIFF_MAX_BYTES, TOOL_DIFF_MAX_LINES};

use crate::controls::WebText as _;
use crate::icons::registry;

/// Lines of an output card kept — the tail, because a command's verdict is at
/// the end. Same cap the CLI attach printer applies.
pub(crate) const OUTPUT_LINES_MAX: usize = 200;

/// Rows of a diff card shown before it folds behind "Show more".
const DIFF_PREVIEW_ROWS: usize = 12;

/// A tool call's local extras, accumulated as the engine reports them.
#[derive(Default)]
pub(crate) struct ToolExtras {
    /// Every edit the call made, in order.
    edits: Vec<EditCard>,
    /// The call's streamed output (`Execute` tools), tail-capped.
    output: Option<OutputCard>,
}

struct EditCard {
    path: PathBuf,
    file: DiffFile,
    /// EXP-786: lines the contract cap dropped off the end of the patch.
    omitted: usize,
}

#[derive(Default)]
struct OutputCard {
    lines: Vec<String>,
    /// Set with the final chunk; `None` while the command is still running.
    exit_code: Option<i32>,
    /// A partial last line waiting for its newline.
    partial: String,
    /// EXP-750: a LIVE `terminal/*` command — the card marks itself running
    /// and offers a Stop until the exit code lands.
    live: bool,
    /// The terminal the Stop button kills. `None` on the plain
    /// `ToolCallContent::Content` path, which has nothing to stop.
    terminal_id: Option<String>,
}

/// Everything local a transcript accumulated: the per-tool-call extras.
#[derive(Default)]
pub(crate) struct LocalExtras {
    by_tool_call: HashMap<String, ToolExtras>,
    /// Which tool call a feed row belongs to (the join, see the module docs).
    by_item: HashMap<FeedItemId, String>,
    /// EXP-783: `by_item` in bind order, so the cap evicts the OLDEST rows.
    bound: std::collections::VecDeque<FeedItemId>,
}

/// EXP-783 — how many feed rows may carry extras at once.
///
/// The extras used to be bounded only by `steer::feed::trim` draining the
/// feed at 2000 items ([`LocalExtras::prune_before`]). The feed now keeps the
/// whole run, so a card holding a whole hunk diff or a terminal's scrollback
/// needs a ceiling of its own — the transcript rows above it stay, their
/// heavyweight attachments do not.
const EXTRAS_ITEM_CAP: usize = 2_000;

impl LocalExtras {
    /// Record that `item` is the feed row of `tool_call_id`.
    pub(crate) fn bind(&mut self, item: FeedItemId, tool_call_id: String) {
        if self.by_item.insert(item, tool_call_id).is_none() {
            self.bound.push_back(item);
        }
        while self.bound.len() > EXTRAS_ITEM_CAP {
            let Some(oldest) = self.bound.pop_front() else { break };
            if let Some(tool_call_id) = self.by_item.remove(&oldest) {
                self.by_tool_call.remove(&tool_call_id);
            }
        }
    }

    /// Fold one local event in. [`engine::LocalFeedEvent::Activity`] and
    /// `Phase` are the caller's — they are feed and lifecycle, not extras.
    pub(crate) fn apply(&mut self, event: engine::LocalFeedEvent) {
        match event {
            engine::LocalFeedEvent::EditDiff {
                tool_call_id,
                path,
                old_text,
                new_text,
            } => {
                // A write that changed nothing (an agent rewriting a file
                // byte for byte) is not a diff — no card at all.
                if let Some(card) = tool_edit_card(path, old_text.as_deref(), &new_text) {
                    self.by_tool_call
                        .entry(tool_call_id)
                        .or_default()
                        .edits
                        .push(card);
                }
            }
            engine::LocalFeedEvent::Output {
                tool_call_id,
                chunk,
                exit_code,
            } => {
                let card = self
                    .by_tool_call
                    .entry(tool_call_id)
                    .or_default()
                    .output
                    .get_or_insert_with(OutputCard::default);
                card.push(&chunk);
                if exit_code.is_some() {
                    card.finish(exit_code);
                }
            }
            // EXP-750: the tool call names the live terminal it renders. The
            // engine flushes what the command already wrote just before this
            // arrives, so a card that ALREADY carries an exit code was over
            // before it was bound and must not go live again.
            engine::LocalFeedEvent::TerminalBound {
                tool_call_id,
                terminal_id,
            } => {
                let card = self
                    .by_tool_call
                    .entry(tool_call_id)
                    .or_default()
                    .output
                    .get_or_insert_with(OutputCard::default);
                card.live = card.exit_code.is_none();
                card.terminal_id = Some(terminal_id);
            }
            // EXP-791: the plan reaches the reader through the answer card
            // and the transcript, and a thought is transcript too — neither
            // is pinned state here any more.
            engine::LocalFeedEvent::Plan { .. } | engine::LocalFeedEvent::Thought { .. } => {}
            // The run is over, so nothing is coming to close a card that
            // never saw an exit code: a REPLAYED `TerminalBound` has no
            // terminal behind it at all, and a child that ignored the kill
            // signal never reports one. Either would keep drawing "Running"
            // over a finished transcript.
            // EXP-758: `Failed` is the same edge for a card. The run died on
            // its handshake or transport, so nothing will ever close one.
            engine::LocalFeedEvent::Phase(
                engine::EnginePhase::Ended | engine::EnginePhase::Failed(_),
            ) => self.end_live(),
            // A tool card's header is already the feed's `tool` row — the
            // status/locations it carries add nothing the row does not show,
            // and a second header per call would double every line.
            engine::LocalFeedEvent::ToolCall { .. }
            | engine::LocalFeedEvent::Activity { .. }
            | engine::LocalFeedEvent::Phase(_) => {}
        }
    }

    /// The run ended: every still-live output card stops claiming to run.
    /// The exit code stays `None` — none ever arrived, and inventing one
    /// would render a verdict the command never gave.
    pub(crate) fn end_live(&mut self) {
        for tool in self.by_tool_call.values_mut() {
            if let Some(output) = tool.output.as_mut() {
                output.live = false;
            }
        }
    }

    /// Drop everything hanging off feed rows the feed itself has already
    /// dropped. `steer::feed::trim` evicts past `FEED_BYTE_CAP`, and without
    /// this the extras (an `EditCard` holds a whole hunk diff) keep every one
    /// of them for the life of the run. `first` is the id of the OLDEST
    /// surviving feed item; entries below it, and any tool call no surviving
    /// row still names, go. [`EXTRAS_ITEM_CAP`] bounds them meanwhile.
    pub(crate) fn prune_before(&mut self, first: FeedItemId) {
        self.by_item.retain(|item, _| *item >= first);
        self.bound.retain(|item| *item >= first);
        let live: std::collections::HashSet<&str> =
            self.by_item.values().map(String::as_str).collect();
        self.by_tool_call.retain(|id, _| live.contains(id.as_str()));
    }

    fn for_item(&self, item: FeedItemId) -> Option<&ToolExtras> {
        self.by_tool_call.get(self.by_item.get(&item)?)
    }

    /// Whether `item`'s row has anything hanging off it.
    pub(crate) fn has_extras(&self, item: FeedItemId) -> bool {
        self.for_item(item)
            .is_some_and(|extras| !extras.edits.is_empty() || extras.output.is_some())
    }

    /// EXP-862 — `item`'s per-edit patches, for the diff pane scoped to that
    /// row. `None` when the row produced no local edit card (a remote viewer
    /// reads the wire patch off the feed row instead).
    pub(crate) fn edit_files(&self, item: FeedItemId) -> Option<Vec<DiffFile>> {
        let extras = self.for_item(item)?;
        (!extras.edits.is_empty())
            .then(|| extras.edits.iter().map(|edit| edit.file.clone()).collect())
    }
}

impl OutputCard {
    fn push(&mut self, chunk: &str) {
        self.partial.push_str(chunk);
        while let Some(newline) = self.partial.find('\n') {
            let line: String = self.partial.drain(..=newline).collect();
            self.lines.push(line.trim_end_matches(['\n', '\r']).to_string());
        }
        self.trim();
    }

    fn finish(&mut self, exit_code: Option<i32>) {
        if !self.partial.is_empty() {
            let last = std::mem::take(&mut self.partial);
            self.lines.push(last);
        }
        self.exit_code = exit_code;
        // The exit code IS the end of a live terminal (EXP-750).
        self.live = false;
        self.trim();
    }

    /// Keep the TAIL: a command's verdict is at the end, and an unbounded
    /// buffer of a `yarn install` is megabytes of nothing.
    fn trim(&mut self) {
        if self.lines.len() > OUTPUT_LINES_MAX {
            let overflow = self.lines.len() - OUTPUT_LINES_MAX;
            self.lines.drain(..overflow);
        }
    }

    fn rows(&self) -> Vec<String> {
        let mut rows = self.lines.clone();
        if !self.partial.is_empty() {
            rows.push(self.partial.clone());
        }
        rows
    }
}

// ---------------------------------------------------------------------------
// old_text/new_text → one cut patch → the card's DiffFile (pure)
// ---------------------------------------------------------------------------

/// EXP-786 — the marker line the publisher appends to a cut wire patch
/// (`\ N more lines truncated`): unified-diff metadata, so a parser skips it
/// and this reads the count back.
fn truncated_marker_count(line: &str) -> Option<usize> {
    line.strip_prefix("\\ ")?
        .strip_suffix(" more lines truncated")?
        .parse()
        .ok()
}

/// Read ONE per-call patch (the shape [`unified_diff`] emits: `---`/`+++`
/// headers and hunks, no `diff --git` line) into the scm model, plus the
/// lines a `\ N more lines truncated` marker says were dropped. `None` when
/// the patch names no file or holds no hunk.
pub(crate) fn parse_tool_diff(patch: &str) -> Option<(DiffFile, usize)> {
    let path = patch
        .lines()
        .find_map(|line| line.strip_prefix("+++ b/"))
        .or_else(|| patch.lines().find_map(|line| line.strip_prefix("--- a/")))?;
    let omitted = patch.lines().find_map(truncated_marker_count).unwrap_or(0);
    // The scm parser opens a file on `diff --git` only — synthesise the
    // header a git patch would carry.
    let framed = format!("diff --git a/{path} b/{path}\n{patch}");
    let file = coding::scm::parse_unified_diff(&framed).into_iter().next()?;
    (!file.hunks.is_empty()).then_some((file, omitted))
}

/// The edit card for ONE local edit: the shared patch, cut to the contract
/// caps — the SAME bytes the publisher puts on the wire for a remote viewer —
/// and read back into the scm model. `None` when the write changed nothing.
fn tool_edit_card(path: PathBuf, old_text: Option<&str>, new_text: &str) -> Option<EditCard> {
    let display = path.to_string_lossy();
    let patch = unified_diff(&display, old_text, new_text);
    if patch.is_empty() {
        return None;
    }
    let (kept, omitted) = truncate_unified_diff(&patch, TOOL_DIFF_MAX_LINES, TOOL_DIFF_MAX_BYTES);
    let (file, _) = parse_tool_diff(&kept)?;
    Some(EditCard {
        path,
        file,
        omitted,
    })
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// The cards hanging off feed row `item`: one per edit, then the command
/// output. `None` when the row has none (every remote row, and every local
/// row that is not a tool call).
///
/// `on_toggle` is the host's own listener (`cx.listener(..)`), so this stays
/// free of the hosting view's type; `on_kill` is the same idea for the Stop
/// button on a LIVE terminal card (EXP-750) and is handed the terminal id to
/// stop. It is `None` when the source cannot stop anything (a replay, a
/// remote viewer): there the whole Running/Stop strip is left out rather than
/// offering a button whose click goes nowhere.
///
/// EXP-862: `on_open` is the edit card HEADER's click — the diff pane, scoped
/// to this one edit. `None` where there is no pane to open into.
pub(crate) fn render_extras(
    extras: &LocalExtras,
    item: FeedItemId,
    expanded: bool,
    on_toggle: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    on_open: Option<OpenDiff>,
    on_kill: Option<Box<dyn Fn(&str, &mut Window, &mut App) + 'static>>,
    cx: &App,
) -> Option<AnyElement> {
    let tool = extras.for_item(item)?;
    if tool.edits.is_empty() && tool.output.is_none() {
        return None;
    }
    let muted = cx.theme().muted_foreground;
    let mut column = v_flex().w_full().min_w_0().gap_1().pl_5().pt_1();
    for (index, edit) in tool.edits.iter().enumerate() {
        column = column.child(render_edit_card(
            edit,
            (item, index),
            expanded,
            on_open.clone(),
            cx,
        ));
    }
    if let Some(output) = tool.output.as_ref() {
        column = column.child(render_output_card(output, item, expanded, on_kill, cx));
    }
    let foldable = tool
        .edits
        .iter()
        .any(|edit| hunk_rows(&edit.file) > DIFF_PREVIEW_ROWS)
        || tool
            .output
            .as_ref()
            .is_some_and(|output| output.rows().len() > DIFF_PREVIEW_ROWS);
    if foldable {
        column = column.child(fold_toggle(item, expanded, on_toggle, muted));
    }
    Some(column.into_any_element())
}

/// EXP-862 — "open the diff pane at this edit", the inline card header's
/// click. `Rc` because one tool row can carry several edit cards and they all
/// open the same scope.
pub(crate) type OpenDiff = std::rc::Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// The `Show more` / `Show less` line under a folded card. It STOPS the click
/// (EXP-862): the card above it opens the diff pane now, and unfolding a
/// patch in place must not also open the pane beside it.
fn fold_toggle(
    item: FeedItemId,
    expanded: bool,
    on_toggle: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    muted: gpui::Hsla,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(("session-extras-toggle", item as usize))
        .mt_0p5()
        .cursor_pointer()
        .text_xs()
        .text_color(muted)
        .child(if expanded { "Show less" } else { "Show more" })
        .on_click(move |event: &ClickEvent, window, cx| {
            cx.stop_propagation();
            on_toggle(event, window, cx);
        })
}

/// EXP-786: "… N more lines" — the header's note for a cut patch.
pub(crate) fn omitted_caption(omitted: usize) -> String {
    format!("… {omitted} more line{}", if omitted == 1 { "" } else { "s" })
}

fn hunk_rows(file: &DiffFile) -> usize {
    file.hunks.iter().map(|hunk| hunk.lines.len()).sum()
}

fn render_edit_card(
    edit: &EditCard,
    id: (FeedItemId, usize),
    expanded: bool,
    on_open: Option<OpenDiff>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let mut body = v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(
            h_flex()
                // EXP-862: the header IS the way into the diff pane, scoped
                // to this edit — the card shows a preview of one patch, the
                // pane shows the patch.
                .id(("session-edit-card", id.0 as usize * 64 + id.1))
                .w_full()
                .min_w_0()
                .gap_1p5()
                .items_center()
                .text_2xs()
                .text_color(muted)
                .when_some(on_open, |header, on_open| {
                    header
                        .cursor_pointer()
                        .hover(|this| this.text_color(cx.theme().foreground))
                        .on_click(move |event: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            on_open(event, window, cx);
                        })
                })
                .child(Icon::new(registry::CODING_DIFF).xsmall())
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .font_family(theme::terminal::FONT_FAMILY)
                        .child(SharedString::from(
                            edit.path.to_string_lossy().to_string(),
                        )),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .font_family(theme::terminal::FONT_FAMILY)
                        .text_color(theme::tokens::GREEN.to_hsla())
                        .child(SharedString::from(format!("+{}", edit.file.additions))),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .font_family(theme::terminal::FONT_FAMILY)
                        .text_color(cx.theme().danger)
                        .child(SharedString::from(format!("-{}", edit.file.deletions))),
                )
                // EXP-786: the contract cap cut the patch — say by how much,
                // so the card never passes for the whole edit.
                .when(edit.omitted > 0, |this| {
                    this.child(
                        div()
                            .flex_shrink_0()
                            .child(SharedString::from(omitted_caption(edit.omitted))),
                    )
                }),
        );
    let rows: Vec<&DiffLine> = edit
        .file
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .collect();
    let shown = if expanded {
        rows.len()
    } else {
        rows.len().min(DIFF_PREVIEW_ROWS)
    };
    let mut lines = v_flex().w_full().min_w_0();
    for line in rows.iter().take(shown) {
        let (marker, tint) = match line.kind {
            DiffLineKind::Addition => ("+", Some(theme::tokens::GREEN.to_hsla())),
            DiffLineKind::Deletion => ("-", Some(cx.theme().danger)),
            DiffLineKind::Context => (" ", None),
        };
        lines = lines.child(
            div()
                .w_full()
                .min_w_0()
                .truncate()
                .text_2xs()
                .font_family(theme::terminal::FONT_FAMILY)
                .when_some(tint, |this, tint| this.text_color(tint))
                .when(tint.is_none(), |this| this.text_color(muted))
                .child(SharedString::from(format!("{marker}{}", line.content))),
        );
    }
    body = body.child(lines);
    crate::surface::glass_row_card()
        .w_full()
        .min_w_0()
        .px_2()
        .py_1p5()
        .child(body)
        .into_any_element()
}

fn render_output_card(
    output: &OutputCard,
    item: FeedItemId,
    expanded: bool,
    on_kill: Option<Box<dyn Fn(&str, &mut Window, &mut App) + 'static>>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let rows = output.rows();
    let shown = if expanded {
        rows.len()
    } else {
        rows.len().min(DIFF_PREVIEW_ROWS)
    };
    // The TAIL is what matters, so a collapsed card shows the last rows.
    let skip = rows.len().saturating_sub(shown);
    let mut lines = v_flex().w_full().min_w_0();
    for line in rows.iter().skip(skip) {
        lines = lines.child(
            div()
                .w_full()
                .min_w_0()
                .truncate()
                .text_2xs()
                .text_color(muted)
                .font_family(theme::terminal::FONT_FAMILY)
                .child(SharedString::from(line.clone())),
        );
    }
    let mut card = crate::surface::glass_row_card()
        .w_full()
        .min_w_0()
        .px_2()
        .py_1p5();
    // EXP-750: a live terminal says so and offers a Stop — above the output,
    // because the tail of a running command moves under the reader's eye.
    // Only where the click can actually reach a terminal, though: a replay
    // gets neither marker nor button (its card is history, not a run).
    if let Some(on_kill) = on_kill.filter(|_| output.live) {
        let terminal_id = output.terminal_id.clone().unwrap_or_default();
        card = card.child(
            h_flex()
                .w_full()
                .min_w_0()
                .gap_1p5()
                .items_center()
                .text_2xs()
                .text_color(muted)
                .child(Icon::new(registry::NAV_TERMINAL).xsmall())
                .child("Running")
                .child(
                    Button::new(("session-terminal-stop", item as usize))
                        .ghost()
                        .cursor_pointer()
                        .xsmall()
                        .icon(registry::CODING_STOP)
                        .tooltip("Stop this command")
                        .on_click(move |_: &ClickEvent, window, cx| {
                            on_kill(&terminal_id, window, cx)
                        }),
                ),
        );
    }
    card = card.child(lines);
    if let Some(code) = output.exit_code.filter(|code| *code != 0) {
        card = card.child(
            div()
                .text_2xs()
                .text_color(cx.theme().danger)
                .child(SharedString::from(format!("exit {code}"))),
        );
    }
    card.into_any_element()
}

/// EXP-786 — a REMOTE row's per-call diff: the patch the publisher cut and
/// put on the wire, rendered through the same edit card a local run gets.
/// The cut is the publisher's (the trailing `\ N more lines truncated`
/// marker carries the count); nothing is re-truncated here. An unparseable
/// patch renders nothing rather than a broken card.
pub(crate) fn render_wire_diff(
    diff: &str,
    item: FeedItemId,
    expanded: bool,
    on_toggle: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    on_open: Option<OpenDiff>,
    cx: &App,
) -> AnyElement {
    let Some((file, omitted)) = parse_tool_diff(diff) else {
        return div().into_any_element();
    };
    let muted = cx.theme().muted_foreground;
    let edit = EditCard {
        path: PathBuf::from(file.path.clone()),
        file,
        omitted,
    };
    let foldable = hunk_rows(&edit.file) > DIFF_PREVIEW_ROWS;
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .pl_5()
        .pt_1()
        .child(render_edit_card(&edit, (item, 0), expanded, on_open, cx))
        .when(foldable, |column| {
            column.child(fold_toggle(item, expanded, on_toggle, muted))
        })
        .into_any_element()
}

/// EXP-772: the composer's ONE chip — the session MODE. Model, effort and
/// every other option picker left the mid-session UI: an agent is configured
/// when it starts, and the only thing worth flipping mid-run is plan on/off.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConfigChip {
    /// The mode id in force.
    pub(crate) value: String,
    /// Its leading label ("Mode").
    pub(crate) label: String,
    /// What it currently reads ("Plan").
    pub(crate) value_label: String,
    /// Every mode the run advertised; one entry = read-only.
    pub(crate) values: Vec<steer::frames::ConfigValue>,
}

/// EXP-772: the mode chip, or `None` when the run advertises no modes (codex
/// advertises none, so its composer draws nothing). Mirrored ×4 as `modeChip`.
///
/// EXP-847 gave it its reader back: the session HEADER wears it as a READ-ONLY
/// "Plan" chip while `current_mode` is [`PLAN_MODE_ID`]
/// (`SteerSessionView::plan_mode_label`), so a run that is planning says so and
/// an approved `ExitPlanMode` visibly clears it. Never a control — EXP-790
/// keeps the mode a launch-time choice.
pub(crate) fn mode_chip(config: Option<&steer::SessionConfig>) -> Option<ConfigChip> {
    let config = config?;
    if config.modes.is_empty() {
        return None;
    }
    let current = config.current_mode.as_deref().unwrap_or_default();
    let value_label = config
        .modes
        .iter()
        .find(|mode| mode.id == current)
        .map(|mode| mode.label.clone())
        .filter(|label| !label.is_empty())
        .or_else(|| (!current.is_empty()).then(|| current.to_string()))
        .unwrap_or_else(|| crate::slash_commands::CONFIG_DEFAULT_VALUE_LABEL.to_string());
    Some(ConfigChip {
        value: current.to_string(),
        label: crate::slash_commands::CONFIG_MODE_LABEL.to_string(),
        value_label,
        values: config
            .modes
            .iter()
            .map(|mode| steer::frames::ConfigValue::new(mode.id.clone(), mode.label.clone()))
            .collect(),
    })
}

/// The mode id every plan-capable agent advertises.
pub(crate) const PLAN_MODE_ID: &str = "plan";

/// EXP-772: the plan/build PAIR — exactly two modes, one of them `plan`.
/// Mirrored ×4 as `planModeToggle` and kept as that mirror (lock-tested
/// below); EXP-790 retired the mid-session pill that drew it, so no view
/// reads it any more — plan is a launch-time switch on the chat page.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlanModeToggle {
    /// The mode to switch to when turning plan ON.
    pub(crate) plan_id: String,
    /// The mode to switch back to when turning it OFF.
    pub(crate) build_id: String,
    /// Plan mode is in force right now.
    pub(crate) active: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn plan_mode_toggle(config: Option<&steer::SessionConfig>) -> Option<PlanModeToggle> {
    let config = config?;
    if config.modes.len() != 2 {
        return None;
    }
    let plan = config.modes.iter().find(|mode| mode.id == PLAN_MODE_ID)?;
    let build = config.modes.iter().find(|mode| mode.id != PLAN_MODE_ID)?;
    Some(PlanModeToggle {
        plan_id: plan.id.clone(),
        build_id: build.id.clone(),
        active: config.current_mode.as_deref() == Some(plan.id.as_str()),
    })
}

/// The header's compact context read (`124k / 200k`) — the sheet spells out
/// the percent and the cost.
pub(crate) fn context_summary(usage: Option<&steer::SessionUsage>) -> Option<String> {
    let full = crate::usage_bar::format_context_usage(usage);
    let (head, _) = full.split_once(" (")?;
    Some(head.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use coding::scm::FileStatus;

    /// The feed drops its oldest rows at `FEED_CAP`; the extras must follow,
    /// or a long run keeps every hunk it ever rendered. A tool call whose LAST
    /// row went keeps nothing; one that still has a row survives whole.
    #[test]
    fn pruning_drops_the_extras_of_dropped_rows() {
        let mut extras = LocalExtras::default();
        for (item, call) in [(1u64, "old"), (2, "kept"), (3, "kept"), (4, "new")] {
            extras.bind(item, call.to_string());
            extras.apply(engine::LocalFeedEvent::EditDiff {
                tool_call_id: call.to_string(),
                path: PathBuf::from("src/lib.rs"),
                old_text: Some("a\n".to_string()),
                new_text: "b\n".to_string(),
            });
        }
        assert!(extras.has_extras(1));

        extras.prune_before(3);

        // Row 1 and 2 are gone; "old" had no other row, so it went with them.
        assert!(!extras.has_extras(1));
        assert!(!extras.has_extras(2));
        assert!(!extras.by_tool_call.contains_key("old"));
        // "kept" still has row 3, so row 3 still renders its edits.
        assert!(extras.has_extras(3));
        assert!(extras.has_extras(4));
        assert_eq!(extras.by_item.len(), 2);
        assert_eq!(extras.by_tool_call.len(), 2);
    }

    fn line(kind: DiffLineKind, content: &str) -> (DiffLineKind, String) {
        (kind, content.to_string())
    }

    fn rows(file: &DiffFile) -> Vec<(DiffLineKind, String)> {
        file.hunks
            .iter()
            .flat_map(|hunk| hunk.lines.iter())
            .map(|line| (line.kind, line.content.clone()))
            .collect()
    }

    fn card(old: Option<&str>, new: &str) -> Option<EditCard> {
        tool_edit_card(PathBuf::from("src/lib.rs"), old, new)
    }

    /// A one-line edit is a one-line diff, not a whole-file replacement — the
    /// shared patch keeps the common prefix and suffix as context.
    #[test]
    fn an_edit_diffs_only_the_block_that_changed() {
        let card = card(Some("a\nb\nc\n"), "a\nB\nc\n").expect("an edit");
        assert_eq!(card.path, PathBuf::from("src/lib.rs"));
        assert_eq!(card.file.status, FileStatus::Modified);
        assert_eq!((card.file.additions, card.file.deletions), (1, 1));
        assert_eq!(card.omitted, 0);
        assert_eq!(
            rows(&card.file),
            vec![
                line(DiffLineKind::Context, "a"),
                line(DiffLineKind::Deletion, "b"),
                line(DiffLineKind::Addition, "B"),
                line(DiffLineKind::Context, "c"),
            ]
        );
        // The hunk header numbers both sides from the first context line.
        let hunk = &card.file.hunks[0];
        assert_eq!((hunk.old_start, hunk.new_start), (1, 1));
        assert_eq!((hunk.old_lines, hunk.new_lines), (3, 3));
    }

    /// A new file has no old side at all: every line is an addition and the
    /// status says Added (the card must not claim to have deleted nothing).
    #[test]
    fn a_created_file_is_all_additions() {
        let card = card(None, "one\ntwo\n").expect("a new file");
        assert_eq!(card.file.status, FileStatus::Added);
        assert_eq!((card.file.additions, card.file.deletions), (2, 0));
        assert_eq!(
            rows(&card.file),
            vec![
                line(DiffLineKind::Addition, "one"),
                line(DiffLineKind::Addition, "two"),
            ]
        );
    }

    /// An unchanged write (an agent rewriting a file with the same bytes)
    /// produces no card rather than a diff of nothing.
    #[test]
    fn an_identical_write_produces_no_card() {
        assert!(card(Some("a\nb\n"), "a\nb\n").is_none());
        let mut extras = LocalExtras::default();
        extras.bind(1, "call-1".to_string());
        extras.apply(engine::LocalFeedEvent::EditDiff {
            tool_call_id: "call-1".to_string(),
            path: PathBuf::from("same.rs"),
            old_text: Some("a\nb\n".to_string()),
            new_text: "a\nb\n".to_string(),
        });
        assert!(!extras.has_extras(1));
    }

    /// EXP-786: a local edit is cut to the SAME caps the wire applies, and
    /// the card knows how many lines it lost — a local viewer and a remote
    /// one show the same prefix of one edit.
    #[test]
    fn a_long_edit_is_cut_to_the_contract_caps() {
        let new: String = (0..TOOL_DIFF_MAX_LINES * 2)
            .map(|n| format!("line {n}\n"))
            .collect();
        let card = card(None, &new).expect("a big new file");
        let shown = rows(&card.file).len();
        assert!(shown < TOOL_DIFF_MAX_LINES * 2, "{shown} rows were kept");
        // The patch is the headers + the hunk line + the kept rows; what was
        // dropped is everything past the cap.
        assert_eq!(shown + 3 + card.omitted, TOOL_DIFF_MAX_LINES * 2 + 3);
        assert!(card.omitted > 0);
        assert_eq!(omitted_caption(1), "… 1 more line");
        assert_eq!(omitted_caption(card.omitted), format!("… {} more lines", card.omitted));
        // A short edit loses nothing.
        assert_eq!(self::card(None, "x\n").expect("tiny").omitted, 0);
    }

    /// EXP-786: the wire patch a publisher cut ends in its marker line; the
    /// remote card reads the count off it and renders the hunk under it.
    #[test]
    fn a_wire_patch_parses_with_its_truncation_marker() {
        let patch = "--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,2 +1,2 @@\n a\n-b\n+B\n\\ 7 more lines truncated\n";
        let (file, omitted) = parse_tool_diff(patch).expect("a hunk");
        assert_eq!(file.path, "src/x.rs");
        assert_eq!(omitted, 7);
        assert_eq!(
            rows(&file),
            vec![
                line(DiffLineKind::Context, "a"),
                line(DiffLineKind::Deletion, "b"),
                line(DiffLineKind::Addition, "B"),
            ]
        );
        // No file, no card; a hunk-less patch is no card either.
        assert!(parse_tool_diff("nothing here").is_none());
        assert!(parse_tool_diff("--- a/x\n+++ b/x\n").is_none());
    }

    /// The output card keeps the TAIL and folds partial chunks into lines —
    /// a streamed build log arrives byte-wise, not line-wise.
    #[test]
    fn output_streams_into_lines_and_keeps_the_tail() {
        let mut card = OutputCard::default();
        card.push("one\ntw");
        card.push("o\n");
        assert_eq!(card.rows(), vec!["one".to_string(), "two".to_string()]);
        for n in 0..OUTPUT_LINES_MAX + 5 {
            card.push(&format!("line {n}\n"));
        }
        assert_eq!(card.rows().len(), OUTPUT_LINES_MAX);
        assert_eq!(
            card.rows().last().map(String::as_str),
            Some(format!("line {}", OUTPUT_LINES_MAX + 4).as_str())
        );
        // A chunk with no trailing newline still shows, and the exit code
        // closes the card.
        let mut card = OutputCard::default();
        card.push("no newline");
        assert_eq!(card.rows(), vec!["no newline".to_string()]);
        card.finish(Some(1));
        assert_eq!(card.exit_code, Some(1));
    }

    /// EXP-750: a bound terminal card runs until its exit code lands — that
    /// is what draws the "Running" marker and the Stop button, and what
    /// takes them away again.
    #[test]
    fn a_bound_terminal_card_is_live_until_its_exit_code_lands() {
        let mut extras = LocalExtras::default();
        extras.apply(engine::LocalFeedEvent::TerminalBound {
            tool_call_id: "call-1".to_string(),
            terminal_id: "term-1".to_string(),
        });
        fn card<'a>(extras: &'a LocalExtras, id: &str) -> &'a OutputCard {
            extras
                .by_tool_call
                .get(id)
                .and_then(|tool| tool.output.as_ref())
                .expect("the bound terminal has a card")
        }
        assert!(card(&extras, "call-1").live);
        assert_eq!(
            card(&extras, "call-1").terminal_id.as_deref(),
            Some("term-1")
        );

        extras.apply(engine::LocalFeedEvent::Output {
            tool_call_id: "call-1".to_string(),
            chunk: "building\n".to_string(),
            exit_code: None,
        });
        assert!(
            card(&extras, "call-1").live,
            "a chunk without an exit keeps it running"
        );
        assert_eq!(card(&extras, "call-1").rows(), vec!["building".to_string()]);

        extras.apply(engine::LocalFeedEvent::Output {
            tool_call_id: "call-1".to_string(),
            chunk: String::new(),
            exit_code: Some(0),
        });
        assert!(!card(&extras, "call-1").live, "the exit code ends the run");
        assert_eq!(card(&extras, "call-1").exit_code, Some(0));

        // A terminal bound only AFTER it finished (the engine flushes the
        // whole buffer plus the code in one chunk) never goes live again.
        let mut late = LocalExtras::default();
        late.apply(engine::LocalFeedEvent::Output {
            tool_call_id: "call-2".to_string(),
            chunk: "done\n".to_string(),
            exit_code: Some(2),
        });
        late.apply(engine::LocalFeedEvent::TerminalBound {
            tool_call_id: "call-2".to_string(),
            terminal_id: "term-2".to_string(),
        });
        assert!(!card(&late, "call-2").live);
    }

    /// Review C4: a bound terminal whose exit code NEVER lands — a replayed
    /// `TerminalBound` (no terminal behind it) or a child that ignored the
    /// kill signal — must not keep drawing "Running" over a finished run.
    /// The end of the run is that card's other closing edge.
    #[test]
    fn the_end_of_the_run_ends_every_live_card() {
        let mut extras = LocalExtras::default();
        for id in ["call-1", "call-2"] {
            extras.apply(engine::LocalFeedEvent::TerminalBound {
                tool_call_id: id.to_string(),
                terminal_id: format!("term-{id}"),
            });
        }
        extras.apply(engine::LocalFeedEvent::Output {
            tool_call_id: "call-1".to_string(),
            chunk: "building\n".to_string(),
            exit_code: None,
        });
        assert!(extras
            .by_tool_call
            .values()
            .all(|tool| tool.output.as_ref().is_some_and(|output| output.live)));

        // A phase that is not the end changes nothing.
        extras.apply(engine::LocalFeedEvent::Phase(engine::EnginePhase::Live));
        assert!(extras
            .by_tool_call
            .values()
            .all(|tool| tool.output.as_ref().is_some_and(|output| output.live)));

        extras.apply(engine::LocalFeedEvent::Phase(engine::EnginePhase::Ended));
        for id in ["call-1", "call-2"] {
            let card = extras
                .by_tool_call
                .get(id)
                .and_then(|tool| tool.output.as_ref())
                .expect("the bound terminal has a card");
            assert!(!card.live, "{id} is no longer running");
            // No code was ever reported, so none is invented — the card just
            // stops claiming to run.
            assert_eq!(card.exit_code, None);
        }
        // What the card already showed survives the edge.
        assert_eq!(
            extras
                .by_tool_call
                .get("call-1")
                .and_then(|tool| tool.output.as_ref())
                .map(|output| output.rows()),
            Some(vec!["building".to_string()])
        );
    }

    /// Extras key on the FEED ROW, through the tool-call id the engine
    /// tagged the activity with — never on titles or text.
    #[test]
    fn extras_reach_the_feed_row_their_tool_call_landed_on() {
        let mut extras = LocalExtras::default();
        extras.apply(engine::LocalFeedEvent::EditDiff {
            tool_call_id: "call-1".to_string(),
            path: PathBuf::from("a.rs"),
            old_text: Some("a\n".to_string()),
            new_text: "b\n".to_string(),
        });
        // Nothing is reachable until the row it belongs to lands.
        assert!(!extras.has_extras(7));
        extras.bind(7, "call-1".to_string());
        assert!(extras.has_extras(7));
        assert!(!extras.has_extras(8), "a different row keeps its own cards");
    }

    // ── EXP-772: the composer's ONE chip (×4 `modeChip`) ──────────────────

    fn config(modes: &[(&str, &str)], current: Option<&str>) -> steer::SessionConfig {
        steer::SessionConfig {
            // EXP-772: the engine publishes an EMPTY option list now — the
            // mid-session model/effort pickers are gone on every client.
            options: Vec::new(),
            current_mode: current.map(str::to_string),
            modes: modes
                .iter()
                .map(|(id, label)| steer::frames::ConfigMode::new(*id, *label))
                .collect(),
            commands: Vec::new(),
        }
    }

    /// The chip row is the mode and nothing else — a claude run reads "Plan",
    /// a run with no modes (codex) draws nothing at all.
    #[test]
    fn the_only_chip_left_is_the_mode() {
        let claude = config(&[("plan", "Plan"), ("bypassPermissions", "Build")], Some("plan"));
        let chip = mode_chip(Some(&claude)).expect("claude advertises modes");
        assert_eq!((chip.label.as_str(), chip.value_label.as_str()), ("Mode", "Plan"));
        assert_eq!(chip.values.len(), 2);

        // No modes, no chip; no config at all, no chip.
        assert!(mode_chip(Some(&config(&[], None))).is_none());
        assert!(mode_chip(None).is_none());

        // A mode the publisher never described still names itself.
        let unknown = config(&[("plan", "Plan")], Some("acceptEdits"));
        assert_eq!(
            mode_chip(Some(&unknown)).expect("a mode is in force").value_label,
            "acceptEdits"
        );
    }

    /// EXP-772: exactly two modes with `plan` among them is the toggle shape;
    /// anything else falls back to the chip.
    #[test]
    fn a_plan_build_pair_becomes_a_plan_switch() {
        let claude = config(&[("plan", "Plan"), ("bypassPermissions", "Build")], Some("plan"));
        let toggle = plan_mode_toggle(Some(&claude)).expect("the claude pair");
        assert_eq!(toggle.plan_id, "plan");
        assert_eq!(toggle.build_id, "bypassPermissions");
        assert!(toggle.active);

        // Off plan, the switch points back at Build.
        let building = config(
            &[("plan", "Plan"), ("bypassPermissions", "Build")],
            Some("bypassPermissions"),
        );
        let toggle = plan_mode_toggle(Some(&building)).expect("the claude pair");
        assert!(!toggle.active);

        // A pair without `plan`, a single mode and a three-mode list are all
        // chips, not switches.
        assert!(plan_mode_toggle(Some(&config(&[("a", "A"), ("b", "B")], Some("a")))).is_none());
        assert!(plan_mode_toggle(Some(&config(&[("plan", "Plan")], Some("plan")))).is_none());
        assert!(plan_mode_toggle(Some(&config(
            &[("plan", "Plan"), ("b", "B"), ("c", "C")],
            Some("plan")
        )))
        .is_none());
        assert!(plan_mode_toggle(None).is_none());
    }

    /// The header shows the tokens; the sheet adds the percent and the cost.
    #[test]
    fn the_header_summary_drops_the_percent() {
        let usage = steer::SessionUsage {
            context_used: 124_000,
            context_size: 200_000,
            cost_usd: None,
        };
        assert_eq!(context_summary(Some(&usage)), Some("124k / 200k".to_string()));
        assert_eq!(context_summary(None), None);
    }
}
