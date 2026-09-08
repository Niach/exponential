//! EXP-746 — the LOCAL-only half of a session transcript.
//!
//! An in-process ACP run sees strictly more than the relay does: the exact
//! patch of every edit, a command's output, the agent's plan, its thoughts.
//! None of that may ride the wire — raw patch bodies and command strings are
//! precisely what the redactor exists to keep off it — so it arrives as
//! [`engine::LocalFeedEvent`]s and is rendered HERE, hung off the feed rows
//! the matching [`steer::ActivityEvent`]s appended.
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
//! * the hunk is computed here, because ACP hands over `old_text`/`new_text`
//!   and the scm model speaks unified diffs. [`edit_diff`] is that conversion
//!   (a common prefix/suffix trim around one replacement block — the shape a
//!   single edit actually has), pure and unit-tested.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui::{
    div, prelude::FluentBuilder as _, AnyElement, App, ClickEvent, InteractiveElement as _,
    IntoElement, ParentElement, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use coding::scm::{DiffFile, DiffLine, DiffLineKind, FileStatus, UnifiedHunk};
use steer::feed::FeedItemId;

use crate::controls::WebText as _;
use crate::icons::registry;

/// Lines of an output card kept — the tail, because a command's verdict is at
/// the end. Same cap the CLI attach printer applies.
pub(crate) const OUTPUT_LINES_MAX: usize = 200;

/// Context lines drawn around an edit's replacement block.
const EDIT_CONTEXT_LINES: usize = 3;

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

/// Everything local a transcript accumulated: the per-tool-call extras, the
/// pinned plan, and the agent's latest thought.
#[derive(Default)]
pub(crate) struct LocalExtras {
    by_tool_call: HashMap<String, ToolExtras>,
    /// Which tool call a feed row belongs to (the join, see the module docs).
    by_item: HashMap<FeedItemId, String>,
    /// EXP-783: `by_item` in bind order, so the cap evicts the OLDEST rows.
    bound: std::collections::VecDeque<FeedItemId>,
    plan: Vec<engine::PlanEntryView>,
    thought: Option<String>,
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

