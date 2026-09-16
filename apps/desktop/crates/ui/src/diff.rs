//! EXP-895 — the ONE diff view, UNIFIED, on every desktop surface.
//!
//! Side-by-side is gone. A diff is one column of per-file CARDS: a sticky
//! `letter · dimmed-dir/basename · +N −M · chevron` header over rows of
//! `old-gutter · new-gutter · sign · text`, with `N unchanged lines` dividers
//! where the patch skipped context. That is the same anatomy the web
//! `@exp/ui` `FileDiffCard` renders and the natives mirror, so a review reads
//! identically on all four clients.
//!
//! Everything about a diff's MODEL lives in [`domain::diff`] (hand-mirrored ×4,
//! byte-locked by `packages/domain-contract/fixtures/diff/cases.json`): the
//! parse, the counts, the `N unchanged lines` arithmetic and the `+N`/`−N`
//! labels. This module only turns a `DiffFile` into rows and paints them.
//!
//! Three producers, one renderer:
//!   * `issues.prFiles` / `repositories.branchDiff` [`PullFile`]s
//!     ([`files_from_pull`]),
//!   * the desktop worktree's own `git diff` (`coding::scm`),
//!   * a steer wire patch (the transcript's edit card, which renders its rows
//!     through the very same [`render_diff_row`] at [`DiffOptions::card`]).
//!
//! Rows are fixed-height so the virtual list can pre-size, and a COLLAPSIBLE
//! diff renders through a pure projection ([`project_rows`]) rather than a
//! second row list — a collapsed file is exactly its header row, which is
//! exactly one `glass_row_card`.

mod highlight;

pub use highlight::language_for_filename;

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    div, point, px, size, AnyElement, App, Bounds, FocusHandle, Focusable, HighlightStyle,
    InteractiveElement as _, IntoElement, ParentElement, Pixels, Point, Render, ScrollStrategy,
    ScrollWheelEvent, SharedString, Size, Styled, StyledText, Window,
};
use gpui_component::{
    h_flex,
    scroll::{ScrollableElement as _, ScrollbarAxis, ScrollbarHandle},
    v_flex, v_virtual_list, ActiveTheme as _, ElementExt as _, Sizable as _,
    VirtualListScrollHandle,
};

use api::issues::PullFile;
use coding::scm::DiffFile;
use domain::diff::{
    additions_label, deletions_label, unchanged_before, unchanged_between, unchanged_label,
    DiffLineKind, DiffStatus,
};
use gpui_component::highlighter::HighlightTheme;

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Web renders the patch at `text-[0.6875rem]` (11px) mono — mirrored here.
const CODE_TEXT_SIZE: f32 = 11.0;
const LINE_ROW_H: f32 = 18.0;
const LINE_ROW_H_COMPACT: f32 = 15.0;
/// The per-file header is a glass row CARD (a clickable disclosure), so it
/// needs a control's height.
const FILE_HEADER_H: f32 = 30.0;
const FILE_HEADER_H_COMPACT: f32 = 24.0;
const NOTE_ROW_H: f32 = 22.0;
const FILE_GAP_H: f32 = 8.0;
/// Line-number gutter width — 4 digits + padding at 11px mono.
const GUTTER_W: f32 = 36.0;
/// The `+`/`−`/`` sign column between the gutters and the text.
const SIGN_W: f32 = 14.0;
/// Estimated mono advance at 11px — sizes rows for horizontal scrolling
/// (slightly generous so the longest line never clips at the right edge).
const CHAR_W: f32 = 6.8;

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

/// How one surface wants its diff painted. Four constructors, one per
/// surface — a caller never assembles the flags itself, so "the review look"
/// cannot drift from "the pane look" by a stray boolean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffOptions {
    /// Per-file disclosure: a collapsed file is its header row alone.
    pub collapsible: bool,
    /// Whether a freshly installed diff opens with every file unfolded.
    pub start_expanded: bool,
    /// Tighter rows, one gutter, no syntax highlighting — the transcript's
    /// inline edit card, which is a PREVIEW inside a paragraph of text.
    pub compact: bool,
    /// Run the Tree-sitter pass. Off on `compact`, and off wherever the
    /// caller would pay for a highlight nobody reads.
    pub highlight: bool,
}

impl DiffOptions {
    /// The Reviews screen: a stack of per-file cards, OPEN — a review is
    /// read top to bottom, and the file list beside it is how a big PR is
    /// navigated. Each card still folds on its own header.
    pub fn review() -> Self {
        Self {
            collapsible: true,
            start_expanded: true,
            compact: false,
            highlight: true,
        }
    }

    /// The run's / issue's Changes face: the same cards, OPEN — a run's diff
    /// is small and you came to read it.
    pub fn pane() -> Self {
        Self {
            collapsible: true,
            start_expanded: true,
            compact: false,
            highlight: true,
        }
    }

    /// The IDE's Source Control screen: one flat, always-expanded list (no
    /// disclosure — the file list beside it is the navigation).
    pub fn source_control() -> Self {
        Self {
            collapsible: false,
            start_expanded: true,
            compact: false,
            highlight: true,
        }
    }

    /// The transcript's inline edit card: compact rows, no highlight, no
    /// disclosure of its own (the tool row's Show more owns that).
    pub fn card() -> Self {
        Self {
            collapsible: false,
            start_expanded: false,
            compact: true,
            highlight: false,
        }
    }

    fn line_h(&self) -> f32 {
        if self.compact {
            LINE_ROW_H_COMPACT
        } else {
            LINE_ROW_H
        }
    }

    fn header_h(&self) -> f32 {
        if self.compact {
            FILE_HEADER_H_COMPACT
        } else {
            FILE_HEADER_H
        }
    }

    fn text_size(&self) -> f32 {
        if self.compact {
            CODE_TEXT_SIZE - 1.
        } else {
            CODE_TEXT_SIZE
        }
    }
}

impl Default for DiffOptions {
    fn default() -> Self {
        Self::review()
    }
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

/// One row of the flat, virtualizable render list. Everything a row needs to
/// paint is ON it — [`render_diff_row`] is a free function over `&App`, so the
/// transcript's edit card paints the exact same rows without a `DiffView`.
pub(crate) enum RenderRow {
    /// The file's card header: status letter, dimmed dir + basename, counts,
    /// chevron.
    FileHeader {
        path: SharedString,
        /// The basename, at full weight.
        name: SharedString,
        /// The directory with its trailing slash, dimmed (`apps/web/src/`).
        dir: SharedString,
        previous_path: Option<SharedString>,
        status: DiffStatus,
        additions: u32,
        deletions: u32,
        binary: bool,
    },
    /// What a file with no hunks says instead of rows (web `noHunksNote`),
    /// and the transcript card's trailing "… N more lines".
    Note { message: SharedString },
    /// The verbatim `@@ … @@` line.
    HunkHeader { header: SharedString },
    /// `N unchanged lines` — context the patch never carried. A PLAIN row,
    /// never a button: there is nothing on the wire to expand to.
    Unchanged { lines: u32 },
    /// One unified line: both gutters, the sign, the text.
    Line {
        kind: DiffLineKind,
        old_no: Option<u32>,
        new_no: Option<u32>,
        text: SharedString,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    },
    /// Spacer between two files' cards.
    FileGap,
}

impl RenderRow {
    fn height(&self, options: &DiffOptions) -> Pixels {
        match self {
            RenderRow::FileHeader { .. } => px(options.header_h()),
            RenderRow::Note { .. } => px(NOTE_ROW_H),
            RenderRow::HunkHeader { .. } | RenderRow::Unchanged { .. } | RenderRow::Line { .. } => {
                px(options.line_h())
            }
            RenderRow::FileGap => px(FILE_GAP_H),
        }
    }
}

// ---------------------------------------------------------------------------
// Card chrome
// ---------------------------------------------------------------------------

/// Where a row sits in its file's card. The card is drawn PER ROW (a
/// virtualized list has no single element to put a border on), so each row
/// paints the slice of the frame it owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowShape {
    /// The whole card in one row — a COLLAPSED file. Identical chrome to
    /// [`crate::surface::glass_row_card`].
    Only,
    /// An expanded file's header: rounded top, open bottom.
    Top,
    Middle,
    /// The file's last row: rounded bottom.
    Bottom,
    /// The spacer between cards — no chrome at all.
    Gap,
}

