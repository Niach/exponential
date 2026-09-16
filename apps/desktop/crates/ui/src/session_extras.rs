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
//! wire's own per-call diff and settled output through the same cards
//! ([`render_wire_extras`]).
//!
//! The join is the tricky part and is worth stating once: the wire `tool`
//! event carries no id (`FeedKind::Tool` is `{name, detail, subagent_id}`),
//! and a `FeedItemId` is a local sequence number the engine cannot know. So
//! the engine tags each activity with the ACP `tool_call_id` it belongs to,
//! the drain records `feed item → tool call` as the row lands, and the extras
//! key off the FEED ITEM. Nothing tries to match on titles or text.
//!
//! EXP-895: the card no longer paints its own lines. It renders the SHARED
//! rows ([`crate::diff::file_rows`] at [`crate::diff::DiffOptions::card`],
//! painted by [`crate::diff::render_diff_row`]) — the same anatomy the
//! Changes face shows, only compact and unhighlighted — so an edit reads the
//! same inline as it does in the pane. What stays local is the hosting: a
//! card is rows, never a [`crate::diff::DiffView`] entity with its own scroll
//! and file list (one per edit in a long run is a lot of state), and it is
//! COLLAPSED to its header until the row's Show more opens it.
//!
//! The patch itself is built by the shared [`steer::unified_diff`] (ACP hands
//! over `old_text`/`new_text`) and read back through the contract parser
//! [`domain::diff::parse_diff`] — the exact bytes a remote viewer gets, so
//! the local and the wire card cannot drift.

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

use coding::scm::DiffFile;

use crate::diff::{file_rows, render_diff_row, DiffOptions, RenderRow, RowShape};
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

    /// EXP-895 — the WIRE's settled output as a card, so a local and a remote
    /// viewer of one run read the SAME thing once a command ends. The publisher
    /// already redacted it and cut it to the contract's caps; its leading
    /// `\ N more lines truncated` marker is simply the log's first line. No exit
    /// code rides the wire, so the tool row's own `failed` tint is the verdict.
    fn from_wire(text: &str) -> Self {
        let mut card = OutputCard::default();
        card.push(text);
        card.finish(None);
        card
    }
}

// ---------------------------------------------------------------------------
// old_text/new_text → one cut patch → the card's DiffFile (pure)
// ---------------------------------------------------------------------------

/// Read ONE per-call patch (the shape [`unified_diff`] emits: `---`/`+++`
/// headers and hunks, no `diff --git` line) into the shared model, plus the
/// lines the publisher's `\ N more lines truncated` marker (EXP-786) says
/// were dropped. Both are the CONTRACT parser's job now
/// ([`domain::diff::parse_diff`] opens a bare section and reads the marker
/// back as `truncated_lines`); `None` when the patch names no file or holds
/// no hunk.
pub(crate) fn tool_diff_file(patch: &str) -> Option<(DiffFile, usize)> {
    let diff = domain::diff::parse_diff(patch);
    let omitted = diff.truncated_lines.unwrap_or(0) as usize;
    let file = diff.files.into_iter().next()?;
    (!file.path.is_empty() && !file.hunks.is_empty()).then_some((file, omitted))
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
    let (file, _) = tool_diff_file(&kept)?;
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
        // EXP-910: this renderer serves the RUNNING row only (steer_viewer's
        // `ToolRowMode::Live`), so its log is a TAIL — the last few lines, the
        // way a terminal shows a running command, instead of the whole 200-row
        // buffer under the reader's eye. The SETTLED row is
        // `render_wire_extras`' and is untouched.
        column = column.child(render_output_card(
            output, item, expanded, true, on_kill, cx,
        ));
    }
    // EXP-895: an edit card collapses to its HEADER, so any patch at all is
    // worth a toggle (the output card still folds only when it overflows).
    let foldable = tool.edits.iter().any(|edit| hunk_rows(&edit.file) > 0)
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

