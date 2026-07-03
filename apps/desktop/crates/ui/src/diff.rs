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

use gpui::{
    div, px, size, AnyElement, App, FocusHandle, Focusable, HighlightStyle,
    InteractiveElement as _, IntoElement, ParentElement, Pixels, Render, ScrollStrategy,
    SharedString, Size, Styled, StyledText, Window,
};
use gpui_component::{
    h_flex, scroll::ScrollableElement as _, scroll::ScrollbarAxis, theme::Colorize as _,
    v_flex, v_virtual_list, ActiveTheme as _, VirtualListScrollHandle,
};

use api::issues::PullFile;
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

enum Phase {
    Loading,
    Error(SharedString),
    Ready,
}

// ---------------------------------------------------------------------------
// The view
// ---------------------------------------------------------------------------

/// The standalone side-by-side diff component. Construct with [`new`], then
/// either push data via [`set_files`] / [`set_error`] or let it fetch itself
/// via [`fetch`]. Read-only; embeds anywhere (issue detail "Changes" tab, a
/// popped-out review window).
///
/// [`new`]: DiffView::new
/// [`set_files`]: DiffView::set_files
/// [`set_error`]: DiffView::set_error
/// [`fetch`]: DiffView::fetch
pub struct DiffView {
    focus_handle: FocusHandle,
    phase: Phase,
    rows: Vec<RenderRow>,
    sizes: Rc<Vec<Size<Pixels>>>,
    files: Vec<FileSummary>,
    scroll: VirtualListScrollHandle,
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

    /// Build and display the given PR files. Parsing + highlighting happen
    /// here, once — render only slices precomputed rows. Prefer [`fetch`],
    /// which runs the (Tree-sitter-heavy) build on the background executor;
    /// this synchronous form is for callers that already hold the files.
    ///
    /// [`fetch`]: DiffView::fetch
    pub fn set_files(&mut self, files: Vec<PullFile>, cx: &mut gpui::Context<Self>) {
        let theme = cx.theme().highlight_theme.clone();
        let (rows, summaries) = build_rows(&files, &theme);
        self.set_rows(rows, summaries, cx);
    }

    /// Install prebuilt rows (from [`build_rows`]) and flip to Ready.
    fn set_rows(
        &mut self,
        rows: Vec<RenderRow>,
        summaries: Vec<FileSummary>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.sizes = Rc::new(rows.iter().map(|r| size(px(100.), r.height())).collect());
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
                // BLUE tints (§4 tokens; no indigo token exists).
                h_flex()
                    .w_full()
                    .h(px(LINE_ROW_H))
                    .items_center()
                    .px_2()
                    .bg(theme.blue.opacity(0.05))
                    .text_color(theme.blue.lighten(0.4).opacity(0.8))
                    .font_family(mono)
                    .text_size(px(CODE_TEXT_SIZE))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(header.clone())
                    .into_any_element()
            }
            RenderRow::Line { left, right } => h_flex()
                .w_full()
                .h(px(LINE_ROW_H))
                .child(self.render_cell(left.as_ref(), cx))
                .child(div().w(px(1.)).h_full().flex_shrink_0().bg(theme.border))
                .child(self.render_cell(right.as_ref(), cx))
                .into_any_element(),
        }
    }

    /// One side of a line row: gutter (the anchor's line number) + code.
    /// `None` renders the blank filler that keeps both columns aligned.
    fn render_cell(&self, cell: Option<&RenderCell>, cx: &App) -> AnyElement {
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
                // IS the write-back anchor (they cannot drift).
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
                div()
                    .flex_1()
                    .h_full()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .px_1()
                    .text_color(theme.foreground)
                    .child(
                        StyledText::new(text).with_highlights(cell.highlights.iter().cloned()),
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
            Phase::Ready => v_flex()
                .id("diff-view-list")
                .relative()
                .size_full()
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
                .into_any_element(),
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

/// Build the flat render-row list + per-file summaries from PR files.
/// Pure — needs only a [`HighlightTheme`], no gpui App/Window.
fn build_rows(files: &[PullFile], theme: &HighlightTheme) -> (Vec<RenderRow>, Vec<FileSummary>) {
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

        let hunks = file
            .patch
            .as_deref()
            .map(patch::parse_patch)
            .unwrap_or_default();
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

        let aligned = patch::align_file(&file.filename, &hunks);

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