    /// How many feed rows currently carry extras. EXP-783: the renderer folds
    /// this into its fingerprint memo's epoch — it moves exactly when a row
    /// gains its first card, which is the only thing about extras a row's
    /// HEIGHT depends on.
    pub(crate) fn bound_items(&self) -> usize {
        self.by_tool_call.len()
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
                let file = edit_diff(&path, old_text.as_deref(), &new_text);
                self.by_tool_call
                    .entry(tool_call_id)
                    .or_default()
                    .edits
                    .push(EditCard { path, file });
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
            // The plan is replaced wholesale each time (ACP semantics).
            engine::LocalFeedEvent::Plan { entries } => self.plan = entries,
            engine::LocalFeedEvent::Thought { text, .. } => {
                let text = text.trim();
                self.thought = (!text.is_empty()).then(|| text.to_string());
            }
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

    /// The pinned plan, newest wholesale replacement.
    pub(crate) fn plan(&self) -> &[engine::PlanEntryView] {
        &self.plan
    }

    /// The agent's latest thought, if it is still thinking out loud.
    pub(crate) fn thought(&self) -> Option<&str> {
        self.thought.as_deref()
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
// old_text/new_text → one unified hunk (pure)
// ---------------------------------------------------------------------------

/// The [`DiffFile`] for ONE edit: the changed block with up to
/// [`EDIT_CONTEXT_LINES`] of context, exactly the shape a single tool call
/// produces. Deliberately not a full diff algorithm — an edit replaces one
/// contiguous region, and trimming the common prefix and suffix finds it
/// without a dependency.
pub(crate) fn edit_diff(path: &Path, old_text: Option<&str>, new_text: &str) -> DiffFile {
    let status = if old_text.is_none() {
        FileStatus::Added
    } else {
        FileStatus::Modified
    };
    let old: Vec<&str> = old_text.map(split_lines).unwrap_or_default();
    let new: Vec<&str> = split_lines(new_text);

    let mut prefix = 0;
    while prefix < old.len() && prefix < new.len() && old[prefix] == new[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < old.len() - prefix
        && suffix < new.len() - prefix
        && old[old.len() - 1 - suffix] == new[new.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let lead = prefix.saturating_sub(EDIT_CONTEXT_LINES);
    let old_tail = (old.len() - suffix + EDIT_CONTEXT_LINES).min(old.len());
    let new_tail = (new.len() - suffix + EDIT_CONTEXT_LINES).min(new.len());

    let mut lines: Vec<DiffLine> = Vec::new();
    let mut additions = 0;
    let mut deletions = 0;
    // Leading context.
    for (ix, line) in old.iter().enumerate().take(prefix).skip(lead) {
        lines.push(DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(ix as u32 + 1),
            new_line: Some(ix as u32 + 1),
            content: (*line).to_string(),
        });
    }
    for (ix, line) in old.iter().enumerate().take(old.len() - suffix).skip(prefix) {
        deletions += 1;
        lines.push(DiffLine {
            kind: DiffLineKind::Deletion,
            old_line: Some(ix as u32 + 1),
            new_line: None,
            content: (*line).to_string(),
        });
    }
    for (ix, line) in new.iter().enumerate().take(new.len() - suffix).skip(prefix) {
        additions += 1;
        lines.push(DiffLine {
            kind: DiffLineKind::Addition,
            old_line: None,
            new_line: Some(ix as u32 + 1),
            content: (*line).to_string(),
        });
    }
    // Trailing context (numbered on both sides — the tail is common).
    for offset in 0..(old_tail - (old.len() - suffix)) {
        let old_ix = old.len() - suffix + offset;
        let new_ix = new.len() - suffix + offset;
        if old_ix >= old_tail || new_ix >= new_tail {
            break;
        }
        lines.push(DiffLine {
            kind: DiffLineKind::Context,
            old_line: Some(old_ix as u32 + 1),
            new_line: Some(new_ix as u32 + 1),
            content: old[old_ix].to_string(),
        });
    }

    let old_start = lead as u32 + 1;
    let new_start = lead as u32 + 1;
    let old_lines = (old_tail - lead) as u32;
    let new_lines = (new_tail - lead) as u32;
    // A write that changed nothing (an agent rewriting a file byte for byte)
    // is not a diff — a card of pure context lines claims an edit that never
    // happened.
    let hunks = if lines.is_empty() || (additions == 0 && deletions == 0) {
        Vec::new()
    } else {
        vec![UnifiedHunk {
            old_start,
            old_lines,
            new_start,
            new_lines,
            header: format!("@@ -{old_start},{old_lines} +{new_start},{new_lines} @@"),
            lines,
        }]
    };
    DiffFile {
        path: path.to_string_lossy().to_string(),
        previous_path: None,
        status,
        additions,
        deletions,
        hunks,
        binary: false,
    }
}

/// Lines WITHOUT their terminators, dropping the empty tail a trailing
/// newline produces (`"a\n"` is one line, not two).
fn split_lines(text: &str) -> Vec<&str> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last().is_some_and(|line| line.is_empty()) && lines.len() > 1 {
        lines.pop();
    }
    lines.into_iter().map(|line| line.trim_end_matches('\r')).collect()
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
pub(crate) fn render_extras(
    extras: &LocalExtras,
    item: FeedItemId,
    expanded: bool,
    on_toggle: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    on_kill: Option<Box<dyn Fn(&str, &mut Window, &mut App) + 'static>>,
    cx: &App,
) -> Option<AnyElement> {
    let tool = extras.for_item(item)?;
    if tool.edits.is_empty() && tool.output.is_none() {
        return None;
    }
    let muted = cx.theme().muted_foreground;
    let mut column = v_flex().w_full().min_w_0().gap_1().pl_5().pt_1();
    for edit in &tool.edits {
        column = column.child(render_edit_card(edit, expanded, cx));
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
        column = column.child(
            div()
                .id(("session-extras-toggle", item as usize))
                .mt_0p5()
                .cursor_pointer()
                .text_xs()
                .text_color(muted)
                .child(if expanded { "Show less" } else { "Show more" })
                .on_click(on_toggle),
        );
    }
    Some(column.into_any_element())
}

fn hunk_rows(file: &DiffFile) -> usize {
    file.hunks.iter().map(|hunk| hunk.lines.len()).sum()
}

fn render_edit_card(edit: &EditCard, expanded: bool, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let mut body = v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .gap_1p5()
                .items_center()
                .text_2xs()
                .text_color(muted)
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
                ),
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

/// The pinned plan card — one row per entry, the running one marked. Rendered
/// above the composer, not in the feed: a plan is state, not an event, and it
/// is replaced wholesale every time the agent revises it.
pub(crate) fn render_plan_card(entries: &[engine::PlanEntryView], cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let mut column = v_flex().w_full().min_w_0().gap_0p5().child(
        h_flex()
            .gap_1p5()
            .items_center()
            .text_xs()
            .text_color(muted)
            .child(Icon::new(registry::CODING_PLAN).xsmall())
            .child("Plan"),
    );
    for entry in entries {
        let (glyph, tint) = match entry.status {
            engine::PlanEntryStatusView::Completed => ("✓", muted),
            engine::PlanEntryStatusView::InProgress => ("▸", cx.theme().foreground),
            engine::PlanEntryStatusView::Pending => ("·", muted),
        };
        column = column.child(
            h_flex()
                .w_full()
                .min_w_0()
                .gap_1p5()
                .items_start()
                .text_xs()
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(muted)
                        .font_family(theme::terminal::FONT_FAMILY)
                        .child(glyph),
                )
                .child(
                    div()
                        .min_w_0()
                        .text_color(tint)
                        .child(SharedString::from(entry.content.clone())),
                ),
        );
    }
    div()
        .w_full()
        .flex_shrink_0()
        .px_3()
        .py_2()
        .border_t_1()
        .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
        .child(column)
        .into_any_element()
}

/// The agent's latest thought, one clamped line above the composer. Thoughts
/// are not feed rows on purpose: they are superseded constantly, and a
/// transcript of every one of them would bury the work.
/// A thought as one plain line: codex's reasoning headlines arrive as
/// `**Bold markdown**` and the pinned line is not a markdown surface.
pub(crate) fn plain_thought(text: &str) -> String {
    let mut line = text.lines().find(|line| !line.trim().is_empty()).unwrap_or("").trim();
    loop {
        let trimmed = line
            .strip_prefix("**")
            .and_then(|rest| rest.strip_suffix("**"))
            .or_else(|| line.strip_prefix('*').and_then(|rest| rest.strip_suffix('*')))
            .or_else(|| line.strip_prefix('_').and_then(|rest| rest.strip_suffix('_')))
            .or_else(|| line.strip_prefix('#').map(|rest| rest.trim_start_matches('#')))
            .map(str::trim);
        match trimmed {
            Some(next) if next != line => line = next,
            _ => break,
        }
    }
    line.to_string()
}

pub(crate) fn render_thought(text: &str, cx: &App) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let text = plain_thought(text);
    h_flex()
        .w_full()
        .flex_shrink_0()
        .gap_1p5()
        .items_center()
        .px_3()
        .py_1p5()
        .child(
            Icon::new(registry::CODING_ASSISTANT)
                .xsmall()
                .text_color(muted.opacity(0.6)),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(text)),
        )
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

/// EXP-772: the plan/build PAIR — exactly two modes, one of them `plan`. That
/// shape (claude, and pi when it launched with the plan extension) draws a
/// compact "Plan" toggle pill instead of a two-value chip; anything else falls
/// back to the chip. Mirrored ×4 as `planModeToggle`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PlanModeToggle {
    /// The mode to switch to when turning plan ON.
    pub(crate) plan_id: String,
    /// The mode to switch back to when turning it OFF.
    pub(crate) build_id: String,
    /// Plan mode is in force right now.
    pub(crate) active: bool,
}

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

