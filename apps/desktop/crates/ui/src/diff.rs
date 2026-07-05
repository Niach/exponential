//! Side-by-side PR diff view (masterplan-v3 §7.8, DC-7) — the standalone,
//! READ-ONLY review component behind the Phase-5 gate bullet "diff renders
//! side-by-side highlighted from a real PR".
//!
//! Data: `issues.prFiles({ issueId })` via [`api::issues::pr_files`] — the
//! same `PullFile` shape the web `components/diff-view.tsx` consumes (that
//! component is a data-shape reference only; it is currently unwired on web).
//!
//! Pipeline: `diff/patch.rs` (pure unified-patch parse + old/new alignment
//! with filler rows) → `diff/highlight.rs` (Tree-sitter syntax highlight per
//! side, language from filename) → ONE flat virtualized row list where every
//! line row renders the old column left / new column right at 50% each.
//!
//! Deliberate §7.8 delta, documented: the spec sketch says "a resizable split
//! of two VirtualLists". Two independent `VirtualList`s cannot be
//! scroll-locked with the component's current API (each owns its scroll
//! handle; reconciling two live wheel targets per frame drifts), so this
//! renders a SINGLE `v_virtual_list` whose rows contain both cells —
//! row-alignment is structural (it can never desync), virtualization is
//! preserved, and the 50/50 split mirrors GitHub's split view. Screens embed
//! [`DiffView`] (Phase-3 track) and can pair it with a file list driven by
//! [`DiffView::files`] + [`DiffView::scroll_to_file`].
//!
//! READ-ONLY in v1, but every populated cell carries its [`Anchor`]
//! `{ filename, side, line }` (§7.8: future review-comment write-back is a
//! pure addition). No comment UI exists here.

mod highlight;
mod patch;

pub use highlight::language_for_filename;
pub use patch::{Anchor, CellKind, Side};

use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use std::cell::RefCell;

use gpui::{
    div, point, px, size, AnyElement, App, FocusHandle, Focusable, HighlightStyle,
    InteractiveElement as _, IntoElement, ParentElement, Pixels, Point, Render, ScrollStrategy,
    ScrollWheelEvent, SharedString, Size, Styled, StyledText, Window,
};
use gpui_component::{
    h_flex,
    scroll::{ScrollableElement as _, ScrollbarAxis, ScrollbarHandle},
    theme::Colorize as _,
    v_flex, v_virtual_list, ActiveTheme as _, ElementExt as _, VirtualListScrollHandle,
};

use api::issues::PullFile;
use coding::scm::{DiffFile, DiffLine, DiffLineKind, UnifiedHunk};
use gpui_component::highlighter::HighlightTheme;

// ---------------------------------------------------------------------------
// Row model (built once per set_files, rendered per visible range)
// ---------------------------------------------------------------------------

/// Compact-density metrics. Web renders the patch at `text-[0.6875rem]`
/// (11px) mono — mirrored here; rows are fixed-height so the virtual list can
/// pre-size (§7.8 "files can be huge — virtualize").
const CODE_TEXT_SIZE: f32 = 11.0;
const LINE_ROW_H: f32 = 18.0;
const FILE_HEADER_H: f32 = 26.0;
const NOTE_ROW_H: f32 = 24.0;
const FILE_GAP_H: f32 = 8.0;
/// Line-number gutter width — 4 digits + padding at 11px mono.
const GUTTER_W: f32 = 40.0;
/// Estimated mono advance at 11px — sizes rows for horizontal scrolling
/// (slightly generous so the longest line never clips at the right edge).
const CHAR_W: f32 = 6.8;

/// One populated cell of a line row, highlight spans precomputed. The
/// [`Anchor`] is carried on every cell (§7.8 read-only-but-anchored mandate).
struct RenderCell {
    kind: CellKind,
    anchor: Anchor,
    text: SharedString,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
}

enum RenderRow {
    /// Per-file header: path (+ `old → new` for renames), status badge,
    /// `+additions −deletions` (web `FilePatch` header parity).
    FileHeader {
        path: SharedString,
        previous_path: Option<SharedString>,
        status: SharedString,
        additions: u32,
        deletions: u32,
    },
    /// Web parity: `Renamed.` / `No textual diff (binary or too large).`
    Note { message: SharedString },
    /// The verbatim `@@ … @@` line, spanning both columns.
    HunkHeader { header: SharedString },
    /// Old column left, new column right; `None` = filler opposite an
    /// unpaired add/remove.
    Line {
        left: Option<RenderCell>,
        right: Option<RenderCell>,
    },
    /// Spacer between files.
    FileGap,
}

impl RenderRow {
    fn height(&self) -> Pixels {
        match self {
            RenderRow::FileHeader { .. } => px(FILE_HEADER_H),
            RenderRow::Note { .. } => px(NOTE_ROW_H),
            RenderRow::HunkHeader { .. } | RenderRow::Line { .. } => px(LINE_ROW_H),
            RenderRow::FileGap => px(FILE_GAP_H),
        }
    }
}

/// Public per-file summary — feeds the §7.8 side file list (owned by the
/// screen embedding this component) and `scroll_to_file`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSummary {
    pub filename: SharedString,
    /// GitHub status string: `added` / `modified` / `renamed` / `removed` / …
    pub status: SharedString,
    pub additions: u32,
    pub deletions: u32,
    /// Index of the file's header row in the rendered list (scroll target).
    pub row_index: usize,
}