/// The chrome a shape paints, as data — pure, so "a collapsed file is one
/// rounded card" is a unit test rather than a screenshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RowChrome {
    pub(crate) fill: bool,
    pub(crate) border_top: bool,
    pub(crate) border_bottom: bool,
    pub(crate) border_sides: bool,
    pub(crate) round_top: bool,
    pub(crate) round_bottom: bool,
}

impl RowShape {
    pub(crate) fn chrome(self) -> RowChrome {
        match self {
            RowShape::Only => RowChrome {
                fill: true,
                border_top: true,
                border_bottom: true,
                border_sides: true,
                round_top: true,
                round_bottom: true,
            },
            RowShape::Top => RowChrome {
                fill: true,
                border_top: true,
                border_bottom: false,
                border_sides: true,
                round_top: true,
                round_bottom: false,
            },
            RowShape::Middle => RowChrome {
                fill: true,
                border_top: false,
                border_bottom: false,
                border_sides: true,
                round_top: false,
                round_bottom: false,
            },
            RowShape::Bottom => RowChrome {
                fill: true,
                border_top: false,
                border_bottom: true,
                border_sides: true,
                round_top: false,
                round_bottom: true,
            },
            RowShape::Gap => RowChrome {
                fill: false,
                border_top: false,
                border_bottom: false,
                border_sides: false,
                round_top: false,
                round_bottom: false,
            },
        }
    }
}

/// The per-row card frame: [`theme::tokens::glass`]'s row fill + row stroke at
/// [`theme::tokens::radius::MD`] — the same three tokens
/// [`crate::surface::glass_row_card`] uses, sliced per row.
pub(crate) fn card_row(shape: RowShape) -> gpui::Div {
    let chrome = shape.chrome();
    let stroke = theme::tokens::glass::STROKE_ROW.to_hsla();
    let radius = px(theme::tokens::radius::MD);
    let mut row = div().w_full().min_w_0().overflow_hidden();
    if chrome.fill {
        row = row.bg(theme::tokens::glass::FILL_ROW.to_hsla());
    }
    if chrome.border_sides {
        row = row.border_l_1().border_r_1();
    }
    if chrome.border_top {
        row = row.border_t_1();
    }
    if chrome.border_bottom {
        row = row.border_b_1();
    }
    if chrome.border_sides || chrome.border_top || chrome.border_bottom {
        row = row.border_color(stroke);
    }
    if chrome.round_top {
        row = row.rounded_tl(radius).rounded_tr(radius);
    }
    if chrome.round_bottom {
        row = row.rounded_bl(radius).rounded_br(radius);
    }
    row
}

/// GitHub's one-letter vocabulary over the contract's statuses.
pub(crate) fn status_letter(status: DiffStatus) -> &'static str {
    match status {
        DiffStatus::Added => "A",
        DiffStatus::Removed => "D",
        DiffStatus::Modified => "M",
        DiffStatus::Renamed => "R",
        DiffStatus::Copied => "C",
    }
}

/// Its colour. Only the two states that ARE a colour in the diff body carry
/// one — `M`/`R`/`C` stay muted, because an amber "modified" and a sky
/// "renamed" invent two accent hues the token set does not have.
pub(crate) fn status_color(status: DiffStatus, cx: &App) -> gpui::Hsla {
    match status {
        DiffStatus::Added => theme::tokens::diff::ADD_FG.to_hsla(),
        DiffStatus::Removed => theme::tokens::diff::DEL_FG.to_hsla(),
        _ => cx.theme().muted_foreground,
    }
}

/// The dimmed directory (with its trailing slash) and the basename of a path.
pub(crate) fn split_path(path: &str) -> (String, String) {
    match path.rfind('/') {
        Some(slash) => (path[..=slash].to_string(), path[slash + 1..].to_string()),
        None => (String::new(), path.to_string()),
    }
}

/// What a file with NO hunks says instead of rows — web `noHunksNote`, word
/// for word, so the four clients explain a hunkless file the same way.
pub(crate) fn no_hunks_note(file: &DiffFile) -> &'static str {
    if file.binary {
        return "Binary file";
    }
    match file.status {
        DiffStatus::Added => "Empty file added",
        DiffStatus::Removed => "File removed",
        DiffStatus::Renamed => "Renamed without content changes",
        DiffStatus::Copied => "Copied without content changes",
        DiffStatus::Modified => "No textual diff (binary or too large)",
    }
}

// ---------------------------------------------------------------------------
// Row building (pure — no gpui App/Window)
// ---------------------------------------------------------------------------

/// Per-file summary — feeds an external file list and `scroll_to_file`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSummary {
    pub filename: SharedString,
    pub status: DiffStatus,
    pub additions: u32,
    pub deletions: u32,
    /// Index of the file's header row in the flat row list (scroll target).
    pub row_index: usize,
    /// The file's FULL span: its header, every body row, and the trailing
    /// [`RenderRow::FileGap`] that separates it from the next file. Equals
    /// `row_index..next_file.row_index`, and `row_index..rows.len()` for the
    /// last file, so the ranges TILE the list.
    pub row_range: Range<usize>,
}

/// ONE file's rows, highlighted in a single Tree-sitter pass over the unified
/// line sequence (Meta lines excluded — `\ No newline at end of file` is not
/// code). Shared with the transcript's edit card, which renders these rows
/// through [`render_diff_row`] without ever building a [`DiffView`].
pub(crate) fn file_rows(
    file: &DiffFile,
    theme: &HighlightTheme,
    options: &DiffOptions,
) -> Vec<RenderRow> {
    let (dir, name) = split_path(&file.path);
    let mut rows = vec![RenderRow::FileHeader {
        path: file.path.clone().into(),
        name: name.into(),
        dir: dir.into(),
        previous_path: file
            .previous_path
            .clone()
            .filter(|previous| !previous.is_empty())
            .map(SharedString::from),
        status: file.status,
        additions: file.additions,
        deletions: file.deletions,
        binary: file.binary,
    }];

    if file.hunks.is_empty() {
        rows.push(RenderRow::Note {
            message: no_hunks_note(file).into(),
        });
        return rows;
    }

    // ONE highlight pass per file, over the unified sequence in ROW order, so
    // an added line's tokens are resolved in the company of the context around
    // it (the old side-by-side split parsed each side separately and lost
    // exactly that).
    let code: Vec<&str> = file
        .hunks
        .iter()
        .flat_map(|hunk| hunk.lines.iter())
        .filter(|line| line.kind != DiffLineKind::Meta)
        .map(|line| line.text.as_str())
        .collect();
    let highlights: Vec<Vec<(Range<usize>, HighlightStyle)>> = if options.highlight {
        highlight::highlight_lines(language_for_filename(&file.path), &code, theme)
    } else {
        Vec::new()
    };

    let mut code_ix = 0usize;
    for (index, hunk) in file.hunks.iter().enumerate() {
        let skipped = if index == 0 {
            unchanged_before(hunk)
        } else {
            unchanged_between(&file.hunks[index - 1], hunk)
        };
        if skipped > 0 {
            rows.push(RenderRow::Unchanged { lines: skipped });
        }
        rows.push(RenderRow::HunkHeader {
            header: hunk.header.clone().into(),
        });
        for line in &hunk.lines {
            let runs = if line.kind == DiffLineKind::Meta {
                Vec::new()
            } else {
                let runs = highlights.get(code_ix).cloned().unwrap_or_default();
                code_ix += 1;
                runs
            };
            rows.push(RenderRow::Line {
                kind: line.kind,
                old_no: line.old_no,
                new_no: line.new_no,
                text: line.text.clone().into(),
                highlights: runs,
            });
        }
    }
    rows
}

