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
//! EXP-916: the per-edit card is gone. A run of consecutive edit calls is
//! ONE **edited-files card** ([`render_edit_card`]) over the contract's
//! [`domain::edit_card::edit_card`] — a title and one FLUSH file card per
//! path, whose rows are the SHARED diff rows ([`crate::diff::file_rows`] at
//! [`crate::diff::DiffOptions::card`], painted by
//! [`crate::diff::render_diff_row`]). So an edit reads the same inline as it
//! does on the Changes face, only compact. The card never navigates: a click
//! toggles a row IN PLACE, and only the LIVE row opens itself.
//!
//! What stays local is the live patch: the engine streams `EditDiff`s before
//! the wire's own `tool_update` lands one, so [`LocalExtras`] keeps the cut
//! patch of the call it is running and the card reads it until the wire
//! catches up.
//!
//! The patch itself is built by the shared [`steer::unified_diff`] (ACP hands
//! over `old_text`/`new_text`) and read back through the contract parser
//! [`domain::diff::parse_diff`] — the exact bytes a remote viewer gets, so
//! the local and the wire card cannot drift.

use std::collections::HashMap;
use std::path::PathBuf;

use gpui::{
    div, AnyElement, App, ClickEvent, InteractiveElement as _, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use coding::scm::DiffFile;

use crate::diff::{file_rows, render_diff_row, DiffOptions, RowShape};
use steer::feed::FeedItemId;
use steer::{truncate_unified_diff, unified_diff, TOOL_DIFF_MAX_BYTES, TOOL_DIFF_MAX_LINES};

use crate::controls::WebText as _;
use crate::icons::registry;

/// A card's own click listener — the host's `cx.listener(..)`, so this module
/// stays free of the hosting view's type.
pub(crate) type CardClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// Lines of an output card kept — the tail, because a command's verdict is at
/// the end. Same cap the CLI attach printer applies.
pub(crate) const OUTPUT_LINES_MAX: usize = 200;

/// Rows of an OUTPUT card shown before it folds behind "Show more".
const OUTPUT_PREVIEW_ROWS: usize = 12;

/// A tool call's local extras, accumulated as the engine reports them.
#[derive(Default)]
pub(crate) struct ToolExtras {
    /// EXP-916: every patch the call published so far, CONCATENATED — one
    /// unified diff with one section per file, exactly the shape the wire's
    /// own `tool_update` diff has, so the edited-files card reads either
    /// through the same contract parser.
    edits: String,
    /// The call's streamed output (`Execute` tools), tail-capped.
    output: Option<OutputCard>,
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
                if let Some(patch) = tool_edit_patch(path, old_text.as_deref(), &new_text) {
                    self.by_tool_call
                        .entry(tool_call_id)
                        .or_default()
                        .edits
                        .push_str(&patch);
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

    /// EXP-916 — the LIVE patch of `item`'s call: what the engine has written
    /// so far, before the wire's `tool_update` carries one. The edited-files
    /// card falls back to it so a running edit shows its diff on the runner
    /// instead of sitting `pending` until the call settles. `None` for every
    /// remote row (they have only the wire's).
    pub(crate) fn edit_patch(&self, item: FeedItemId) -> Option<&str> {
        self.for_item(item)
            .map(|extras| extras.edits.as_str())
            .filter(|patch| !patch.is_empty())
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

/// The patch for ONE local edit: the shared unified diff, cut to the contract
/// caps — the SAME bytes the publisher puts on the wire for a remote viewer.
/// `None` when the write changed nothing, or when the cut patch names no file
/// (there would be no row to hang it on).
fn tool_edit_patch(path: PathBuf, old_text: Option<&str>, new_text: &str) -> Option<String> {
    let display = path.to_string_lossy();
    let patch = unified_diff(&display, old_text, new_text);
    if patch.is_empty() {
        return None;
    }
    let (mut kept, omitted) =
        truncate_unified_diff(&patch, TOOL_DIFF_MAX_LINES, TOOL_DIFF_MAX_BYTES);
    // EXP-786: the publisher's own marker (`engine::mapper`), so the local
    // patch is BYTE-IDENTICAL to the one a remote viewer gets and the
    // contract parser reads the cut back out of either.
    if omitted > 0 {
        kept.push_str(&format!("\\ {omitted} more lines truncated\n"));
    }
    tool_diff_file(&kept)?;
    Some(kept)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// The card hanging off feed row `item`: the command's output.
///
/// EXP-916 took the per-edit cards out of here — a run of edits is the ONE
/// edited-files card the transcript draws over the whole run
/// ([`render_edit_card`]), never a stack of cards under one tool row. What is
/// left is the LIVE window of an `execute` call.
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
    on_toggle: CardClick,
    on_kill: Option<Box<dyn Fn(&str, &mut Window, &mut App) + 'static>>,
    cx: &App,
) -> Option<AnyElement> {
    let tool = extras.for_item(item)?;
    let output = tool.output.as_ref()?;
    let muted = cx.theme().muted_foreground;
    let mut column = v_flex().w_full().min_w_0().gap_1().pl_5().pt_1();
    // EXP-910: this renderer serves the RUNNING row only (steer_viewer's
    // `ToolRowMode::Live`), so its log is a TAIL — the last few lines, the
    // way a terminal shows a running command, instead of the whole 200-row
    // buffer under the reader's eye. The SETTLED row is
    // `render_wire_extras`' and is untouched.
    column = column.child(render_output_card(
        output, item, expanded, true, on_kill, cx,
    ));
    if output.rows().len() > OUTPUT_PREVIEW_ROWS {
        column = column.child(fold_toggle(item, expanded, on_toggle, muted));
    }
    Some(column.into_any_element())
}

/// The `Show more` / `Show less` line under a folded output card.
fn fold_toggle(
    item: FeedItemId,
    expanded: bool,
    on_toggle: CardClick,
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

// ---------------------------------------------------------------------------
// EXP-916 — the edited-files card
// ---------------------------------------------------------------------------

/// A row of an edited-files card was clicked: the path it named. `Rc` because
/// one card hangs the same listener on every one of its rows.
pub(crate) type EditRowClick = std::rc::Rc<dyn Fn(&str, &mut Window, &mut App) + 'static>;

/// EXP-916 — THE edited-files card: one glass card per [`steer::feed::FeedRow::Edits`].
///
/// `[diff glyph] {N} files edited` over one FLUSH file card per path — the
/// contract's rows ([`domain::edit_card::edit_card`]), in the contract's
/// order, at the contract's preview depth
/// ([`domain::edit_card::EDIT_CARD_PREVIEW`], then `{n} more` / `Show less`).
///
/// * a `ready` row is the shared file-card header (`letter · dir/name · +a −d
///   · chevron`) over its compact patch;
/// * a `pending` row is the path alone, its chevron muted — the call is still
///   writing and there is nothing to open;
/// * a `failed` row is the path in `danger` and the word `failed`, no chevron.
///
/// `open` is the set of paths the reader unfolded; the card itself opens
/// exactly the LIVE row (`view.live_index`), whose body is bounded to
/// [`domain::contract::DIFF_UI_INLINE_DIFF_MAX_HEIGHT`] so a running edit
/// cannot push the conversation off screen. A click toggles a row IN PLACE —
/// a card never opens the Changes face.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_edit_card(
    id: FeedItemId,
    view: &domain::edit_card::EditCardView,
    open: &std::collections::HashSet<String>,
    more_open: bool,
    on_toggle_row: EditRowClick,
    on_toggle_more: CardClick,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let preview = domain::edit_card::EDIT_CARD_PREVIEW;
    // The live row must be REACHABLE: a card whose running edit sits past the
    // preview depth stays open until the run moves on.
    let live_clamped = view.live_index.is_some_and(|index| index >= preview);
    let show_all = more_open || live_clamped;
    let shown = if show_all {
        view.rows.len()
    } else {
        view.rows.len().min(preview)
    };
    let mut card = crate::surface::glass_card()
        .w_full()
        .min_w_0()
        .gap_1()
        .px_2()
        .py_1p5()
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .gap_1p5()
                .items_center()
                .text_2xs()
                .text_color(muted)
                .child(Icon::new(registry::CODING_DIFF).xsmall())
                .child(SharedString::from(view.title.clone())),
        );
    let mut rows = v_flex().w_full().min_w_0().overflow_hidden();
    for (index, row) in view.rows.iter().take(shown).enumerate() {
        let live = view.live_index == Some(index);
        let opened = live || open.contains(&row.path);
        let mut slot = div().w_full().min_w_0();
        // A hairline is the only seam between two stacked file cards — the
        // parent card already carries the border and the radius.
        if index > 0 {
            slot = slot
                .border_t_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla());
        }
        let body = match &row.file {
            Some(file) => edit_row_ready(id, index, &row.path, file, opened, on_toggle_row.clone(), cx),
            None => edit_row_stub(row, cx),
        };
        rows = rows.child(slot.child(body));
    }
    card = card.child(rows);
    // The fold: only while the card is not holding itself open for a live row.
    if !live_clamped {
        if let Some(more) = domain::edit_card::edit_card_more_label(view.rows.len()) {
            let label = if more_open {
                domain::contract::DIFF_UI_SHOW_LESS.to_string()
            } else {
                more
            };
            card = card.child(
                div()
                    .id(("session-edit-card-more", id as usize))
                    .px_1()
                    .cursor_pointer()
                    .text_2xs()
                    .text_color(muted)
                    .child(SharedString::from(label))
                    .on_click(move |event: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        on_toggle_more(event, window, cx);
                    }),
            );
        }
    }
    card.into_any_element()
}

/// A `ready` row: the shared file-card header, plus its patch when open.
fn edit_row_ready(
    id: FeedItemId,
    index: usize,
    path: &str,
    file: &DiffFile,
    open: bool,
    on_toggle_row: EditRowClick,
    cx: &App,
) -> AnyElement {
    let options = DiffOptions::card();
    let rows = file_rows(file, &cx.theme().highlight_theme, &options);
    let Some(header) = rows.first() else {
        return div().into_any_element();
    };
    let key = id as usize * 64 + index;
    let clicked = path.to_string();
    let mut column = v_flex().w_full().min_w_0().child(
        div()
            .id(("session-edit-row", key))
            .w_full()
            .min_w_0()
            .cursor_pointer()
            .hover(|this| this.bg(cx.theme().list_hover))
            .on_click(move |_: &ClickEvent, window, cx| {
                cx.stop_propagation();
                on_toggle_row(&clicked, window, cx);
            })
            .child(render_diff_row(
                header,
                if open { RowShape::Top } else { RowShape::Only },
                gpui::px(0.),
                &options,
                cx,
            )),
    );
    if open {
        let mut body = v_flex().w_full().min_w_0();
        for row in rows.iter().skip(1) {
            body = body.child(render_diff_row(row, RowShape::Middle, gpui::px(0.), &options, cx));
        }
        column = column.child(
            div()
                .id(("session-edit-body", key))
                .w_full()
                .min_w_0()
                .max_h(gpui::px(
                    domain::contract::DIFF_UI_INLINE_DIFF_MAX_HEIGHT as f32,
                ))
                .overflow_y_scroll()
                .child(body),
        );
    }
    column.into_any_element()
}

/// A `pending` / `failed` row: the path, and what became of it. Neither can be
/// opened — there is no patch behind either.
fn edit_row_stub(row: &domain::edit_card::EditCardRow, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let failed = row.state == domain::edit_card::EditRowState::Failed;
    let (dir, name) = crate::diff::split_path(&row.path);
    let tint = if failed { theme.danger } else { theme.muted_foreground };
    let mut line = h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap_2()
        .h(gpui::px(24.))
        .px_2()
        .text_size(gpui::px(11.))
        .font_family(theme.mono_font_family.clone())
        .child(div().flex_shrink_0().w(gpui::px(12.)))
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(theme.muted_foreground)
                        .child(SharedString::from(dir)),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(tint)
                        .child(SharedString::from(name)),
                ),
        );
    if failed {
        line = line.child(
            div()
                .flex_shrink_0()
                .text_color(theme.danger)
                .child("failed"),
        );
    } else {
        // The call is still writing: a chevron that says "nothing to open".
        line = line.child(
            div().flex_shrink_0().child(
                Icon::new(registry::UI_CHEVRON_DOWN)
                    .xsmall()
                    .text_color(theme.muted_foreground.opacity(0.4)),
            ),
        );
    }
    line.into_any_element()
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
        rows.len().min(OUTPUT_PREVIEW_ROWS)
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