/// EXP-895 — ONE edit, rendered through the SHARED diff rows at
/// [`DiffOptions::card`]: the file-card header (status letter, dimmed dir +
/// basename, `+N −M`, chevron) over compact unified lines, exactly the
/// anatomy the Changes face shows.
///
/// A card is COLLAPSED to its header until the tool row's "Show more" opens
/// it — a transcript is prose with evidence hanging off it, not a diff
/// viewer; the header click opens the pane at this edit, where the whole
/// patch lives. Open, the body stops at [`DIFF_PREVIEW_ROWS`] and says how
/// much it is holding back (the publisher's own cut, EXP-786, counted in the
/// same caption).
/// How many BODY rows an edit card shows (pure): none while it is collapsed
/// — the card is its header — and at most [`DIFF_PREVIEW_ROWS`] while it is
/// open, because a transcript is prose with evidence hanging off it and the
/// pane is where a whole patch is read.
fn card_body_rows(rows: usize, expanded: bool) -> usize {
    if expanded {
        rows.saturating_sub(1).min(DIFF_PREVIEW_ROWS)
    } else {
        0
    }
}

fn render_edit_card(
    edit: &EditCard,
    id: (FeedItemId, usize),
    expanded: bool,
    on_open: Option<OpenDiff>,
    cx: &App,
) -> AnyElement {
    let options = DiffOptions::card();
    let rows = file_rows(&edit.file, &cx.theme().highlight_theme, &options);
    let Some(header) = rows.first() else {
        return div().into_any_element();
    };

    // Collapsed: the header IS the whole card (one `glass_row_card`-shaped
    // row). Open: header, body, and a trailing note for everything not shown.
    let shown = card_body_rows(rows.len(), expanded);
    let body: Vec<&RenderRow> = rows.iter().skip(1).take(shown).collect();
    let hidden = if expanded {
        rows.len().saturating_sub(1 + body.len()) + edit.omitted
    } else {
        0
    };
    let note = (hidden > 0).then(|| RenderRow::Note {
        message: SharedString::from(omitted_caption(hidden)),
    });
    let tail = note.is_some();

    let header_shape = if body.is_empty() && !tail {
        RowShape::Only
    } else {
        RowShape::Top
    };
    // EXP-862: the header is the way INTO the diff pane, scoped to this edit
    // — the card shows a preview of one patch, the pane shows the patch.
    let mut card = v_flex().w_full().min_w_0().child(
        div()
            .id(("session-edit-card", id.0 as usize * 64 + id.1))
            .w_full()
            .min_w_0()
            .when_some(on_open, |header, on_open| {
                header
                    .cursor_pointer()
                    .hover(|this| this.bg(cx.theme().list_hover))
                    .on_click(move |event: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        on_open(event, window, cx);
                    })
            })
            .child(render_diff_row(header, header_shape, gpui::px(0.), &options, cx)),
    );
    let last = body.len().saturating_sub(1);
    for (index, row) in body.iter().enumerate() {
        let shape = if index == last && !tail {
            RowShape::Bottom
        } else {
            RowShape::Middle
        };
        card = card.child(render_diff_row(row, shape, gpui::px(0.), &options, cx));
    }
    if let Some(note) = note {
        card = card.child(render_diff_row(&note, RowShape::Bottom, gpui::px(0.), &options, cx));
    }
    card.into_any_element()
}