/// The flat row list + the per-file summaries that tile it.
fn build_rows(
    files: &[DiffFile],
    theme: &HighlightTheme,
    options: &DiffOptions,
) -> (Vec<RenderRow>, Vec<FileSummary>) {
    let mut rows: Vec<RenderRow> = Vec::new();
    let mut summaries: Vec<FileSummary> = Vec::with_capacity(files.len());
    for (file_ix, file) in files.iter().enumerate() {
        if file_ix > 0 {
            rows.push(RenderRow::FileGap);
        }
        summaries.push(FileSummary {
            filename: file.path.clone().into(),
            status: file.status,
            additions: file.additions,
            deletions: file.deletions,
            row_index: rows.len(),
            row_range: rows.len()..rows.len(),
        });
        rows.extend(file_rows(file, theme, options));
    }
    // Each file spans from its own header up to the NEXT file's header, so the
    // gap emitted before that header is this file's trailing row.
    for ix in 0..summaries.len() {
        let end = summaries
            .get(ix + 1)
            .map(|next| next.row_index)
            .unwrap_or(rows.len());
        summaries[ix].row_range = summaries[ix].row_index..end;
    }
    (rows, summaries)
}

/// The flat row list projected onto the rows the virtual list actually
/// renders — `result[i]` is the row index of the i-th rendered row.
///
/// Collapse is PRESENTATIONAL: there is one projection rule, and an
/// all-expanded `expanded` set is the identity. A collapsed file contributes
/// its header row plus the trailing [`RenderRow::FileGap`] only — that gap IS
/// the 8px between cards.
///
/// Pure (no gpui App/Window) so the projection is unit-testable.
pub(crate) fn project_rows(
    rows: &[RenderRow],
    files: &[FileSummary],
    expanded: &std::collections::HashSet<usize>,
) -> Vec<usize> {
    let mut visible = Vec::with_capacity(files.len() * 2);
    for (file_ix, summary) in files.iter().enumerate() {
        if expanded.contains(&file_ix) {
            visible.extend(summary.row_range.clone());
            continue;
        }
        visible.push(summary.row_range.start);
        let last = summary.row_range.end.saturating_sub(1);
        if last > summary.row_range.start && matches!(rows.get(last), Some(RenderRow::FileGap)) {
            visible.push(last);
        }
    }
    visible
}

/// The card shape of the i-th VISIBLE row: which slice of its file's frame it
/// paints. A file whose only visible row is its header is one whole card.
fn row_shape(rows: &[RenderRow], visible: &[usize], item: usize) -> RowShape {
    let Some(&ix) = visible.get(item) else {
        return RowShape::Gap;
    };
    if matches!(rows.get(ix), Some(RenderRow::FileGap)) {
        return RowShape::Gap;
    }
    let first = item == 0
        || visible
            .get(item - 1)
            .is_none_or(|&prev| matches!(rows.get(prev), Some(RenderRow::FileGap)));
    let last = visible
        .get(item + 1)
        .is_none_or(|&next| matches!(rows.get(next), Some(RenderRow::FileGap)));
    match (first, last) {
        (true, true) => RowShape::Only,
        (true, false) => RowShape::Top,
        (false, true) => RowShape::Bottom,
        (false, false) => RowShape::Middle,
    }
}

/// Which file's header the sticky overlay shows at `scroll_y`: the LAST file
/// whose header has already scrolled past the top and whose span has not yet
/// left. Pure — the list's own geometry (`sizes` in projection order, the
/// projection, the summaries) in, a file index out.
pub(crate) fn sticky_file(
    sizes: &[f32],
    visible: &[usize],
    files: &[FileSummary],
    scroll_y: f32,
) -> Option<usize> {
    if scroll_y <= 0. {
        return None;
    }
    // Prefix sums: `offset[i]` is the top of visible row `i`.
    let mut offset = 0f32;
    let mut current: Option<usize> = None;
    for (item, &ix) in visible.iter().enumerate() {
        let height = sizes.get(item).copied().unwrap_or(0.);
        if let Some(file_ix) = files.iter().position(|file| file.row_index == ix) {
            if offset <= scroll_y {
                current = Some(file_ix);
            } else {
                break;
            }
        }
        // A file whose whole span has scrolled past leaves nothing to stick.
        if let Some(file_ix) = current {
            let span_end = files
                .get(file_ix + 1)
                .map(|next| next.row_index)
                .unwrap_or(usize::MAX);
            if ix >= span_end {
                current = None;
            }
        }
        offset += height;
    }
    current
}

/// The inner content width the longest line needs. Character widths are
/// estimated (`CHAR_W`) — generous enough that nothing clips, exact enough
/// that the scroll range stays sane.
fn required_text_width(rows: &[RenderRow]) -> f32 {
    let mut max_chars = 0usize;
    for row in rows {
        match row {
            RenderRow::Line { text, .. } => max_chars = max_chars.max(text.chars().count()),
            RenderRow::HunkHeader { header } => max_chars = max_chars.max(header.chars().count()),
            _ => {}
        }
    }
    max_chars as f32 * CHAR_W + 16.
}

// ---------------------------------------------------------------------------
// Prepared diffs
// ---------------------------------------------------------------------------

/// Prebuilt render rows + per-file summaries, produced off the foreground and
/// installed with [`DiffView::set_prepared`]. Carries the [`DiffOptions`] it
/// was built at, so the rows and the paint can never disagree.
pub struct PreparedDiff {
    rows: Vec<RenderRow>,
    summaries: Vec<FileSummary>,
    options: DiffOptions,
}

/// GitHub's [`PullFile`]s → the shared model through
/// [`domain::diff::from_pull_file`], the ONE mapping the contract fixture's
/// `pullFile` cases lock ×4.
pub fn files_from_pull(files: &[PullFile]) -> Vec<DiffFile> {
    files
        .iter()
        .map(|file| {
            domain::diff::from_pull_file(
                &file.filename,
                file.previous_filename.as_deref(),
                &file.status,
                i64::from(file.additions),
                i64::from(file.deletions),
                file.patch.as_deref(),
            )
        })
        .collect()
}

/// THE build: files → rows, off the foreground (needs only a
/// [`HighlightTheme`], no gpui App/Window).
pub fn build_diff(
    files: &[DiffFile],
    theme: &HighlightTheme,
    options: DiffOptions,
) -> PreparedDiff {
    let (rows, summaries) = build_rows(files, theme, &options);
    PreparedDiff {
        rows,
        summaries,
        options,
    }
}

/// [`build_diff`] from `issues.prFiles` / `repositories.branchDiff`.
pub fn build_pr_diff(
    files: &[PullFile],
    theme: &HighlightTheme,
    options: DiffOptions,
) -> PreparedDiff {
    build_diff(&files_from_pull(files), theme, options)
}

/// [`build_diff`] from the desktop's own `git diff` (`coding::scm`) — the
/// Source Control screen and the session's Changes face.
pub fn build_scm_diff(
    files: &[DiffFile],
    theme: &HighlightTheme,
    options: DiffOptions,
) -> PreparedDiff {
    build_diff(files, theme, options)
}

// ---------------------------------------------------------------------------
// HScroll — one offset, the whole width
// ---------------------------------------------------------------------------

/// A [`ScrollbarHandle`] over a plain shared offset: the horizontal scrollbar
/// drags it and the wheel handler nudges it. Unified rows are ONE column, so
/// the offset applies at full strength (the side-by-side halving is gone).
/// Offsets are ≤ 0, matching the convention every gpui scroll handle uses.
#[derive(Clone, Default)]
struct HScroll {
    inner: Rc<RefCell<HScrollState>>,
}

#[derive(Default)]
struct HScrollState {
    x: Pixels,
    content_w: Pixels,
    viewport: Bounds<Pixels>,
}

impl HScroll {
    fn shift(&self) -> Pixels {
        self.inner.borrow().x
    }