/// EXP-895/EXP-916 — a row's WIRE evidence: what an `execute` call printed
/// once it settled. The publisher's own cut is what shows (an output's LEADING
/// `\ N more lines truncated` marker) and nothing is re-truncated here.
///
/// This is the SETTLED row's renderer on every client, the desktop runner
/// included (EXP-895 runner parity): [`LocalExtras`]'s own output card serves
/// only the pre-settle live window, where it streams. Collapsed, the row is
/// its headline alone; the reader's Show more opens the log. An EDIT row has
/// nothing here — its patch belongs to the edited-files card (EXP-916).
pub(crate) fn render_wire_extras(
    output: Option<&str>,
    item: FeedItemId,
    expanded: bool,
    on_toggle: CardClick,
    cx: &App,
) -> Option<AnyElement> {
    let printed = output.filter(|text| !text.trim().is_empty())?;
    let muted = cx.theme().muted_foreground;
    let mut column = v_flex().w_full().min_w_0().gap_1().pl_5().pt_1();
    // A settled log is COMPACT until it is asked for: the headline already
    // says what ran and whether it failed.
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
    column = column.child(fold_toggle(item, expanded, on_toggle, muted));
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
    use crate::diff::RenderRow;
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

    /// EXP-916: a local edit is a cut PATCH now; the card reads it back
    /// through the contract parser, exactly as it reads the wire's.
    fn card(old: Option<&str>, new: &str) -> Option<(DiffFile, usize)> {
        let patch = tool_edit_patch(PathBuf::from("src/lib.rs"), old, new)?;
        tool_diff_file(&patch)
    }

    /// A one-line edit is a one-line diff, not a whole-file replacement — the
    /// shared patch keeps the common prefix and suffix as context.
    #[test]
    fn an_edit_diffs_only_the_block_that_changed() {
        let (file, omitted) = card(Some("a\nb\nc\n"), "a\nB\nc\n").expect("an edit");
        assert_eq!(file.path, "src/lib.rs");
        assert_eq!(file.status, DiffStatus::Modified);
        assert_eq!((file.additions, file.deletions), (1, 1));
        assert_eq!(omitted, 0);
        assert_eq!(
            rows(&file),
            vec![
                line(DiffLineKind::Context, "a"),
                line(DiffLineKind::Del, "b"),
                line(DiffLineKind::Add, "B"),
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
        let (file, _) = card(None, "one\ntwo\n").expect("a new file");
        assert_eq!(file.status, DiffStatus::Added);
        assert_eq!((file.additions, file.deletions), (2, 0));
        assert_eq!(
            rows(&file),
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

    /// EXP-916: a card ROW is the shared file-card header — the same one the
    /// Changes face draws — over the patch, rendered FLUSH (the edited-files
    /// card is the frame). The copy and the depths are the contract's.
    #[test]
    fn an_edit_card_row_is_the_shared_file_card_header() {
        let (file, _) = card(Some("a\nb\nc\n"), "a\nB\nc\n").expect("an edit");
        let options = DiffOptions::card();
        assert!(options.flush, "a card's files stack inside the card's frame");
        assert!(options.compact, "and at the card's density");
        let rows = file_rows(
            &file,
            &gpui_component::highlighter::HighlightTheme::default_dark(),
            &options,
        );
        assert!(
            matches!(rows.first(), Some(RenderRow::FileHeader { .. })),
            "the first row is the file card's header"
        );
        assert!(rows.len() > 1, "and the patch has a body under it");

        // The card's own numbers and words are the contract's, never this
        // module's (`domain::edit_card` is the ×4 mirror).
        let members = [
            domain::edit_card::EditCardMember {
                id: 1,
                detail: Some("src/lib.rs"),
                diff: None,
                settled: false,
            },
        ];
        let view = domain::edit_card::edit_card(&members, Some(1));
        assert_eq!(view.title, domain::contract::DIFF_UI_EDITED_FILES_ONE);
        assert_eq!(view.rows[0].state, domain::edit_card::EditRowState::Pending);
        assert_eq!(view.live_index, Some(0), "the live row opens itself");
        assert_eq!(domain::edit_card::EDIT_CARD_PREVIEW, 5);
        assert_eq!(domain::edit_card::edit_card_more_label(5), None);
        assert_eq!(
            domain::edit_card::edit_card_more_label(8).as_deref(),
            Some("3 more")
        );
        assert_eq!(domain::contract::DIFF_UI_SHOW_LESS, "Show less");
        assert_eq!(domain::contract::DIFF_UI_INLINE_DIFF_MAX_HEIGHT, 288);
    }

    /// EXP-786: a local edit is cut to the SAME caps the wire applies, and
    /// the card knows how many lines it lost — a local viewer and a remote
    /// one show the same prefix of one edit.
    #[test]
    fn a_long_edit_is_cut_to_the_contract_caps() {
        let new: String = (0..TOOL_DIFF_MAX_LINES * 2)
            .map(|n| format!("line {n}\n"))
            .collect();
        let (file, omitted) = card(None, &new).expect("a big new file");
        // Every line of a new file is an addition, so the kept ones are
        // exactly what survived the cap — plus, on a CUT patch, the one
        // empty context row the contract parser fills the declared hunk
        // range with.
        let kept = rows(&file)
            .iter()
            .filter(|(kind, _)| *kind == DiffLineKind::Add)
            .count();
        assert!(kept < TOOL_DIFF_MAX_LINES * 2, "{kept} rows were kept");
        assert_eq!(kept + omitted, TOOL_DIFF_MAX_LINES * 2);
        assert!(omitted > 0);
        // A short edit loses nothing.
        assert_eq!(self::card(None, "x\n").expect("tiny").1, 0);
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
