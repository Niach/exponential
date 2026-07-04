//! Read-only file viewer (masterplan v4 §4.5): opens a trunk-relative file
//! Tree-sitter highlighted (reusing the diff view's `highlighter` machinery —
//! `crate::diff::language_for_filename` + gpui-component's `SyntaxHighlighter`),
//! virtualized, no editing (L15). Binary/oversized (>2 MB) files show a
//! placeholder with size + "Open in terminal".
//!
//! The trunk-relative `path` is resolved against the per-window trunk root the
//! file tree published (`crate::file_tree::window_trunk_root`) — a file is only
//! reachable by clicking it in the tree, so the root is always resolved first.
//! Read + highlight run on the background executor (Tree-sitter over a whole
//! file is heavy); only the cheap swap-to-`Ready` runs on the UI thread.

use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::{
    div, px, size, AnyElement, App, FocusHandle, Focusable, HighlightStyle,
    InteractiveElement as _, IntoElement, ParentElement, Pixels, Render, SharedString, Size,
    Styled, StyledText, Window, WindowId,
};
use gpui_component::highlighter::{HighlightTheme, LanguageRegistry, SyntaxHighlighter};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    scroll::{ScrollableElement as _, ScrollbarAxis},
    v_flex, v_virtual_list, ActiveTheme as _, Icon, IconName, Sizable as _, VirtualListScrollHandle,
};
use ropey::Rope;

use crate::file_tree::{self, OpenTerminalHere, MAX_VIEWER_BYTES};

/// Compact code metrics (mirror of the diff view: fixed-height mono rows so the
/// virtual list can pre-size).
const CODE_TEXT_SIZE: f32 = 12.0;
const LINE_ROW_H: f32 = 18.0;

/// One rendered line: its 1-based number and precomputed highlight runs
/// (byte ranges LOCAL to the line, ready for `StyledText::with_highlights`).
struct Line {
    text: SharedString,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
}

/// The completed background read for a file — `Send` (built off the UI thread).
enum Loaded {
    /// A readable text file: its lines + highlights.
    Text(Vec<Line>),
    /// Too large to view (>2 MB) — carries the byte size for the placeholder.
    TooLarge(u64),
    /// Binary content — carries the byte size for the placeholder.
    Binary(u64),
    /// The file could not be read (missing / permission).
    Error(String),
}

enum Phase {
    /// No trunk root resolved yet, or nothing navigated.
    Idle,
    Loading,
    Ready {
        lines: Vec<Line>,
        sizes: Rc<Vec<Size<Pixels>>>,
    },
    TooLarge(u64),
    Binary(u64),
    Error(SharedString),
}

/// The read-only trunk file viewer center screen. `path` is trunk-relative;
/// [`set_path`](Self::set_path) re-points it on navigation (screens reuse one
/// instance, like the issue-detail view).
pub struct FileViewerView {
    /// Trunk-relative path of the file being shown (empty until navigated).
    path: String,
    /// This window (for the trunk-root registry lookup — `set_path` has no
    /// `&Window`, so the id is captured at construction).
    window_id: WindowId,
    /// Absolute directory of the open file (the "Open terminal here" target).
    parent_dir: Option<PathBuf>,
    phase: Phase,
    /// Stale-load guard.
    generation: u64,
    scroll: VirtualListScrollHandle,
    focus_handle: FocusHandle,
}