    fn set_content_width(&self, width: Pixels) {
        let mut state = self.inner.borrow_mut();
        if state.content_w != width {
            state.content_w = width;
            state.x = px(0.);
        }
    }

    fn set_viewport_bounds(&self, bounds: Bounds<Pixels>) {
        self.inner.borrow_mut().viewport = bounds;
        self.clamp();
    }

    fn scroll_by(&self, dx: Pixels) {
        self.inner.borrow_mut().x += dx;
        self.clamp();
    }

    fn clamp(&self) {
        let mut state = self.inner.borrow_mut();
        let min = (state.viewport.size.width - state.content_w).min(px(0.));
        state.x = state.x.clamp(min, px(0.));
    }
}

impl ScrollbarHandle for HScroll {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.inner.borrow().viewport
    }

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

// ---------------------------------------------------------------------------
// Row painting (free — shared with the transcript's edit card)
// ---------------------------------------------------------------------------

/// Paint one row. Free (`&App` only) so the transcript's inline edit card
/// renders the SAME rows as the full pane without hosting a [`DiffView`].
///
/// `shape` says which slice of the file's card frame the row draws — and, for
/// a [`RenderRow::FileHeader`], whether the file is open: a header whose shape
/// is [`RowShape::Only`] is a whole card, i.e. a COLLAPSED file, so its
/// chevron points down.
pub(crate) fn render_diff_row(
    row: &RenderRow,
    shape: RowShape,
    shift: Pixels,
    options: &DiffOptions,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let mono = theme.mono_font_family.clone();
    let text_size = px(options.text_size());
    match row {
        RenderRow::FileGap => div().w_full().h(px(FILE_GAP_H)).into_any_element(),
        RenderRow::FileHeader { .. } => {
            let expanded = shape != RowShape::Only;
            let chevron = gpui_component::Icon::new(crate::icons::registry::UI_CHEVRON_DOWN)
                .xsmall()
                .text_color(theme.muted_foreground);
            let chevron: AnyElement = if expanded {
                chevron
                    .transform(gpui::Transformation::rotate(gpui::percentage(0.5)))
                    .into_any_element()
            } else {
                chevron.into_any_element()
            };
            file_header_row(row, shape, options, chevron, cx).into_any_element()
        }
        RenderRow::Note { message } => card_row(shape)
            .flex()
            .items_center()
            .h(px(NOTE_ROW_H))
            .px_2()
            .text_size(text_size)
            .text_color(theme.muted_foreground)
            .child(message.clone())
            .into_any_element(),
        RenderRow::HunkHeader { header } => card_row(shape)
            .flex()
            .items_center()
            .h(px(options.line_h()))
            .bg(theme::tokens::diff::HUNK_BG.to_hsla())
            .text_color(theme::tokens::diff::HUNK_FG.to_hsla())
            .font_family(mono)
            .text_size(text_size)
            .child(
                div()
                    .ml(shift)
                    .px_2()
                    .whitespace_nowrap()
                    .child(header.clone()),
            )
            .into_any_element(),
        RenderRow::Unchanged { lines } => card_row(shape)
            .flex()
            .items_center()
            .justify_center()
            .h(px(options.line_h()))
            .bg(theme::tokens::diff::HUNK_BG.to_hsla().opacity(0.6))
            .text_color(theme::tokens::diff::GUTTER_FG.to_hsla())
            .font_family(mono)
            .text_size(text_size)
            .child(SharedString::from(unchanged_label(*lines)))
            .into_any_element(),
        RenderRow::Line {
            kind,
            old_no,
            new_no,
            text,
            highlights,
        } => {
            let gutter = theme::tokens::diff::GUTTER_FG.to_hsla();
            let (wash, sign, sign_color) = match kind {
                DiffLineKind::Add => (
                    Some(theme::tokens::diff::ADD_BG.to_hsla()),
                    "+",
                    theme::tokens::diff::ADD_FG.to_hsla(),
                ),
                DiffLineKind::Del => (
                    Some(theme::tokens::diff::DEL_BG.to_hsla()),
                    "\u{2212}",
                    theme::tokens::diff::DEL_FG.to_hsla(),
                ),
                DiffLineKind::Context => (None, "", gutter),
                // `\ No newline at end of file` — numbered on neither side.
                DiffLineKind::Meta => (None, "", gutter),
            };
            let number = |value: Option<u32>| {
                div()
                    .w(px(GUTTER_W))
                    .h_full()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .px_1()
                    .text_color(gutter)
                    .child(SharedString::from(
                        value.map(|n| n.to_string()).unwrap_or_default(),
                    ))
            };
            let mut line = card_row(shape)
                .flex()
                .items_center()
                .h(px(options.line_h()))
                .font_family(mono)
                .text_size(text_size);
            if let Some(wash) = wash {
                line = line.bg(wash);
            }
            // The OLD gutter is the first thing a narrow surface gives up —
            // the transcript's inline card is ~400px wide and the new-side
            // number is the one a reader follows.
            if !options.compact {
                line = line.child(number(*old_no));
            }
            line.child(number(*new_no))
                .child(
                    div()
                        .w(px(SIGN_W))
                        .h_full()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(sign_color)
                        .child(sign),
                )
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .min_w(px(0.))
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .flex_shrink_0()
                                .ml(shift)
                                .pr_2()
                                .whitespace_nowrap()
                                .text_color(if *kind == DiffLineKind::Meta {
                                    gutter
                                } else {
                                    theme.foreground.opacity(0.9)
                                })
                                .child(
                                    StyledText::new(if text.is_empty() {
                                        SharedString::from(" ")
                                    } else {
                                        text.clone()
                                    })
                                    .with_highlights(highlights.iter().cloned()),
                                ),
                        ),
                )
                .into_any_element()
        }
    }
}

/// The file card's header, with the caller's chevron (static in
/// [`render_diff_row`], animated in the view). Returns a `Div` so the view can
/// hang an id, a hover and a click on it.
fn file_header_row(
    row: &RenderRow,
    shape: RowShape,
    options: &DiffOptions,
    chevron: AnyElement,
    cx: &App,
) -> gpui::Div {
    let RenderRow::FileHeader {
        name,
        dir,
        previous_path,
        status,
        additions,
        deletions,
        ..
    } = row
    else {
        return card_row(shape);
    };
    let theme = cx.theme();
    let mono = theme.mono_font_family.clone();
    let muted = theme.muted_foreground;
    let text_size = px(options.text_size() + 1.);
    let mut header = card_row(shape)
        .flex()
        .items_center()
        .h(px(options.header_h()))
        .px_2()
        .gap_2()
        .text_size(text_size)
        .font_family(mono)
        .child(
            div()
                .flex_shrink_0()
                .w(px(12.))
                .text_center()
                .text_color(status_color(*status, cx))
                .child(status_letter(*status)),
        )
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(muted)
                        .child(dir.clone()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_color(theme.foreground)
                        .child(name.clone()),
                ),
        );
    if let Some(previous) = previous_path {
        header = header.child(
            div()
                .min_w_0()
                .flex_shrink(1.)
                .truncate()
                .text_color(muted)
                .child(SharedString::from(format!("\u{2190} {previous}"))),
        );
    }
    header
        .child(
            div()
                .flex_shrink_0()
                .text_color(theme::tokens::diff::ADD_FG.to_hsla())
                .child(SharedString::from(additions_label(*additions))),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_color(theme::tokens::diff::DEL_FG.to_hsla())
                .child(SharedString::from(deletions_label(*deletions))),
        )
        .child(div().flex_shrink_0().child(chevron))
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

enum Phase {
    Loading,
    Error(SharedString),
    Ready,
}