/// Prebuilt render rows + per-file summaries, produced off the foreground by
/// [`build_pr_diff`] / [`build_scm_diff`] and installed with
/// [`DiffView::set_prepared`]. Holds the (Tree-sitter-heavy) build result so
/// the highlight pass runs on the background executor and only the cheap swap
/// touches the UI thread. Opaque on purpose — `RenderRow` stays private.
pub struct PreparedDiff {
    rows: Vec<RenderRow>,
    summaries: Vec<FileSummary>,
}

enum Phase {
    Loading,
    Error(SharedString),
    Ready,
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// The standalone side-by-side diff component. Construct with [`new`], then
/// either install a background-built [`PreparedDiff`] via [`set_prepared`]
/// (see [`build_pr_diff`] / [`build_scm_diff`]), push an error via
/// [`set_error`], or let it fetch itself via [`fetch`]. Read-only; embeds
/// anywhere (issue detail "Changes" tab, a popped-out review window).
///
/// [`new`]: DiffView::new
/// [`set_prepared`]: DiffView::set_prepared
/// [`set_error`]: DiffView::set_error
/// [`fetch`]: DiffView::fetch
pub struct DiffView {
    focus_handle: FocusHandle,
    phase: Phase,
    rows: Vec<RenderRow>,
    sizes: Rc<Vec<Size<Pixels>>>,
    files: Vec<FileSummary>,
    /// Widest cell text (px) — sizes each side's inner (scrolled) content.
    cell_text_w: f32,
    /// The SHARED horizontal offset both columns scroll by (JetBrains-style
    /// synced split): one draggable bar + wheel input, the 50/50 split stays
    /// pinned to the pane.
    h_scroll: SharedHScroll,
    scroll: VirtualListScrollHandle,
}

// ---------------------------------------------------------------------------
// SharedHScroll — one offset, two synced columns
// ---------------------------------------------------------------------------

/// A [`ScrollbarHandle`] over a plain shared offset: the horizontal scrollbar
/// drags it, the wheel handler nudges it, and both diff columns translate
/// their inner content by HALF of it (the bar's range spans both columns, so
/// full drag = full per-column scroll). Offsets are ≤ 0, matching the
/// convention every gpui scroll handle uses.
#[derive(Clone, Default)]
struct SharedHScroll {
    inner: Rc<RefCell<SharedHScrollState>>,
}

#[derive(Default)]
struct SharedHScrollState {
    x: Pixels,
    /// Full two-column content width (both cell contents + gutters + divider).
    content_w: Pixels,
    /// The pane width as of the last prepaint (clamp input).
    viewport_w: Pixels,
}

impl SharedHScroll {
    /// The per-COLUMN translation for the current offset.
    fn column_shift(&self) -> Pixels {
        self.inner.borrow().x * 0.5
    }

    fn set_content_width(&self, width: Pixels) {
        let mut state = self.inner.borrow_mut();
        if state.content_w != width {
            state.content_w = width;
            state.x = px(0.);
        }
    }

    fn set_viewport_width(&self, width: Pixels) {
        self.inner.borrow_mut().viewport_w = width;
        self.clamp();
    }

    fn scroll_by(&self, dx: Pixels) {
        self.inner.borrow_mut().x += dx;
        self.clamp();
    }

    fn clamp(&self) {
        let mut state = self.inner.borrow_mut();
        let min = (state.viewport_w - state.content_w).min(px(0.));
        state.x = state.x.clamp(min, px(0.));
    }
}

impl ScrollbarHandle for SharedHScroll {
    fn offset(&self) -> Point<Pixels> {
        point(self.inner.borrow().x, px(0.))
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.inner.borrow_mut().x = offset.x;
        self.clamp();
    }