    #[test]
    fn a_thought_headline_drops_its_markdown_emphasis() {
        assert_eq!(plain_thought("**Preparing exact final question options**"), "Preparing exact final question options");
        assert_eq!(plain_thought("*soft*"), "soft");
        assert_eq!(plain_thought("## Heading\nbody"), "Heading");
        assert_eq!(plain_thought("plain words"), "plain words");
        assert_eq!(plain_thought("   "), "");
    }

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

    /// A one-line edit is a one-line diff, not a whole-file replacement — the
    /// common prefix and suffix stay context.
    #[test]
    fn an_edit_diffs_only_the_block_that_changed() {
        let file = edit_diff(
            Path::new("src/lib.rs"),
            Some("a\nb\nc\n"),
            "a\nB\nc\n",
        );
        assert_eq!(file.status, FileStatus::Modified);
        assert_eq!((file.additions, file.deletions), (1, 1));
        assert_eq!(
            rows(&file),
            vec![
                line(DiffLineKind::Context, "a"),
                line(DiffLineKind::Deletion, "b"),
                line(DiffLineKind::Addition, "B"),
                line(DiffLineKind::Context, "c"),
            ]
        );
        // The hunk header numbers both sides from the first context line.
        let hunk = &file.hunks[0];
        assert_eq!((hunk.old_start, hunk.new_start), (1, 1));
        assert_eq!((hunk.old_lines, hunk.new_lines), (3, 3));
    }