/// The unified diff component. Construct with [`DiffView::new`], then install
/// a background-built [`PreparedDiff`] via [`DiffView::set_prepared`] (see
/// [`build_diff`]), push an error via [`DiffView::set_error`], or let it fetch
/// itself via [`DiffView::fetch`]. Read-only; embeds anywhere.
pub struct DiffView {
    focus_handle: FocusHandle,
    phase: Phase,
    rows: Vec<RenderRow>,
    options: DiffOptions,
    /// Sizes of the VISIBLE rows, in `visible` order.
    sizes: Rc<Vec<Size<Pixels>>>,
    files: Vec<FileSummary>,
    /// File indices currently unfolded.
    expanded: std::collections::HashSet<usize>,
    /// `visible[i]` is the row index of the i-th rendered row.
    visible: Vec<usize>,
    /// The last toggled file + a replay counter: only the file the user just
    /// clicked animates its chevron.
    chevron_anim: Option<(usize, u64)>,
    chevron_seq: u64,
    text_w: f32,
    h_scroll: HScroll,
    scroll: VirtualListScrollHandle,
}

impl DiffView {
    /// A new, empty view in the loading state. The options arrive with the
    /// first [`PreparedDiff`]; [`DiffView::set_options`] presets them for the
    /// self-fetching path.
    pub fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            phase: Phase::Loading,
            rows: Vec::new(),
            options: DiffOptions::default(),
            sizes: Rc::new(Vec::new()),
            files: Vec::new(),
            expanded: std::collections::HashSet::new(),
            visible: Vec::new(),
            chevron_anim: None,
            chevron_seq: 0,
            text_w: 0.,
            h_scroll: HScroll::default(),
            scroll: VirtualListScrollHandle::new(),
        }
    }

    /// The options [`DiffView::fetch`] builds at. A [`PreparedDiff`] carries
    /// its own and overrides this on install.
    pub fn set_options(&mut self, options: DiffOptions) {
        self.options = options;
    }

    /// Back to the loading state (e.g. before a re-fetch).
    pub fn set_loading(&mut self, cx: &mut gpui::Context<Self>) {
        self.phase = Phase::Loading;
        self.clear_rows();
        cx.notify();
    }

    /// Show a load failure (web parity: "Couldn't load changes: …").
    pub fn set_error(&mut self, message: impl Into<SharedString>, cx: &mut gpui::Context<Self>) {
        self.phase = Phase::Error(message.into());
        self.clear_rows();
        cx.notify();
    }

    fn clear_rows(&mut self) {
        self.rows.clear();
        self.sizes = Rc::new(Vec::new());
        self.files.clear();
        self.visible.clear();
        self.expanded.clear();
        self.chevron_anim = None;
    }

    /// Install a [`PreparedDiff`] built off the foreground and flip to Ready.
    /// The Tree-sitter-heavy build is already done — this is only the cheap
    /// pointer swap on the UI thread.
    pub fn set_prepared(&mut self, prepared: PreparedDiff, cx: &mut gpui::Context<Self>) {
        self.options = prepared.options;
        self.set_rows(prepared.rows, prepared.summaries, cx);
    }

    fn set_rows(
        &mut self,
        rows: Vec<RenderRow>,
        summaries: Vec<FileSummary>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.text_w = required_text_width(&rows);
        self.h_scroll
            .set_content_width(px(2. * GUTTER_W + SIGN_W + self.text_w));
        self.rows = rows;
        self.files = summaries;
        self.expanded.clear();
        if !self.options.collapsible || self.options.start_expanded {
            self.expanded.extend(0..self.files.len());
        }
        self.chevron_anim = None;
        self.rebuild_projection();
        self.phase = Phase::Ready;
        cx.notify();
    }

    fn rebuild_projection(&mut self) {
        self.visible = project_rows(&self.rows, &self.files, &self.expanded);
        let options = self.options;
        self.sizes = Rc::new(
            self.visible
                .iter()
                .map(|&ix| {
                    size(
                        px(100.),
                        self.rows
                            .get(ix)
                            .map(|row| row.height(&options))
                            .unwrap_or(px(0.)),
                    )
                })
                .collect(),
        );
    }

    /// Expand/collapse one file. No-op while the mode is not collapsible.
    fn toggle_file(&mut self, file_ix: usize, cx: &mut gpui::Context<Self>) {
        if !self.options.collapsible || file_ix >= self.files.len() {
            return;
        }
        if !self.expanded.remove(&file_ix) {
            self.expanded.insert(file_ix);
        }
        self.chevron_seq += 1;
        self.chevron_anim = Some((file_ix, self.chevron_seq));
        self.rebuild_projection();
        cx.notify();
    }

    /// Fetch `issues.prFiles` for `issue_id` on the background executor and
    /// populate the view. Both the blocking HTTP and the row build stay off
    /// the foreground; only the cheap `set_prepared` swap runs on the UI
    /// thread.
    pub fn fetch(
        &mut self,
        client: Arc<api::TrpcClient>,
        issue_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        self.set_loading(cx);
        let theme = cx.theme().highlight_theme.clone();
        let options = self.options;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    api::issues::pr_files(&client, &issue_id)
                        .map(|pr| build_pr_diff(&pr.files, &theme, options))
                })
                .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(prepared) => this.set_prepared(prepared, cx),
                Err(err) => this.set_error(err.to_string(), cx),
            });
        })
        .detach();
    }

    /// Per-file summaries in render order (drives an external file list).
    pub fn files(&self) -> &[FileSummary] {
        &self.files
    }

    /// Scroll the diff so `file_ix`'s header row is at the top.
    pub fn scroll_to_file(&mut self, file_ix: usize, cx: &mut gpui::Context<Self>) {
        if let Some(summary) = self.files.get(file_ix) {
            let row_index = summary.row_index;
            // A collapsed target opens first — scrolling to a card that shows
            // nothing is not "selecting a file".
            if self.options.collapsible && !self.expanded.contains(&file_ix) {
                self.expanded.insert(file_ix);
                self.rebuild_projection();
            }
            if let Some(item) = self.visible.iter().position(|&ix| ix == row_index) {
                self.scroll.scroll_to_item(item, ScrollStrategy::Top);
                cx.notify();
            }
        }
    }

    // -- rendering ----------------------------------------------------------

    fn render_row(&self, item: usize, cx: &mut gpui::Context<Self>) -> AnyElement {
        let Some(&ix) = self.visible.get(item) else {
            return div().into_any_element();
        };
        let Some(row) = self.rows.get(ix) else {
            return div().into_any_element();
        };
        let shape = row_shape(&self.rows, &self.visible, item);
        if matches!(row, RenderRow::FileHeader { .. }) {
            if let Some(file_ix) = self.files.iter().position(|file| file.row_index == ix) {
                return self.render_file_card(row, shape, file_ix, cx);
            }
        }
        render_diff_row(row, shape, self.h_scroll.shift(), &self.options, cx)
    }

    /// The clickable file header: the shared header row plus an id, the hover
    /// wash and a chevron that ANIMATES for the file the user just toggled
    /// (every other header paints its settled angle — an id-per-state scheme
    /// would spin every chevron on the first frame the diff lands).
    fn render_file_card(
        &self,
        row: &RenderRow,
        shape: RowShape,
        file_ix: usize,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        use gpui::{percentage, Animation, AnimationExt as _, Transformation};
        use gpui::{InteractiveElement as _, StatefulInteractiveElement as _};
        use gpui_component::{Icon, Sizable as _};

        let expanded = self.expanded.contains(&file_ix);
        let row_hover = cx.theme().list_hover;
        let chevron = Icon::new(crate::icons::registry::UI_CHEVRON_DOWN)
            .xsmall()
            .text_color(cx.theme().muted_foreground);
        let chevron: AnyElement = match self.chevron_anim {
            Some((animating, seq)) if animating == file_ix => {
                let (from, to) = if expanded { (0.0, 0.5) } else { (0.5, 0.0) };
                chevron
                    .with_animation(
                        SharedString::from(format!("diff-chevron-{file_ix}-{seq}")),
                        Animation::new(theme::motion::STANDARD)
                            .with_easing(theme::motion::standard()),
                        move |this, delta| {
                            this.transform(Transformation::rotate(percentage(
                                from + (to - from) * delta,
                            )))
                        },
                    )
                    .into_any_element()
            }
            _ if expanded => chevron
                .transform(Transformation::rotate(percentage(0.5)))
                .into_any_element(),
            _ => chevron.into_any_element(),
        };

        let header = file_header_row(row, shape, &self.options, chevron, cx);
        if !self.options.collapsible {
            return header.into_any_element();
        }
        header
            .id(SharedString::from(format!("diff-file-{file_ix}")))
            .cursor_pointer()
            .hover(move |this| this.bg(row_hover))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_file(file_ix, cx)))
            .into_any_element()
    }

    /// The sticky overlay: the current file's header, pinned over the list
    /// while its body scrolls under it. Skipped in the compact and
    /// non-collapsible modes (no disclosure = no card = nothing to stick).
    fn sticky_header(&self, cx: &mut gpui::Context<Self>) -> Option<AnyElement> {
        if !self.options.collapsible || self.options.compact {
            return None;
        }
        let scroll_y = f32::from(-self.scroll.base_handle().offset().y);
        let sizes: Vec<f32> = self
            .sizes
            .iter()
            .map(|size| f32::from(size.height))
            .collect();
        let file_ix = sticky_file(&sizes, &self.visible, &self.files, scroll_y)?;
        let row = self.rows.get(self.files.get(file_ix)?.row_index)?;
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .px_2()
                .child(self.render_file_card(row, RowShape::Top, file_ix, cx))
                .into_any_element(),
        )
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
                .child(SharedString::from(format!(
                    "Couldn't load changes: {message}"
                )))
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
                let sticky = self.sticky_header(cx);
                v_flex()
                    .id("diff-view-list")
                    .relative()
                    .size_full()
                    .on_prepaint(move |bounds, _, _| {
                        prepaint_scroll.set_viewport_bounds(bounds);
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
                                    .map(|item| this.render_row(item, cx))
                                    .collect::<Vec<_>>()
                            },
                        )
                        .track_scroll(&self.scroll)
                        .p_2(),
                    )
                    .children(sticky)
                    .scrollbar(&self.scroll, ScrollbarAxis::Vertical)
                    .scrollbar(&h_scroll, ScrollbarAxis::Horizontal)
                    .into_any_element()
            }
        };
        // EXP-269: no opaque fill — the diff floats on the page gradient.
        v_flex().size_full().child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small real patch, inline: the fixtures moved to
    /// `packages/domain-contract/fixtures/diff/cases.json` (EXP-895), which is
    /// where parsing is proven. These two only need to be REAL enough to
    /// render.
    const TS_PATCH: &str = "\
@@ -12,5 +12,6 @@ export async function openPullRequest(
   const branch = issueBranchName(issue.identifier)
   const base = await resolveDefaultBranch(repo)