fn render_output_card(
    output: &OutputCard,
    item: FeedItemId,
    expanded: bool,
    // EXP-910: the call is still RUNNING — the card is bounded to the
    // contract's `steerFeed.liveToolOutputTailLines`, whatever `expanded`
    // says, and opens with a lone `…` when there was more.
    live_tail: bool,
    on_kill: Option<Box<dyn Fn(&str, &mut Window, &mut App) + 'static>>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let rows = output.rows();
    let shown = if live_tail {
        rows.len().min(steer::feed::LIVE_TOOL_OUTPUT_TAIL_LINES)
    } else if expanded {
        rows.len()
    } else {
        rows.len().min(DIFF_PREVIEW_ROWS)
    };
    // The TAIL is what matters, so a collapsed card shows the last rows.
    let skip = rows.len().saturating_sub(shown);
    let mut lines = v_flex().w_full().min_w_0();
    // EXP-910: the live tail says so — a lone `…` where the earlier lines are,
    // the same shape the wire's `\ N more lines truncated` marker has, so it
    // reads as the log's first line rather than as chrome. Mirrored ×4
    // (`steer::feed::live_tool_output_tail`).
    if live_tail && skip > 0 {
        lines = lines.child(
            div()
                .w_full()
                .min_w_0()
                .text_2xs()
                .text_color(muted)
                .font_family(theme::terminal::FONT_FAMILY)
                .child("…"),
        );
    }
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

/// EXP-895 — ONE parse per feed row, not one per repaint. A remote viewer's
/// transcript re-renders on every frame of a live run, and a `tool` row's
/// patch never changes once it has landed; the raw string is the cache key,
/// so a row that is somehow rewritten still re-parses. Thread-local: the
/// transcript is painted on the foreground only, and the cache dies with the
/// window rather than outliving it in a global.
fn wire_edit(item: FeedItemId, patch: &str) -> Option<std::rc::Rc<EditCard>> {
    use std::hash::{Hash as _, Hasher as _};
    thread_local! {
        static WIRE_EDITS: std::cell::RefCell<HashMap<FeedItemId, (u64, Option<std::rc::Rc<EditCard>>)>> =
            std::cell::RefCell::new(HashMap::new());
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    patch.hash(&mut hasher);
    let key = hasher.finish();
    WIRE_EDITS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((held, card)) = cache.get(&item) {
            if *held == key {
                return card.clone();
            }
        }
        let card = tool_diff_file(patch).map(|(file, omitted)| {
            std::rc::Rc::new(EditCard {
                path: PathBuf::from(file.path.clone()),
                file,
                omitted,
            })
        });
        // The feed itself is capped (EXP-783), so this follows it: one entry
        // per live row, evicted wholesale once a transcript grows past it.
        if cache.len() > 512 {
            cache.clear();
        }
        cache.insert(item, (key, card.clone()));
        card
    })
}

/// EXP-786/895 — a row's WIRE evidence: the per-call patch the publisher cut
/// and, once the call settled, what an `execute` call printed. Both go through
/// the very cards a local run gets, so a local and a remote viewer of one run
/// show the same thing; the cuts are the publisher's (a patch's trailing
/// `\ N more lines truncated` marker, an output's LEADING one) and nothing is
/// re-truncated here. An unparseable patch renders nothing rather than a broken
/// card.
///
/// This is the SETTLED row's renderer on every client, the desktop runner
/// included (EXP-895 runner parity): [`LocalExtras`]'s own output card serves
/// only the pre-settle live window, where it streams. Collapsed, the row is its
/// headline plus the edit card's header (`+a −b`); the reader's Show more opens
/// the patch and the log.
pub(crate) fn render_wire_extras(
    diff: Option<&str>,
    output: Option<&str>,
    item: FeedItemId,
    expanded: bool,
    on_toggle: Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    on_open: Option<OpenDiff>,
    cx: &App,
) -> Option<AnyElement> {
    let edit = diff.and_then(|diff| wire_edit(item, diff));
    let printed = output.filter(|text| !text.trim().is_empty());
    if edit.is_none() && printed.is_none() {
        return None;
    }
    let muted = cx.theme().muted_foreground;
    let mut column = v_flex().w_full().min_w_0().gap_1().pl_5().pt_1();
    let mut foldable = false;
    if let Some(edit) = edit.as_ref() {
        column = column.child(render_edit_card(edit, (item, 0), expanded, on_open, cx));
        foldable = foldable || hunk_rows(&edit.file) > 0;
    }
    if let Some(printed) = printed {
        // A settled log is COMPACT until it is asked for: the headline already
        // says what ran and whether it failed.
        foldable = true;
        if expanded {
            column = column.child(render_output_card(
                &OutputCard::from_wire(printed),
                item,
                true,
                false,
                None,
                cx,
            ));
        }
    }
    if foldable {
        column = column.child(fold_toggle(item, expanded, on_toggle, muted));
    }
    Some(column.into_any_element())
}

/// The read of ONE steering value: the session MODE ([`mode_chip`]) or one
/// `config_state.options` entry ([`option_chip`]). EXP-772 took the mid-session
/// pickers away; EXP-877 brought back exactly one read, the model.
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