impl FileViewerView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self {
            path: String::new(),
            window_id: window.window_handle().window_id(),
            parent_dir: None,
            phase: Phase::Idle,
            generation: 0,
            scroll: VirtualListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Re-point at a trunk-relative `path` (called from the screens panel on
    /// `Screen::FileViewer` navigation). Reads + highlights off the foreground.
    pub fn set_path(&mut self, path: String, cx: &mut gpui::Context<Self>) {
        if self.path == path {
            return;
        }
        self.path = path.clone();
        self.parent_dir = None;

        let Some(root) = file_tree::window_trunk_root(self.window_id, cx) else {
            // The tree hasn't resolved a trunk root for this window — nothing
            // to read against (should not happen via the tree click path).
            self.phase = Phase::Idle;
            cx.notify();
            return;
        };
        let abs = root.join(&path);
        self.parent_dir = abs.parent().map(std::path::Path::to_path_buf);
        self.phase = Phase::Loading;
        self.generation += 1;
        let generation = self.generation;
        let theme = cx.theme().highlight_theme.clone();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_executor()
                .spawn(async move { read_file(&abs, &path, &theme) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return; // superseded by a newer navigation
                }
                this.phase = match loaded {
                    Loaded::Text(lines) => {
                        let sizes: Rc<Vec<Size<Pixels>>> =
                            Rc::new(lines.iter().map(|_| size(px(100.), px(LINE_ROW_H))).collect());
                        Phase::Ready { lines, sizes }
                    }
                    Loaded::TooLarge(bytes) => Phase::TooLarge(bytes),
                    Loaded::Binary(bytes) => Phase::Binary(bytes),
                    Loaded::Error(message) => Phase::Error(message.into()),
                };
                cx.notify();
            });
        })
        .detach();
    }

    // -- rendering ----------------------------------------------------------

    fn render_row(&self, ix: usize, lines: &[Line], cx: &App) -> AnyElement {
        let Some(line) = lines.get(ix) else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let gutter_w = gutter_width(lines.len());
        let text: SharedString = if line.text.is_empty() {
            " ".into()
        } else {
            line.text.clone()
        };
        h_flex()
            .w_full()
            .h(px(LINE_ROW_H))
            .font_family(theme.mono_font_family.clone())
            .text_size(px(CODE_TEXT_SIZE))
            .child(
                // Line-number gutter.
                h_flex()
                    .w(px(gutter_w))
                    .h_full()
                    .flex_shrink_0()
                    .justify_end()
                    .items_center()
                    .px_2()
                    .text_color(theme.muted_foreground.opacity(0.7))
                    .child(SharedString::from((ix + 1).to_string())),
            )
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .px_2()
                    .text_color(theme.foreground)
                    .child(StyledText::new(text).with_highlights(line.highlights.iter().cloned())),
            )
            .into_any_element()
    }

    /// Binary / oversized placeholder (§4.5): the human-readable size + an
    /// "Open in terminal" button (the `+` shell tab at the file's directory).
    fn render_unviewable(&self, headline: &str, bytes: u64, cx: &App) -> AnyElement {
        let dir = self.parent_dir.clone();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(format!("{headline} · {}", human_size(bytes)))),
            )
            .child(
                Button::new("file-viewer-open-terminal")
                    .icon(IconName::SquareTerminal)
                    .label("Open in terminal")
                    .ghost()
                    .small()
                    .on_click(move |_, window, cx| {
                        if let Some(dir) = &dir {
                            window.dispatch_action(
                                Box::new(OpenTerminalHere {
                                    path: dir.to_string_lossy().into_owned(),
                                }),
                                cx,
                            );
                        }
                    }),
            )
            .into_any_element()
    }

    fn render_notice(&self, message: &str, cx: &App) -> AnyElement {
        v_flex()
            .size_full()
            .p_4()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(message.to_string())),
            )
            .into_any_element()
    }

    /// The thin top bar naming the open file (read-only viewer, §4.5).
    fn render_header(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        h_flex()
            .flex_shrink_0()
            .w_full()
            .h(px(28.))
            .items_center()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.muted.opacity(0.3))
            .child(
                Icon::new(IconName::File)
                    .xsmall()
                    .text_color(theme.muted_foreground),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_xs()
                    .text_color(theme.foreground)
                    .child(SharedString::from(self.path.clone())),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Read-only"),
            )
            .into_any_element()
    }
}

impl Focusable for FileViewerView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FileViewerView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let body: AnyElement = match &self.phase {
            Phase::Idle => self.render_notice("Open a file from the Files panel.", cx),
            Phase::Loading => self.render_notice("Loading…", cx),
            Phase::Error(message) => {
                self.render_notice(&format!("Couldn’t open file: {message}"), cx)
            }
            Phase::TooLarge(bytes) => {
                self.render_unviewable("File is too large to preview", *bytes, cx)
            }
            Phase::Binary(bytes) => self.render_unviewable("Binary file", *bytes, cx),
            Phase::Ready { lines, sizes } if lines.is_empty() => {
                self.render_notice("Empty file.", cx)
            }
            Phase::Ready { sizes, .. } => {
                let sizes = sizes.clone();
                v_flex()
                    .id("file-viewer-list")
                    .relative()
                    .size_full()
                    .child(
                        v_virtual_list(
                            cx.entity().clone(),
                            "file-viewer-rows",
                            sizes,
                            |this, visible_range, _window, cx| {
                                let Phase::Ready { lines, .. } = &this.phase else {
                                    return Vec::new();
                                };
                                visible_range
                                    .map(|ix| this.render_row(ix, lines, cx))
                                    .collect::<Vec<_>>()
                            },
                        )
                        .track_scroll(&self.scroll)
                        .py_1(),
                    )
                    .scrollbar(&self.scroll, ScrollbarAxis::Vertical)
                    .into_any_element()
            }
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(self.render_header(cx))
            .child(div().flex_1().min_h_0().child(body))
    }
}