    /// A new file has no old side at all: every line is an addition and the
    /// status says Added (the card must not claim to have deleted nothing).
    #[test]
    fn a_created_file_is_all_additions() {
        let file = edit_diff(Path::new("new.rs"), None, "one\ntwo\n");
        assert_eq!(file.status, FileStatus::Added);
        assert_eq!((file.additions, file.deletions), (2, 0));
        assert_eq!(
            rows(&file),
            vec![
                line(DiffLineKind::Addition, "one"),
                line(DiffLineKind::Addition, "two"),
            ]
        );
    }

    /// An unchanged write (an agent rewriting a file with the same bytes)
    /// produces no hunk rather than a diff of nothing.
    #[test]
    fn an_identical_write_produces_no_hunk() {
        let file = edit_diff(Path::new("same.rs"), Some("a\nb\n"), "a\nb\n");
        assert!(file.hunks.is_empty());
        assert_eq!((file.additions, file.deletions), (0, 0));
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

    /// The plan is replaced wholesale, and a blank thought is no thought.
    #[test]
    fn the_plan_replaces_and_a_blank_thought_clears() {
        let entry = |content: &str| engine::PlanEntryView {
            content: content.to_string(),
            priority: engine::PlanEntryPriorityView::Medium,
            status: engine::PlanEntryStatusView::Pending,
        };
        let mut extras = LocalExtras::default();
        extras.apply(engine::LocalFeedEvent::Plan {
            entries: vec![entry("first"), entry("second")],
        });
        assert_eq!(extras.plan().len(), 2);
        extras.apply(engine::LocalFeedEvent::Plan {
            entries: vec![entry("only")],
        });
        assert_eq!(extras.plan().len(), 1);
        extras.apply(engine::LocalFeedEvent::Thought {
            message_id: None,
            text: "  weighing options  ".to_string(),
        });
        assert_eq!(extras.thought(), Some("weighing options"));
        extras.apply(engine::LocalFeedEvent::Thought {
            message_id: None,
            text: "   ".to_string(),
        });
        assert_eq!(extras.thought(), None);
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