/// EXP-877 — the read of ONE `config_state.options` entry by id, generalising
/// [`mode_chip`] over the option list instead of the mode list.
///
/// `value_label` resolves the same way the mode chip's does: the matching
/// `values` entry's label, else the raw value, else the "default" label. The
/// engine publishes `model` VALUE-ONLY (no `values`, because switching is a
/// `/model <alias>` message rather than a menu pick), so the middle branch is
/// the live one — but a publisher that does describe its values still reads
/// as its own labels.
pub(crate) fn option_chip(config: Option<&steer::SessionConfig>, id: &str) -> Option<ConfigChip> {
    let option = config?.options.iter().find(|option| option.id == id)?;
    let value = option.value.clone().unwrap_or_default();
    let values = option.values.clone().unwrap_or_default();
    let value_label = values
        .iter()
        .find(|entry| entry.id == value)
        .map(|entry| entry.label.clone())
        .filter(|label| !label.is_empty())
        .or_else(|| (!value.is_empty()).then(|| value.clone()))
        .unwrap_or_else(|| crate::slash_commands::CONFIG_DEFAULT_VALUE_LABEL.to_string());
    Some(ConfigChip {
        value,
        label: option.label.clone(),
        value_label,
        values,
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

#[cfg(test)]
mod tests {
    use super::*;
    use domain::diff::{DiffLineKind, DiffStatus};

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

    fn line(kind: DiffLineKind, text: &str) -> (DiffLineKind, String) {
        (kind, text.to_string())
    }

    fn rows(file: &DiffFile) -> Vec<(DiffLineKind, String)> {
        file.hunks
            .iter()
            .flat_map(|hunk| hunk.lines.iter())
            .map(|line| (line.kind, line.text.clone()))
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
        assert_eq!(card.file.status, DiffStatus::Modified);
        assert_eq!((card.file.additions, card.file.deletions), (1, 1));
        assert_eq!(card.omitted, 0);
        assert_eq!(
            rows(&card.file),
            vec![
                line(DiffLineKind::Context, "a"),
                line(DiffLineKind::Del, "b"),
                line(DiffLineKind::Add, "B"),
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
        assert_eq!(card.file.status, DiffStatus::Added);
        assert_eq!((card.file.additions, card.file.deletions), (2, 0));
        assert_eq!(
            rows(&card.file),
            vec![
                line(DiffLineKind::Add, "one"),
                line(DiffLineKind::Add, "two"),
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

    /// EXP-895: an edit card is COLLAPSED to its header — one file-card row,
    /// the same one the Changes face draws — until the row's Show more opens
    /// it, and even open it stops at the preview cap.
    #[test]
    fn an_edit_card_is_collapsed_to_its_header() {
        let card = card(Some("a\nb\nc\n"), "a\nB\nc\n").expect("an edit");
        let rows = file_rows(
            &card.file,
            &gpui_component::highlighter::HighlightTheme::default_dark(),
            &DiffOptions::card(),
        );
        assert!(
            matches!(rows.first(), Some(RenderRow::FileHeader { .. })),
            "the first row is the file card's header"
        );
        assert!(rows.len() > 1, "and the patch has a body under it");
        assert_eq!(card_body_rows(rows.len(), false), 0);
        assert_eq!(card_body_rows(rows.len(), true), rows.len() - 1);
        // A long patch stops at the preview cap; the note says how much is
        // held back.
        assert_eq!(card_body_rows(500, true), DIFF_PREVIEW_ROWS);
        assert_eq!(omitted_caption(3), "… 3 more lines");
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
        // Every line of a new file is an addition, so the kept ones are
        // exactly what survived the cap — plus, on a CUT patch, the one
        // empty context row the contract parser fills the declared hunk
        // range with.
        let kept = rows(&card.file)
            .iter()
            .filter(|(kind, _)| *kind == DiffLineKind::Add)
            .count();
        assert!(kept < TOOL_DIFF_MAX_LINES * 2, "{kept} rows were kept");
        assert_eq!(kept + card.omitted, TOOL_DIFF_MAX_LINES * 2);
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
        let (file, omitted) = tool_diff_file(patch).expect("a hunk");
        assert_eq!(file.path, "src/x.rs");
        assert_eq!(omitted, 7);
        assert_eq!(
            rows(&file),
            vec![
                line(DiffLineKind::Context, "a"),
                line(DiffLineKind::Del, "b"),
                line(DiffLineKind::Add, "B"),
            ]
        );
        // No file, no card; a hunk-less patch is no card either.
        assert!(tool_diff_file("nothing here").is_none());
        assert!(tool_diff_file("--- a/x\n+++ b/x\n").is_none());
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

    /// EXP-895: the WIRE's settled output reads as a card of its own — the
    /// publisher's leading `\ N more lines truncated` marker is simply the
    /// log's first line, nothing is live, and no exit code rides the wire.
    #[test]
    fn the_wire_output_becomes_a_finished_card_marker_line_and_all() {
        let card = OutputCard::from_wire("\\ 12 more lines truncated\n42 passed\n");
        assert_eq!(
            card.rows(),
            vec!["\\ 12 more lines truncated".to_string(), "42 passed".to_string()]
        );
        assert!(!card.live, "a settled log never claims to run");
        assert_eq!(card.exit_code, None, "no exit code rides the wire");
        // A log with no trailing newline keeps its last word, and the card is
        // still capped at the local window.
        assert_eq!(OutputCard::from_wire("done").rows(), vec!["done".to_string()]);
        let long: String = (0..OUTPUT_LINES_MAX + 20).map(|n| format!("l{n}\n")).collect();
        assert_eq!(OutputCard::from_wire(&long).rows().len(), OUTPUT_LINES_MAX);
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
            // EXP-877: the option list carries the `model` value and nothing
            // else; the mode tests below do not need it.
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

    /// EXP-877: the composer's model read. A publisher that describes its
    /// values reads as their LABELS; the engine's value-only `model` (the live
    /// case — switching is a `/model <alias>` message, not a menu) reads as
    /// the raw value; a blank value falls back to the default label; an
    /// option nobody published is `None`, and the picker then draws nothing.
    #[test]
    fn the_model_option_reads_its_value() {
        let with = |options: Vec<steer::frames::ConfigOption>| steer::SessionConfig {
            options,
            current_mode: None,
            modes: Vec::new(),
            commands: Vec::new(),
        };
        let described = with(vec![steer::frames::ConfigOption {
            value: Some("opus".to_string()),
            values: Some(vec![
                steer::frames::ConfigValue::new("fable", "Fable"),
                steer::frames::ConfigValue::new("opus", "Opus"),
                steer::frames::ConfigValue::new("sonnet", "Sonnet"),
            ]),
            ..steer::frames::ConfigOption::new("model", "Model")
        }]);
        let chip = option_chip(Some(&described), "model").expect("the model option");
        assert_eq!(
            (chip.label.as_str(), chip.value.as_str(), chip.value_label.as_str()),
            ("Model", "opus", "Opus")
        );
        assert_eq!(chip.values.len(), 3);

        // Value only (what the engine publishes): the value names itself and
        // there is no menu on the wire.
        let bare = with(vec![steer::frames::ConfigOption {
            value: Some("sonnet".to_string()),
            ..steer::frames::ConfigOption::new("model", "Model")
        }]);
        let chip = option_chip(Some(&bare), "model").expect("the model option");
        assert_eq!(chip.value_label, "sonnet");
        assert!(chip.values.is_empty());

        // Blank = the CLI's own default (codex with no `-m`).
        let blank = with(vec![steer::frames::ConfigOption {
            value: Some(String::new()),
            ..steer::frames::ConfigOption::new("model", "Model")
        }]);
        assert_eq!(
            option_chip(Some(&blank), "model").expect("the model option").value_label,
            crate::slash_commands::CONFIG_DEFAULT_VALUE_LABEL
        );

        // Nothing published under that id, and no config at all.
        assert!(option_chip(Some(&bare), "effort").is_none());
        assert!(option_chip(None, "model").is_none());
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
}