    fn content_size(&self) -> Size<Pixels> {
        size(self.inner.borrow().content_w, px(0.))
    }
}

impl DiffView {
    /// A new, empty view in the loading state.
    pub fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            phase: Phase::Loading,
            rows: Vec::new(),
            sizes: Rc::new(Vec::new()),
            files: Vec::new(),
            cell_text_w: 0.,
            h_scroll: SharedHScroll::default(),
            scroll: VirtualListScrollHandle::new(),
        }
    }

    /// Back to the loading state (e.g. before a re-fetch).
    pub fn set_loading(&mut self, cx: &mut gpui::Context<Self>) {
        self.phase = Phase::Loading;
        self.rows.clear();
        self.sizes = Rc::new(Vec::new());
        self.files.clear();
        cx.notify();
    }

    /// Show a load failure (web parity: "Couldn’t load changes: …").
    pub fn set_error(&mut self, message: impl Into<SharedString>, cx: &mut gpui::Context<Self>) {
        self.phase = Phase::Error(message.into());
        self.rows.clear();
        self.sizes = Rc::new(Vec::new());
        self.files.clear();
        cx.notify();
    }

    /// Install a [`PreparedDiff`] built off the foreground (via
    /// [`build_pr_diff`] / [`build_scm_diff`]) and flip to Ready. The
    /// Tree-sitter-heavy parse+highlight is already done — this is only the
    /// cheap pointer swap on the UI thread, mirroring the foreground half of
    /// [`fetch`]. Callers own the background build so the highlight never
    /// blocks the foreground (matching [`fetch`]'s pattern).
    ///
    /// [`fetch`]: DiffView::fetch
    pub fn set_prepared(&mut self, prepared: PreparedDiff, cx: &mut gpui::Context<Self>) {
        self.set_rows(prepared.rows, prepared.summaries, cx);
    }

    /// Install prebuilt rows (from [`build_rows`]) and flip to Ready.
    fn set_rows(
        &mut self,
        rows: Vec<RenderRow>,
        summaries: Vec<FileSummary>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.sizes = Rc::new(rows.iter().map(|r| size(px(100.), r.height())).collect());
        self.cell_text_w = required_cell_text_width(&rows);
        self.h_scroll
            .set_content_width(px(2. * (GUTTER_W + self.cell_text_w) + 1.));
        self.rows = rows;
        self.files = summaries;
        self.phase = Phase::Ready;
        cx.notify();
    }

    /// Fetch `issues.prFiles` for `issue_id` on the background executor and
    /// populate the view. Both the blocking HTTP (§3.5) AND the row build
    /// (per-file Tree-sitter parses — seconds on a 100-file PR) stay off the
    /// foreground; only the cheap `set_rows` swap runs on the UI thread.
    pub fn fetch(
        &mut self,
        client: Arc<api::TrpcClient>,
        issue_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        self.set_loading(cx);
        let theme = cx.theme().highlight_theme.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::issues::pr_files(&client, &issue_id)
                        .map(|pr| build_rows(&pr.files, &theme))
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok((rows, summaries)) => this.set_rows(rows, summaries, cx),
                Err(err) => this.set_error(err.to_string(), cx),
            });
        })
        .detach();
    }

    /// Per-file summaries in render order (drives an external file list).
    pub fn files(&self) -> &[FileSummary] {
        &self.files
    }

    /// Scroll the diff so `file_ix`'s header row is at the top (§7.8 bullet
    /// 4: "selecting a file scrolls the diff").
    pub fn scroll_to_file(&mut self, file_ix: usize, cx: &mut gpui::Context<Self>) {
        if let Some(summary) = self.files.get(file_ix) {
            self.scroll
                .scroll_to_item(summary.row_index, ScrollStrategy::Top);
            cx.notify();
        }
    }

    // -- rendering ----------------------------------------------------------

    fn render_row(&self, ix: usize, cx: &App) -> AnyElement {
        let Some(row) = self.rows.get(ix) else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let mono = theme.mono_font_family.clone();
        // The 50/50 split stays pinned to the pane width; each column's INNER
        // content translates by the shared horizontal offset (synced sides).
        let shift = self.h_scroll.column_shift();
        match row {
            RenderRow::FileGap => div()
                .w_full()
                .h(px(FILE_GAP_H))
                .into_any_element(),
            RenderRow::FileHeader {
                path,
                previous_path,
                status,
                additions,
                deletions,
            } => {
                // Web FilePatch header: muted/30 bar, mono path, +N -N right.
                // Pinned — never scrolls horizontally.
                let mut header = h_flex()
                    .w_full()
                    .h(px(FILE_HEADER_H))
                    .items_center()
                    .gap_2()
                    .px_2()
                    .bg(theme.muted.opacity(0.3))
                    .border_1()
                    .border_color(theme.border)
                    .text_size(px(CODE_TEXT_SIZE + 1.))
                    .font_family(mono);
                if let Some(previous) = previous_path {
                    header = header.child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(theme.muted_foreground)
                            .child(SharedString::from(format!("{previous} →"))),
                    );
                }
                header = header.child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(theme.foreground)
                        .child(path.clone()),
                );
                // Status badge for non-plain-modified files (added/removed/
                // renamed…) — the web shows status only in the no-patch note;
                // the badge also feeds §7.8's per-file status affordance.
                if status.as_ref() != "modified" {
                    header = header.child(
                        div()
                            .px_1()
                            .rounded(theme.radius)
                            .bg(theme.accent.opacity(0.5))
                            .text_color(theme.muted_foreground)
                            .child(status.clone()),
                    );
                }
                header
                    .child(div().flex_1())
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(theme.green.lighten(0.2))
                            .child(SharedString::from(format!("+{additions}"))),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_color(theme.red.lighten(0.2))
                            .child(SharedString::from(format!("-{deletions}"))),
                    )
                    .into_any_element()
            }
            RenderRow::Note { message } => div()
                .w_full()
                .h(px(NOTE_ROW_H))
                .px_2()
                .py_1()
                .text_size(px(CODE_TEXT_SIZE + 1.))
                .text_color(theme.muted_foreground)
                .child(message.clone())
                .into_any_element(),
            RenderRow::HunkHeader { header } => {
                // Web: `text-indigo-300/80 bg-indigo-500/5` → token-locked
                // BLUE tints (§4 tokens; no indigo token exists). Scrolls
                // with the columns (same shift) inside its clip.
                h_flex()
                    .w_full()
                    .h(px(LINE_ROW_H))
                    .items_center()
                    .bg(theme.blue.opacity(0.05))
                    .text_color(theme.blue.lighten(0.4).opacity(0.8))
                    .font_family(mono)
                    .text_size(px(CODE_TEXT_SIZE))
                    .overflow_hidden()
                    .child(
                        div()
                            .ml(shift)
                            .px_2()
                            .whitespace_nowrap()
                            .child(header.clone()),
                    )
                    .into_any_element()
            }
            RenderRow::Line { left, right } => h_flex()
                .w_full()
                .h(px(LINE_ROW_H))
                .child(self.render_cell(left.as_ref(), shift, cx))
                .child(div().w(px(1.)).h_full().flex_shrink_0().bg(theme.border))
                .child(self.render_cell(right.as_ref(), shift, cx))
                .into_any_element(),
        }
    }

    /// One side of a line row: pinned gutter (the anchor's line number) + a
    /// clipped code viewport whose inner content translates by the shared
    /// horizontal offset (`shift` — both sides move in lockstep). `None`
    /// renders the blank filler that keeps both columns aligned.
    fn render_cell(&self, cell: Option<&RenderCell>, shift: Pixels, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let Some(cell) = cell else {
            return div()
                .flex_1()
                .h_full()
                .min_w(px(0.))
                .bg(theme.muted.opacity(0.3))
                .into_any_element();
        };
        // Web tints: added `bg-emerald-500/10`, removed `bg-rose-500/10` —
        // token-locked GREEN/RED (§4: web diff colors from theme tokens).
        let (row_bg, gutter_color) = match cell.kind {
            CellKind::Added => (theme.green.opacity(0.10), theme.green.lighten(0.2)),
            CellKind::Removed => (theme.red.opacity(0.10), theme.red.lighten(0.2)),
            CellKind::Context => (gpui::transparent_black(), theme.muted_foreground),
        };
        let text: SharedString = if cell.text.is_empty() {
            " ".into()
        } else {
            cell.text.clone()
        };
        h_flex()
            .flex_1()
            .h_full()
            .min_w(px(0.))
            .overflow_hidden()
            .bg(row_bg)
            .font_family(theme.mono_font_family.clone())
            .text_size(px(CODE_TEXT_SIZE))
            .child(
                // Gutter — renders `cell.anchor.line`, so the visible number
                // IS the write-back anchor (they cannot drift). Pinned.
                h_flex()
                    .w(px(GUTTER_W))
                    .h_full()
                    .flex_shrink_0()
                    .justify_end()
                    .items_center()
                    .px_1()
                    .text_color(gutter_color.opacity(0.8))
                    .child(SharedString::from(cell.anchor.line.to_string())),
            )
            .child(
                // The clipped viewport: fixed-width inner content shifted by
                // the shared offset.
                div()
                    .flex_1()
                    .h_full()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .child(
                        div()
                            .w(px(self.cell_text_w))
                            .flex_shrink_0()
                            .ml(shift)
                            .whitespace_nowrap()
                            .px_1()
                            .text_color(theme.foreground)
                            .child(
                                StyledText::new(text)
                                    .with_highlights(cell.highlights.iter().cloned()),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl Focusable for DiffView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DiffView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let body: AnyElement = match &self.phase {
            Phase::Loading => div()
                .px_3()
                .py_3()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Loading changes…")
                .into_any_element(),
            Phase::Error(message) => div()
                .px_3()
                .py_3()
                .text_xs()
                .text_color(theme.danger)
                .child(SharedString::from(format!("Couldn’t load changes: {message}")))
                .into_any_element(),
            Phase::Ready if self.rows.is_empty() => div()
                .px_3()
                .py_3()
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("No changed files.")
                .into_any_element(),
            Phase::Ready => {
                let h_scroll = self.h_scroll.clone();
                let wheel_scroll = self.h_scroll.clone();
                let prepaint_scroll = self.h_scroll.clone();
                v_flex()
                    .id("diff-view-list")
                    .relative()
                    .size_full()
                    // Track the pane width so the shared offset clamps right.
                    .on_prepaint(move |bounds, _, _| {
                        prepaint_scroll.set_viewport_width(bounds.size.width);
                    })
                    // Horizontal wheel / shift-wheel drives the shared offset
                    // (vertical deltas fall through to the virtual list).
                    .on_scroll_wheel(cx.listener(move |_, event: &ScrollWheelEvent, window, cx| {
                        let delta = event.delta.pixel_delta(window.line_height());
                        if delta.x != px(0.) {
                            wheel_scroll.scroll_by(delta.x);
                            cx.notify();
                        }
                    }))
                    .child(
                        v_virtual_list(
                            cx.entity().clone(),
                            "diff-rows",
                            self.sizes.clone(),
                            |this, visible_range, _window, cx| {
                                visible_range
                                    .map(|ix| this.render_row(ix, cx))
                                    .collect::<Vec<_>>()
                            },
                        )
                        .track_scroll(&self.scroll)
                        .p_2(),
                    )
                    .scrollbar(&self.scroll, ScrollbarAxis::Vertical)
                    .scrollbar(&h_scroll, ScrollbarAxis::Horizontal)
                    .into_any_element()
            }
        };
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(body)
    }
}

// ---------------------------------------------------------------------------
// Pure row building (gpui-App-free → unit-testable)
// ---------------------------------------------------------------------------

/// Source-agnostic per-file diff the renderer consumes. Both `issues.prFiles`
/// (PR diffs, [`DiffFileModel::from_pull_file`]) and `coding::scm::DiffFile`
/// (working / commit diffs, [`DiffFileModel::from_scm`]) normalize onto this
/// so the two sources share ONE renderer (§4.4). Hunks are already parsed —
/// the PR path parses the GitHub `patch` string here, the SCM path maps its
/// pre-parsed [`UnifiedHunk`]s — so [`build_rows_from_models`] never touches a
/// raw patch again and the render/highlight/virtualize pipeline is identical.
struct DiffFileModel {
    filename: String,
    previous_filename: Option<String>,
    /// GitHub-style status string (`added` / `modified` / `renamed` /
    /// `removed`) — drives the header badge and the empty-hunks note.
    status: String,
    additions: u32,
    deletions: u32,
    /// Empty ⇒ render the "Renamed." / "No textual diff" note (binary,
    /// too-large, or a pure rename).
    hunks: Vec<patch::Hunk>,
}

impl DiffFileModel {
    /// From an `issues.prFiles` [`PullFile`] — the PR-diff path. Parses the
    /// GitHub `patch` body (hunks only) exactly as before.
    fn from_pull_file(file: &PullFile) -> Self {
        Self {
            filename: file.filename.clone(),
            previous_filename: file.previous_filename.clone(),
            status: file.status.clone(),
            additions: file.additions,
            deletions: file.deletions,
            hunks: file
                .patch
                .as_deref()
                .map(patch::parse_patch)
                .unwrap_or_default(),
        }
    }

    /// From a [`coding::scm::DiffFile`] — the Source Control working/commit
    /// diff path (§4.4). Hunks are already parsed by `scm::parse_unified_diff`;
    /// binary files carry none, so they fall through to the note.
    fn from_scm(file: &DiffFile) -> Self {
        Self {
            filename: file.path.clone(),
            previous_filename: file.previous_path.clone(),
            status: scm_status_str(file.status).to_string(),
            additions: file.additions,
            deletions: file.deletions,
            hunks: if file.binary {
                Vec::new()
            } else {
                file.hunks.iter().map(hunk_from_unified).collect()
            },
        }
    }
}

/// Map an scm [`FileStatus`] to the GitHub-style status string the renderer's
/// header badge + note logic already speak (mirrors the `PullFile.status`
/// vocabulary). Untracked collapses to `added` (a wholly-new file).
///
/// [`FileStatus`]: coding::scm::FileStatus
fn scm_status_str(status: coding::scm::FileStatus) -> &'static str {
    use coding::scm::FileStatus;
    match status {
        FileStatus::Modified => "modified",
        FileStatus::Added | FileStatus::Untracked => "added",
        FileStatus::Deleted => "removed",
        FileStatus::Renamed => "renamed",
    }
}

/// Map an scm [`UnifiedHunk`] onto the renderer's [`patch::Hunk`]. The two
/// carry the same information under different field names — the scm side is
/// pre-parsed, so no `@@` re-parse happens.
fn hunk_from_unified(hunk: &UnifiedHunk) -> patch::Hunk {
    patch::Hunk {
        old_start: hunk.old_start,
        old_count: hunk.old_lines,
        new_start: hunk.new_start,
        new_count: hunk.new_lines,
        header: hunk.header.clone(),
        rows: hunk.lines.iter().map(row_from_diff_line).collect(),
    }
}

/// Map one scm [`DiffLine`] onto the renderer's [`patch::DiffRow`]. `unwrap_or`
/// guards a malformed producer (the side's line number is always present for
/// its own kind) rather than panicking.
fn row_from_diff_line(line: &DiffLine) -> patch::DiffRow {
    match line.kind {
        DiffLineKind::Context => patch::DiffRow::Context {
            old_ln: line.old_line.unwrap_or(0),
            new_ln: line.new_line.unwrap_or(0),
            text: line.content.clone(),
        },
        DiffLineKind::Addition => patch::DiffRow::Added {
            new_ln: line.new_line.unwrap_or(0),
            text: line.content.clone(),
        },
        DiffLineKind::Deletion => patch::DiffRow::Removed {
            old_ln: line.old_line.unwrap_or(0),
            text: line.content.clone(),
        },
    }
}

/// Build a [`PreparedDiff`] from `issues.prFiles` off the foreground (the PR
/// path). Pure — needs only a [`HighlightTheme`], no gpui App/Window — so
/// callers run it on the background executor, then install the result with
/// [`DiffView::set_prepared`].
pub fn build_pr_diff(files: &[PullFile], theme: &HighlightTheme) -> PreparedDiff {
    let (rows, summaries) = build_rows(files, theme);
    PreparedDiff { rows, summaries }
}

/// Build a [`PreparedDiff`] from SCM working/commit diffs off the foreground
/// (§4.4). Same background-build contract as [`build_pr_diff`]; install with
/// [`DiffView::set_prepared`].
pub fn build_scm_diff(files: &[DiffFile], theme: &HighlightTheme) -> PreparedDiff {
    let (rows, summaries) = build_rows_from_scm(files, theme);
    PreparedDiff { rows, summaries }
}

/// Build the flat render-row list + per-file summaries from PR files (the
/// `issues.prFiles` path). Pure — needs only a [`HighlightTheme`], no gpui
/// App/Window. Normalizes to [`DiffFileModel`] then defers to the shared core.
fn build_rows(files: &[PullFile], theme: &HighlightTheme) -> (Vec<RenderRow>, Vec<FileSummary>) {
    let models: Vec<DiffFileModel> = files.iter().map(DiffFileModel::from_pull_file).collect();
    build_rows_from_models(&models, theme)
}

/// Build the same render-row list from SCM working/commit diffs (§4.4). Shares
/// the core with [`build_rows`] via [`DiffFileModel`].
fn build_rows_from_scm(
    files: &[DiffFile],
    theme: &HighlightTheme,
) -> (Vec<RenderRow>, Vec<FileSummary>) {
    let models: Vec<DiffFileModel> = files.iter().map(DiffFileModel::from_scm).collect();
    build_rows_from_models(&models, theme)
}

/// The source-agnostic renderer core: normalized [`DiffFileModel`]s →
/// virtualized render rows + per-file summaries. This is the single place the
/// header, note, Tree-sitter highlight, and side-by-side alignment live, so PR
/// and SCM diffs render pixel-identically.
fn build_rows_from_models(
    files: &[DiffFileModel],
    theme: &HighlightTheme,
) -> (Vec<RenderRow>, Vec<FileSummary>) {
    let mut rows: Vec<RenderRow> = Vec::new();
    let mut summaries: Vec<FileSummary> = Vec::new();

    for (file_ix, file) in files.iter().enumerate() {
        if file_ix > 0 {
            rows.push(RenderRow::FileGap);
        }
        summaries.push(FileSummary {
            filename: file.filename.clone().into(),
            status: file.status.clone().into(),
            additions: file.additions,
            deletions: file.deletions,
            row_index: rows.len(),
        });
        rows.push(RenderRow::FileHeader {
            path: file.filename.clone().into(),
            previous_path: file.previous_filename.clone().map(SharedString::from),
            status: file.status.clone().into(),
            additions: file.additions,
            deletions: file.deletions,
        });

        let hunks = &file.hunks;
        if hunks.is_empty() {
            // Web parity: renamed-without-changes vs binary/too-large.
            let message = if file.status == "renamed" {
                "Renamed."
            } else {
                "No textual diff (binary or too large)."
            };
            rows.push(RenderRow::Note {
                message: message.into(),
            });
            continue;
        }

        let aligned = patch::align_file(&file.filename, hunks);

        // Each side's fragment document, in row order (context lines belong
        // to BOTH sides), for one Tree-sitter pass per side.
        let mut old_lines: Vec<&str> = Vec::new();
        let mut new_lines: Vec<&str> = Vec::new();
        for row in &aligned {
            if let patch::DisplayRow::Line { left, right } = row {
                if let Some(cell) = left {
                    old_lines.push(&cell.text);
                }
                if let Some(cell) = right {
                    new_lines.push(&cell.text);
                }
            }
        }
        let lang = highlight::language_for_filename(&file.filename);
        let old_hl = highlight::highlight_lines(lang, &old_lines, theme);
        let new_hl = highlight::highlight_lines(lang, &new_lines, theme);

        let (mut old_ix, mut new_ix) = (0usize, 0usize);
        for row in aligned {
            match row {
                patch::DisplayRow::HunkHeader { header } => {
                    rows.push(RenderRow::HunkHeader {
                        header: header.into(),
                    });
                }
                patch::DisplayRow::Line { left, right } => {
                    let left = left.map(|cell| {
                        let highlights = old_hl.get(old_ix).cloned().unwrap_or_default();
                        old_ix += 1;
                        render_cell_from(cell, highlights)
                    });
                    let right = right.map(|cell| {
                        let highlights = new_hl.get(new_ix).cloned().unwrap_or_default();
                        new_ix += 1;
                        render_cell_from(cell, highlights)
                    });
                    rows.push(RenderRow::Line { left, right });
                }
            }
        }
    }

    (rows, summaries)
}

/// The inner content width one COLUMN needs for its longest line (hunk
/// headers scroll inside the same clip, so they count too). Character widths
/// are estimated (`CHAR_W`) — generous enough that nothing clips, exact
/// enough that the scroll range stays sane.
fn required_cell_text_width(rows: &[RenderRow]) -> f32 {
    let mut max_chars = 0usize;
    for row in rows {
        match row {
            RenderRow::Line { left, right } => {
                for cell in [left, right].into_iter().flatten() {
                    max_chars = max_chars.max(cell.text.chars().count());
                }
            }
            RenderRow::HunkHeader { header } => {
                max_chars = max_chars.max(header.chars().count());
            }
            _ => {}
        }
    }
    max_chars as f32 * CHAR_W + 16.
}

fn render_cell_from(
    cell: patch::Cell,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
) -> RenderCell {
    RenderCell {
        kind: cell.kind,
        anchor: cell.anchor,
        text: cell.text.into(),
        highlights,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS_PATCH: &str = include_str!("diff/fixtures/github-pr-ts.patch");
    const RS_PATCH: &str = include_str!("diff/fixtures/theme-lib-rs.patch");

    fn ts_file() -> PullFile {
        PullFile {
            filename: "apps/web/src/lib/integrations/github-pr.ts".into(),
            status: "modified".into(),
            additions: 4,
            deletions: 3,
            patch: Some(TS_PATCH.to_string()),
            sha: None,
            previous_filename: None,
        }
    }

    fn rs_file() -> PullFile {
        PullFile {
            filename: "apps/desktop/crates/theme/src/lib.rs".into(),
            status: "modified".into(),
            additions: 4,
            deletions: 1,
            patch: Some(RS_PATCH.to_string()),
            sha: None,
            previous_filename: None,
        }
    }

    fn binary_file() -> PullFile {
        PullFile {
            filename: "assets/logo.png".into(),
            status: "added".into(),
            additions: 0,
            deletions: 0,
            patch: None,
            sha: None,
            previous_filename: None,
        }
    }

    #[test]
    fn builds_header_hunks_and_aligned_lines_from_real_patch() {
        let theme = HighlightTheme::default_dark();
        let (rows, summaries) = build_rows(&[ts_file()], &theme);

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].row_index, 0);
        assert!(matches!(&rows[0], RenderRow::FileHeader { path, .. }
            if path.as_ref() == "apps/web/src/lib/integrations/github-pr.ts"));
        let hunk_headers = rows
            .iter()
            .filter(|r| matches!(r, RenderRow::HunkHeader { .. }))
            .count();
        assert_eq!(hunk_headers, 2, "TS fixture has two hunks");
        // Every line row has at least one populated side, and every populated
        // cell carries its anchor (§7.8 acceptance).
        let mut line_rows = 0;
        for row in &rows {
            if let RenderRow::Line { left, right } = row {
                line_rows += 1;
                assert!(left.is_some() || right.is_some());
                for (cell, side) in [(left, Side::Old), (right, Side::New)] {
                    if let Some(cell) = cell {
                        assert_eq!(cell.anchor.side, side);
                        assert_eq!(
                            cell.anchor.filename,
                            "apps/web/src/lib/integrations/github-pr.ts"
                        );
                    }
                }
            }
        }
        // Hunk 0: max(8,9) = 9 aligned rows; hunk 1: max(7,7) = 7.
        assert_eq!(line_rows, 16);
    }

    #[test]
    fn sizes_match_rows_one_to_one() {
        let theme = HighlightTheme::default_dark();
        let files = [ts_file(), binary_file(), rs_file()];
        let (rows, _) = build_rows(&files, &theme);
        let sizes: Vec<_> = rows.iter().map(|r| r.height()).collect();
        assert_eq!(sizes.len(), rows.len());
        // Spot-check the row kinds drive the heights.
        assert_eq!(rows[0].height(), px(FILE_HEADER_H));
        assert!(sizes.contains(&px(FILE_GAP_H)));
    }

    #[test]
    fn binary_file_renders_note_not_lines() {
        let theme = HighlightTheme::default_dark();
        let (rows, summaries) = build_rows(&[binary_file()], &theme);
        assert_eq!(rows.len(), 2);
        assert!(matches!(&rows[1], RenderRow::Note { message }
            if message.as_ref() == "No textual diff (binary or too large)."));
        assert_eq!(summaries[0].status.as_ref(), "added");
    }

    #[test]
    fn renamed_file_without_patch_says_renamed() {
        let theme = HighlightTheme::default_dark();
        let file = PullFile {
            filename: "src/new-name.ts".into(),
            status: "renamed".into(),
            additions: 0,
            deletions: 0,
            patch: None,
            sha: None,
            previous_filename: Some("src/old-name.ts".into()),
        };
        let (rows, _) = build_rows(&[file], &theme);
        assert!(matches!(&rows[0], RenderRow::FileHeader { previous_path: Some(p), .. }
            if p.as_ref() == "src/old-name.ts"));
        assert!(matches!(&rows[1], RenderRow::Note { message } if message.as_ref() == "Renamed."));
    }

    #[test]
    fn multi_file_summaries_point_at_their_header_rows() {
        let theme = HighlightTheme::default_dark();
        let files = [ts_file(), rs_file(), binary_file()];
        let (rows, summaries) = build_rows(&files, &theme);
        assert_eq!(summaries.len(), 3);
        for summary in &summaries {
            assert!(matches!(&rows[summary.row_index], RenderRow::FileHeader { path, .. }
                if path == &summary.filename));
        }
        // Gaps separate files: exactly files-1 of them.
        let gaps = rows
            .iter()
            .filter(|r| matches!(r, RenderRow::FileGap))
            .count();
        assert_eq!(gaps, 2);
    }

    #[test]
    fn syntax_highlight_spans_attach_to_the_right_cells() {
        // The RS fixture adds `pub mod terminal;` — a keyword-bearing added
        // line on the NEW side. With the rust grammar enabled its cell must
        // carry at least one non-default highlight run.
        let theme = HighlightTheme::default_dark();
        let (rows, _) = build_rows(&[rs_file()], &theme);
        let added_mod_line = rows.iter().find_map(|row| match row {
            RenderRow::Line { right: Some(cell), .. }
                if cell.kind == CellKind::Added && cell.text.as_ref() == "pub mod terminal;" =>
            {
                Some(cell)
            }
            _ => None,
        });
        let cell = added_mod_line.expect("the added `pub mod terminal;` line is present");
        assert_eq!(cell.anchor.line, 71);
        assert!(
            !cell.highlights.is_empty(),
            "rust keywords on an added line must be highlighted"
        );
        // All runs stay inside the line text.
        for (range, _) in &cell.highlights {
            assert!(range.end <= cell.text.len());
        }
    }

    // -- SCM adapter (R2.d): coding::scm::DiffFile → same renderer -----------

    use coding::scm::{DiffFile, DiffLine, DiffLineKind, FileStatus, UnifiedHunk};

    /// Re-encode a parsed patch as the scm model, so the PR fixture and the SCM
    /// path exercise the exact same content through [`build_rows_from_scm`].
    fn scm_file_from_patch(path: &str, status: FileStatus, patch: &str) -> DiffFile {
        let hunks: Vec<UnifiedHunk> = patch::parse_patch(patch)
            .into_iter()
            .map(|h| UnifiedHunk {
                old_start: h.old_start,
                old_lines: h.old_count,
                new_start: h.new_start,
                new_lines: h.new_count,
                header: h.header,
                lines: h
                    .rows
                    .into_iter()
                    .map(|row| match row {
                        patch::DiffRow::Context { old_ln, new_ln, text } => DiffLine {
                            kind: DiffLineKind::Context,
                            old_line: Some(old_ln),
                            new_line: Some(new_ln),
                            content: text,
                        },
                        patch::DiffRow::Removed { old_ln, text } => DiffLine {
                            kind: DiffLineKind::Deletion,
                            old_line: Some(old_ln),
                            new_line: None,
                            content: text,
                        },
                        patch::DiffRow::Added { new_ln, text } => DiffLine {
                            kind: DiffLineKind::Addition,
                            old_line: None,
                            new_line: Some(new_ln),
                            content: text,
                        },
                    })
                    .collect(),
            })
            .collect();
        let additions = patch::parse_patch(patch)
            .iter()
            .flat_map(|h| &h.rows)
            .filter(|r| matches!(r, patch::DiffRow::Added { .. }))
            .count() as u32;
        let deletions = patch::parse_patch(patch)
            .iter()
            .flat_map(|h| &h.rows)
            .filter(|r| matches!(r, patch::DiffRow::Removed { .. }))
            .count() as u32;
        DiffFile {
            path: path.to_string(),
            previous_path: None,
            status,
            additions,
            deletions,
            hunks,
            binary: false,
        }
    }

    #[test]
    fn scm_diff_file_renders_headers_hunks_and_anchored_lines() {
        // Same TS fixture as the PR path, routed through the scm model: header,
        // two hunks, 16 aligned line rows, anchors on every populated cell.
        let theme = HighlightTheme::default_dark();
        let file = scm_file_from_patch(
            "apps/web/src/lib/integrations/github-pr.ts",
            FileStatus::Modified,
            TS_PATCH,
        );
        let (rows, summaries) = build_rows_from_scm(&[file], &theme);

        assert_eq!(summaries.len(), 1);
        assert!(matches!(&rows[0], RenderRow::FileHeader { path, .. }
            if path.as_ref() == "apps/web/src/lib/integrations/github-pr.ts"));
        let hunk_headers = rows
            .iter()
            .filter(|r| matches!(r, RenderRow::HunkHeader { .. }))
            .count();
        assert_eq!(hunk_headers, 2);
        let mut line_rows = 0;
        for row in &rows {
            if let RenderRow::Line { left, right } = row {
                line_rows += 1;
                for (cell, side) in [(left, Side::Old), (right, Side::New)] {
                    if let Some(cell) = cell {
                        assert_eq!(cell.anchor.side, side);
                        assert_eq!(
                            cell.anchor.filename,
                            "apps/web/src/lib/integrations/github-pr.ts"
                        );
                    }
                }
            }
        }
        assert_eq!(line_rows, 16, "scm path aligns identically to the PR path");
    }

    #[test]
    fn scm_diff_file_syntax_highlights_like_the_pr_path() {
        // The rust fixture's added `pub mod terminal;` must still carry a
        // Tree-sitter highlight run when it arrives via the scm model.
        let theme = HighlightTheme::default_dark();
        let file = scm_file_from_patch(
            "apps/desktop/crates/theme/src/lib.rs",
            FileStatus::Modified,
            RS_PATCH,
        );
        let (rows, _) = build_rows_from_scm(&[file], &theme);
        let cell = rows
            .iter()
            .find_map(|row| match row {
                RenderRow::Line { right: Some(cell), .. }
                    if cell.kind == CellKind::Added
                        && cell.text.as_ref() == "pub mod terminal;" =>
                {
                    Some(cell)
                }
                _ => None,
            })
            .expect("added `pub mod terminal;` present via scm path");
        assert_eq!(cell.anchor.line, 71);
        assert!(!cell.highlights.is_empty());
    }

    #[test]
    fn scm_binary_file_renders_note() {
        let theme = HighlightTheme::default_dark();
        let file = DiffFile {
            path: "assets/logo.png".into(),
            previous_path: None,
            status: FileStatus::Added,
            additions: 0,
            deletions: 0,
            hunks: Vec::new(),
            binary: true,
        };
        let (rows, summaries) = build_rows_from_scm(&[file], &theme);
        assert_eq!(rows.len(), 2);
        assert!(matches!(&rows[1], RenderRow::Note { message }
            if message.as_ref() == "No textual diff (binary or too large)."));
        // Added maps to the GitHub "added" status string.
        assert_eq!(summaries[0].status.as_ref(), "added");
    }

    #[test]
    fn scm_renamed_without_hunks_says_renamed() {
        let theme = HighlightTheme::default_dark();
        let file = DiffFile {
            path: "src/new-name.rs".into(),
            previous_path: Some("src/old-name.rs".into()),
            status: FileStatus::Renamed,
            additions: 0,
            deletions: 0,
            hunks: Vec::new(),
            binary: false,
        };
        let (rows, _) = build_rows_from_scm(&[file], &theme);
        assert!(matches!(&rows[0], RenderRow::FileHeader { previous_path: Some(p), status, .. }
            if p.as_ref() == "src/old-name.rs" && status.as_ref() == "renamed"));
        assert!(matches!(&rows[1], RenderRow::Note { message }
            if message.as_ref() == "Renamed."));
    }

    #[test]
    fn scm_status_strings_match_the_github_vocabulary() {
        assert_eq!(scm_status_str(FileStatus::Modified), "modified");
        assert_eq!(scm_status_str(FileStatus::Added), "added");
        assert_eq!(scm_status_str(FileStatus::Untracked), "added");
        assert_eq!(scm_status_str(FileStatus::Deleted), "removed");
        assert_eq!(scm_status_str(FileStatus::Renamed), "renamed");
    }

    #[test]
    fn unequal_change_block_gets_filler_on_the_short_side() {
        let theme = HighlightTheme::default_dark();
        let (rows, _) = build_rows(&[ts_file()], &theme);
        // TS hunk 0 block: 2 removed vs 3 added → exactly one right-only row
        // before the trailing context resumes.
        let filler_rows = rows
            .iter()
            .filter(|r| matches!(r, RenderRow::Line { left: None, right: Some(_) }))
            .count();
        assert_eq!(filler_rows, 1);
        assert!(!rows
            .iter()
            .any(|r| matches!(r, RenderRow::Line { left: Some(_), right: None })));
    }
}