-  const title = issue.title
+  const title = `${issue.identifier} ${issue.title}`
+  const body = renderPrBody(issue)
   return createPull({ repo, base, branch, title })
 }
";

    const RS_PATCH: &str = "\
@@ -68,3 +68,5 @@ pub mod theme;
 pub mod tokens;
 pub mod motion;
+pub mod terminal;
+
 pub use tokens::*;
";

    fn ts_file() -> PullFile {
        PullFile {
            filename: "apps/web/src/lib/integrations/github-pr.ts".into(),
            status: "modified".into(),
            additions: 2,
            deletions: 1,
            patch: Some(TS_PATCH.to_string()),
            sha: None,
            previous_filename: None,
        }
    }

    fn rs_file() -> PullFile {
        PullFile {
            filename: "apps/desktop/crates/theme/src/lib.rs".into(),
            status: "modified".into(),
            additions: 2,
            deletions: 0,
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

    fn build(files: &[PullFile], options: DiffOptions) -> (Vec<RenderRow>, Vec<FileSummary>) {
        let theme = HighlightTheme::default_dark();
        build_rows(&files_from_pull(files), &theme, &options)
    }

    fn all_expanded(files: &[FileSummary]) -> std::collections::HashSet<usize> {
        (0..files.len()).collect()
    }

    #[test]
    fn builds_header_hunks_and_unified_lines_from_a_real_patch() {
        let (rows, summaries) = build(&[ts_file()], DiffOptions::review());
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].row_index, 0);
        assert!(
            matches!(&rows[0], RenderRow::FileHeader { path, name, dir, .. }
            if path.as_ref() == "apps/web/src/lib/integrations/github-pr.ts"
                && name.as_ref() == "github-pr.ts"
                && dir.as_ref() == "apps/web/src/lib/integrations/")
        );
        let hunk_headers = rows
            .iter()
            .filter(|row| matches!(row, RenderRow::HunkHeader { .. }))
            .count();
        assert_eq!(hunk_headers, 1);
        // One row per patch line, in patch order — nothing is paired, nothing
        // is filled: the unified list IS the hunk.
        let lines: Vec<&RenderRow> = rows
            .iter()
            .filter(|row| matches!(row, RenderRow::Line { .. }))
            .collect();
        assert_eq!(lines.len(), 7);
        let kinds: Vec<DiffLineKind> = lines
            .iter()
            .filter_map(|row| match row {
                RenderRow::Line { kind, .. } => Some(*kind),
                _ => None,
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                DiffLineKind::Context,
                DiffLineKind::Context,
                DiffLineKind::Del,
                DiffLineKind::Add,
                DiffLineKind::Add,
                DiffLineKind::Context,
                DiffLineKind::Context,
            ]
        );
    }

    /// Every unified line carries BOTH sides' numbers where it has them: a
    /// context line is numbered twice, a deletion only on the old side, an
    /// addition only on the new one.
    #[test]
    fn unified_rows_carry_both_gutters() {
        let (rows, _) = build(&[ts_file()], DiffOptions::review());
        let mut seen = 0;
        for row in &rows {
            let RenderRow::Line {
                kind,
                old_no,
                new_no,
                ..
            } = row
            else {
                continue;
            };
            seen += 1;
            match kind {
                DiffLineKind::Context => {
                    assert!(old_no.is_some() && new_no.is_some());
                }
                DiffLineKind::Del => {
                    assert!(old_no.is_some() && new_no.is_none());
                }
                DiffLineKind::Add => {
                    assert!(old_no.is_none() && new_no.is_some());
                }
                DiffLineKind::Meta => assert!(old_no.is_none() && new_no.is_none()),
            }
        }
        assert!(seen > 0);
    }

    /// The `N unchanged lines` divider comes off the HUNK HEADERS' arithmetic
    /// (`unchanged_before` / `unchanged_between`), and it is a plain row.
    #[test]
    fn unchanged_dividers_come_from_the_hunk_headers() {
        let (rows, _) = build(&[ts_file()], DiffOptions::review());
        // `@@ -12,6 +12,7 @@` skips 11 lines of context before it.
        let dividers: Vec<u32> = rows
            .iter()
            .filter_map(|row| match row {
                RenderRow::Unchanged { lines } => Some(*lines),
                _ => None,
            })
            .collect();
        assert_eq!(dividers, vec![11]);
        // It sits directly ABOVE its hunk header.
        let at = rows
            .iter()
            .position(|row| matches!(row, RenderRow::Unchanged { .. }))
            .expect("the divider is present");
        assert!(matches!(&rows[at + 1], RenderRow::HunkHeader { .. }));
    }

    #[test]
    fn abutting_hunks_get_no_divider() {
        // Two hunks that touch: `@@ -1,2 +1,2 @@` runs to line 2, and the next
        // starts at 3 — nothing was skipped, so nothing is announced.
        let file = PullFile {
            filename: "a.txt".into(),
            status: "modified".into(),
            additions: 2,
            deletions: 2,
            patch: Some("@@ -1,2 +1,2 @@\n-a\n+A\n-b\n+B\n@@ -3,1 +3,1 @@\n-c\n+C\n".to_string()),
            sha: None,
            previous_filename: None,
        };
        let (rows, _) = build(&[file], DiffOptions::review());
        assert!(!rows
            .iter()
            .any(|row| matches!(row, RenderRow::Unchanged { .. })));
        assert_eq!(
            rows.iter()
                .filter(|row| matches!(row, RenderRow::HunkHeader { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn meta_lines_render_without_line_numbers() {
        let file = PullFile {
            filename: "a.txt".into(),
            status: "modified".into(),
            additions: 1,
            deletions: 1,
            patch: Some("@@ -1 +1 @@\n-a\n\\ No newline at end of file\n+b\n".to_string()),
            sha: None,
            previous_filename: None,
        };
        let (rows, _) = build(&[file], DiffOptions::review());
        let meta = rows
            .iter()
            .find_map(|row| match row {
                RenderRow::Line {
                    kind: DiffLineKind::Meta,
                    old_no,
                    new_no,
                    text,
                    highlights,
                } => Some((old_no, new_no, text, highlights)),
                _ => None,
            })
            .expect("the `\\ No newline` row is kept");
        assert!(meta.0.is_none() && meta.1.is_none());
        assert_eq!(meta.2.as_ref(), "No newline at end of file");
        // It is not code, so it never takes a highlight run.
        assert!(meta.3.is_empty());
    }

    #[test]
    fn binary_file_renders_note_not_lines() {
        let (rows, summaries) = build(&[binary_file()], DiffOptions::review());
        assert_eq!(rows.len(), 2);
        assert!(matches!(&rows[1], RenderRow::Note { message }
            if message.as_ref() == "Empty file added"));
        assert_eq!(summaries[0].status, DiffStatus::Added);
    }

    #[test]
    fn renamed_file_without_patch_says_renamed() {
        let file = PullFile {
            filename: "src/new-name.ts".into(),
            status: "renamed".into(),
            additions: 0,
            deletions: 0,
            patch: None,
            sha: None,
            previous_filename: Some("src/old-name.ts".into()),
        };
        let (rows, _) = build(&[file], DiffOptions::review());
        assert!(
            matches!(&rows[0], RenderRow::FileHeader { previous_path: Some(p), .. }
            if p.as_ref() == "src/old-name.ts")
        );
        assert!(matches!(&rows[1], RenderRow::Note { message }
            if message.as_ref() == "Renamed without content changes"));
    }

    #[test]
    fn multi_file_summaries_point_at_their_header_rows() {
        let files = [ts_file(), rs_file(), binary_file()];
        let (rows, summaries) = build(&files, DiffOptions::review());
        assert_eq!(summaries.len(), 3);
        for summary in &summaries {
            assert!(
                matches!(&rows[summary.row_index], RenderRow::FileHeader { path, .. }
                if path == &summary.filename)
            );
        }
        let gaps = rows
            .iter()
            .filter(|row| matches!(row, RenderRow::FileGap))
            .count();
        assert_eq!(gaps, 2);
    }

    #[test]
    fn syntax_highlight_spans_attach_to_their_lines() {
        // The RS patch adds `pub mod terminal;` — a keyword-bearing added line.
        // With the rust grammar enabled it must carry a non-default run.
        let (rows, _) = build(&[rs_file()], DiffOptions::review());
        let line = rows
            .iter()
            .find_map(|row| match row {
                RenderRow::Line {
                    kind: DiffLineKind::Add,
                    text,
                    highlights,
                    ..
                } if text.as_ref() == "pub mod terminal;" => Some((text, highlights)),
                _ => None,
            })
            .expect("the added `pub mod terminal;` line is present");
        assert!(
            !line.1.is_empty(),
            "rust keywords on an added line must be highlighted"
        );
        for (range, _) in line.1 {
            assert!(range.end <= line.0.len());
        }
    }

    #[test]
    fn compact_rows_are_shorter_and_unhighlighted() {
        let theme = HighlightTheme::default_dark();
        let compact = DiffOptions::card();
        let (rows, _) = build_rows(&files_from_pull(&[rs_file()]), &theme, &compact);
        assert!(rows.iter().all(|row| match row {
            RenderRow::Line { highlights, .. } => highlights.is_empty(),
            _ => true,
        }));
        let line = rows
            .iter()
            .find(|row| matches!(row, RenderRow::Line { .. }))
            .expect("a line row");
        assert!(line.height(&compact) < line.height(&DiffOptions::review()));
        let header = &rows[0];
        assert!(header.height(&compact) < header.height(&DiffOptions::review()));
    }

    /// EXP-895: THREE producers, ONE row list. A `git diff` read by
    /// `coding::scm` (the Source Control screen, the run's published diff)
    /// and a GitHub `PullFile` are the same [`DiffFile`] by the time they
    /// reach here, so they render through the very same rows.
    #[test]
    fn domain_diff_files_render_through_the_same_rows() {
        let patch = concat!(
            "diff --git a/apps/web/src/lib/integrations/github-pr.ts",
            " b/apps/web/src/lib/integrations/github-pr.ts\n",
            "index 111..222 100644\n",
            "--- a/apps/web/src/lib/integrations/github-pr.ts\n",
            "+++ b/apps/web/src/lib/integrations/github-pr.ts\n",
            "@@ -12,5 +12,6 @@ export async function openPullRequest(\n",
            "   const branch = issueBranchName(issue.identifier)\n",
            "   const base = await resolveDefaultBranch(repo)\n",
            "-  const title = issue.title\n",
            "+  const title = `${issue.identifier} ${issue.title}`\n",
            "+  const body = renderPrBody(issue)\n",
            "   return createPull({ repo, base, branch, title })\n",
            " }\n",
        );
        let theme = HighlightTheme::default_dark();
        let from_git = coding::scm::parse_unified_diff(patch);
        let (git_rows, git_files) = build_rows(&from_git, &theme, &DiffOptions::review());
        let (pull_rows, pull_files) = build(&[ts_file()], DiffOptions::review());
        assert_eq!(git_files, pull_files);
        assert_eq!(git_rows.len(), pull_rows.len());
        for (git, pull) in git_rows.iter().zip(pull_rows.iter()) {
            match (git, pull) {
                (
                    RenderRow::Line {
                        kind: a,
                        old_no: ao,
                        new_no: an,
                        text: at,
                        ..
                    },
                    RenderRow::Line {
                        kind: b,
                        old_no: bo,
                        new_no: bn,
                        text: bt,
                        ..
                    },
                ) => assert_eq!((a, ao, an, at), (b, bo, bn, bt)),
                (RenderRow::HunkHeader { header: a }, RenderRow::HunkHeader { header: b }) => {
                    assert_eq!(a, b)
                }
                (RenderRow::Unchanged { lines: a }, RenderRow::Unchanged { lines: b }) => {
                    assert_eq!(a, b)
                }
                (RenderRow::FileHeader { path: a, .. }, RenderRow::FileHeader { path: b, .. }) => {
                    assert_eq!(a, b)
                }
                (RenderRow::FileGap, RenderRow::FileGap) => {}
                (a, b) => panic!(
                    "row kinds diverge: {a:?} vs {b:?}",
                    a = kind_of(a),
                    b = kind_of(b)
                ),
            }
        }
    }

    fn kind_of(row: &RenderRow) -> &'static str {
        match row {
            RenderRow::FileHeader { .. } => "file header",
            RenderRow::Note { .. } => "note",
            RenderRow::HunkHeader { .. } => "hunk header",
            RenderRow::Unchanged { .. } => "unchanged",
            RenderRow::Line { .. } => "line",
            RenderRow::FileGap => "gap",
        }
    }

    #[test]
    fn pull_files_map_onto_the_domain_model() {
        let files = files_from_pull(&[ts_file(), binary_file(), {
            let mut renamed = rs_file();
            renamed.status = "renamed".into();
            renamed.previous_filename = Some("crates/theme/src/old.rs".into());
            renamed
        }]);
        assert_eq!(files[0].status, DiffStatus::Modified);
        assert_eq!(files[0].path, "apps/web/src/lib/integrations/github-pr.ts");
        assert_eq!((files[0].additions, files[0].deletions), (2, 1));
        // No patch → no hunks, and GitHub's own counts stand in.
        assert_eq!(files[1].status, DiffStatus::Added);
        assert!(files[1].hunks.is_empty());
        assert_eq!(files[2].status, DiffStatus::Renamed);
        assert_eq!(
            files[2].previous_path.as_deref(),
            Some("crates/theme/src/old.rs")
        );
    }

    // -- the projection -----------------------------------------------------

    #[test]
    fn file_row_ranges_tile_the_flat_row_list() {
        let files = [ts_file(), binary_file(), rs_file()];
        let (rows, summaries) = build(&files, DiffOptions::review());
        assert_eq!(summaries.len(), 3);
        for summary in &summaries {
            assert_eq!(summary.row_range.start, summary.row_index);
            assert!(
                matches!(&rows[summary.row_range.start], RenderRow::FileHeader { path, .. }
                if path == &summary.filename)
            );
        }
        for pair in summaries.windows(2) {
            assert_eq!(pair[0].row_range.end, pair[1].row_range.start);
            assert!(matches!(
                &rows[pair[0].row_range.end - 1],
                RenderRow::FileGap
            ));
        }
        assert_eq!(summaries[2].row_range.end, rows.len());
    }

    /// Collapse is presentational: an ALL-EXPANDED set is the identity, which
    /// is exactly what the non-collapsible surfaces (Source Control, the
    /// transcript card) render through.
    #[test]
    fn an_all_expanded_projection_is_the_identity() {
        let (rows, summaries) = build(&[ts_file(), rs_file()], DiffOptions::source_control());
        let visible = project_rows(&rows, &summaries, &all_expanded(&summaries));
        assert_eq!(visible, (0..rows.len()).collect::<Vec<_>>());
    }

    #[test]
    fn collapsed_projection_is_headers_and_gaps_only() {
        let files = [ts_file(), binary_file(), rs_file()];
        let (rows, summaries) = build(&files, DiffOptions::review());
        let visible = project_rows(&rows, &summaries, &std::collections::HashSet::new());
        assert_eq!(visible.len(), 5, "3 headers + 2 separating gaps");
        for &ix in &visible {
            assert!(matches!(
                &rows[ix],
                RenderRow::FileHeader { .. } | RenderRow::FileGap
            ));
        }
    }

    #[test]
    fn expanding_a_file_exposes_exactly_its_row_range() {
        let files = [ts_file(), binary_file(), rs_file()];
        let (rows, summaries) = build(&files, DiffOptions::review());
        let mut expanded = std::collections::HashSet::new();
        expanded.insert(1usize);
        let visible = project_rows(&rows, &summaries, &expanded);
        for ix in summaries[1].row_range.clone() {
            assert!(visible.contains(&ix), "row {ix} of the open file is hidden");
        }
        for other in [0usize, 2] {
            let body: Vec<usize> = summaries[other]
                .row_range
                .clone()
                .filter(|&ix| {
                    !matches!(&rows[ix], RenderRow::FileHeader { .. } | RenderRow::FileGap)
                })
                .collect();
            assert!(!body.is_empty());
            for ix in body {
                assert!(!visible.contains(&ix), "row {ix} leaked while collapsed");
            }
        }
        assert!(visible.windows(2).all(|pair| pair[0] < pair[1]));
    }

    // -- card chrome --------------------------------------------------------

    /// A collapsed file is ONE row, and that row wears the whole frame.
    #[test]
    fn a_collapsed_file_is_one_rounded_card_row() {
        let files = [ts_file(), rs_file()];
        let (rows, summaries) = build(&files, DiffOptions::review());
        let visible = project_rows(&rows, &summaries, &std::collections::HashSet::new());
        // header, gap, header — the first header is a whole card.
        assert_eq!(row_shape(&rows, &visible, 0), RowShape::Only);
        assert_eq!(row_shape(&rows, &visible, 1), RowShape::Gap);
        assert_eq!(row_shape(&rows, &visible, 2), RowShape::Only);

        // Expanded, the header opens the card and the last body row closes it.
        let visible = project_rows(&rows, &summaries, &all_expanded(&summaries));
        assert_eq!(row_shape(&rows, &visible, 0), RowShape::Top);
        assert_eq!(row_shape(&rows, &visible, 1), RowShape::Middle);
        let gap = visible
            .iter()
            .position(|&ix| matches!(rows[ix], RenderRow::FileGap))
            .expect("the files are separated by a gap");
        assert_eq!(row_shape(&rows, &visible, gap - 1), RowShape::Bottom);
        assert_eq!(
            row_shape(&rows, &visible, visible.len() - 1),
            RowShape::Bottom
        );
    }

    /// The per-row frame IS `surface::glass_row_card`'s, sliced: one row that
    /// is the whole card fills, strokes on all four edges and rounds both
    /// ends; the middle rows only carry the sides through.
    #[test]
    fn the_card_chrome_matches_glass_row_card() {
        let whole = RowShape::Only.chrome();
        assert_eq!(
            whole,
            RowChrome {
                fill: true,
                border_top: true,
                border_bottom: true,
                border_sides: true,
                round_top: true,
                round_bottom: true,
            }
        );
        // Top + Middle + Bottom stacked paint exactly the same frame once.
        let (top, middle, bottom) = (
            RowShape::Top.chrome(),
            RowShape::Middle.chrome(),
            RowShape::Bottom.chrome(),
        );
        assert_eq!(top.border_top, whole.border_top);
        assert_eq!(bottom.border_bottom, whole.border_bottom);
        assert_eq!(top.round_top, whole.round_top);
        assert_eq!(bottom.round_bottom, whole.round_bottom);
        for shape in [top, middle, bottom] {
            assert!(shape.fill && shape.border_sides);
        }
        assert!(!top.border_bottom && !middle.border_top && !middle.border_bottom);
        // And the gap between two cards paints nothing at all.
        let gap = RowShape::Gap.chrome();
        assert!(!gap.fill && !gap.border_sides && !gap.round_top);
    }

    // -- the sticky header --------------------------------------------------

    #[test]
    fn the_sticky_header_follows_the_scroll_offset() {
        let files = [ts_file(), rs_file()];
        let (rows, summaries) = build(&files, DiffOptions::review());
        let options = DiffOptions::review();
        let visible = project_rows(&rows, &summaries, &all_expanded(&summaries));
        let sizes: Vec<f32> = visible
            .iter()
            .map(|&ix| f32::from(rows[ix].height(&options)))
            .collect();

        // At the very top nothing is stuck — the real header is on screen.
        assert_eq!(sticky_file(&sizes, &visible, &summaries, 0.), None);
        // A few rows down, file 0 owns the viewport.
        assert_eq!(
            sticky_file(&sizes, &visible, &summaries, sizes[0] + sizes[1]),
            Some(0)
        );
        // Past file 1's header, file 1 does.
        let second = summaries[1].row_index;
        let at = visible
            .iter()
            .position(|&ix| ix == second)
            .expect("file 1 is visible");
        let offset: f32 = sizes[..=at].iter().sum();
        assert_eq!(sticky_file(&sizes, &visible, &summaries, offset), Some(1));
    }

    #[test]
    fn the_status_letters_cover_the_contract() {
        assert_eq!(status_letter(DiffStatus::Added), "A");
        assert_eq!(status_letter(DiffStatus::Removed), "D");
        assert_eq!(status_letter(DiffStatus::Modified), "M");
        assert_eq!(status_letter(DiffStatus::Renamed), "R");
        assert_eq!(status_letter(DiffStatus::Copied), "C");
    }

    #[test]
    fn the_path_splits_into_a_dimmed_dir_and_a_basename() {
        assert_eq!(
            split_path("apps/web/src/a.ts"),
            ("apps/web/src/".to_string(), "a.ts".to_string())
        );
        assert_eq!(
            split_path("README.md"),
            (String::new(), "README.md".to_string())
        );
    }
}