// ---------------------------------------------------------------------------
// Background read + highlight (pure, off the UI thread)
// ---------------------------------------------------------------------------

/// Read `abs`, classify (oversized / binary / text), and — for text — split
/// into lines with precomputed Tree-sitter highlight runs. `rel` drives the
/// language detection (reusing the diff view's filename→language mapping).
fn read_file(abs: &std::path::Path, rel: &str, theme: &HighlightTheme) -> Loaded {
    let size = match std::fs::metadata(abs) {
        Ok(meta) => meta.len(),
        Err(err) => return Loaded::Error(err.to_string()),
    };
    if size > MAX_VIEWER_BYTES {
        return Loaded::TooLarge(size);
    }
    let bytes = match std::fs::read(abs) {
        Ok(bytes) => bytes,
        Err(err) => return Loaded::Error(err.to_string()),
    };
    // Binary heuristic: a NUL byte in the first chunk (same signal editors use).
    let probe = bytes.len().min(8192);
    if bytes[..probe].contains(&0) {
        return Loaded::Binary(size);
    }

    let text = String::from_utf8_lossy(&bytes);
    let raw_lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    let lang = crate::diff::language_for_filename(rel);
    let highlights = highlight_lines(lang, &raw_lines, theme);
    let lines = raw_lines
        .into_iter()
        .zip(highlights)
        .map(|(text, highlights)| Line {
            text: SharedString::from(text.to_string()),
            highlights,
        })
        .collect();
    Loaded::Text(lines)
}

/// Human-readable byte size (`1.4 MB`, `812 KB`, `40 B`).
fn human_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= MB {
        format!("{:.1} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.0} KB", bytes_f / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Gutter width for a file with `count` lines (widest line number + padding at
/// the mono size).
fn gutter_width(count: usize) -> f32 {
    let digits = count.max(1).to_string().len().max(2) as f32;
    12.0 + digits * (CODE_TEXT_SIZE * 0.62)
}

/// Whole-file syntax highlight, split into per-line style-run vectors with
/// line-local byte ranges. Mirrors the diff view's `highlight_lines` (same
/// `SyntaxHighlighter`); plain-text / unknown languages return empty runs.
fn highlight_lines(
    lang: &str,
    lines: &[&str],
    theme: &HighlightTheme,
) -> Vec<Vec<(Range<usize>, HighlightStyle)>> {
    if lines.is_empty() {
        return Vec::new();
    }
    if lang == "text" || LanguageRegistry::singleton().language(lang).is_none() {
        return lines.iter().map(|_| Vec::new()).collect();
    }

    let doc = lines.join("\n");
    let mut highlighter = SyntaxHighlighter::new(lang);
    let rope = Rope::from_str(&doc);
    highlighter.update(None, &rope, None);
    let styles = highlighter.styles(&(0..doc.len()), theme);

    let mut per_line: Vec<Vec<(Range<usize>, HighlightStyle)>> = Vec::with_capacity(lines.len());
    let mut line_start = 0usize;
    let mut run_ix = 0usize;
    for line in lines {
        let line_end = line_start + line.len();
        let mut runs = Vec::new();
        while run_ix < styles.len() && styles[run_ix].0.end <= line_start {
            run_ix += 1;
        }
        let mut ix = run_ix;
        while ix < styles.len() && styles[ix].0.start < line_end {
            let (range, style) = &styles[ix];
            let start = range.start.max(line_start) - line_start;
            let end = range.end.min(line_end) - line_start;
            if start < end && *style != HighlightStyle::default() {
                runs.push((start..end, *style));
            }
            if range.end > line_end {
                break;
            }
            ix += 1;
        }
        per_line.push(runs);
        line_start = line_end + 1;
    }
    per_line
}
