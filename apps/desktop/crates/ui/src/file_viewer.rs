//! Read-only file viewer (masterplan v4 §4.5): opens trunk-relative files
//! Tree-sitter highlighted (reusing the diff view's `highlighter` machinery —
//! `crate::diff::language_for_filename` + gpui-component's `SyntaxHighlighter`),
//! virtualized, no editing (L15). Binary/oversized (>2 MB) files show a
//! placeholder with size + "Open in terminal".
//!
//! Multi-tab (P2.h): the viewer owns an ordered set of open files + an active
//! index, surfaced as a tab strip in the header. Clicking a file in the tree
//! opens it (or activates its existing tab); the strip caps at [`MAX_TABS`],
//! evicting the least-recently-activated tab. The active tab's path is the
//! `Screen::FileViewer { path }` the window navigates to, so the tree's
//! active-file highlight stays in lockstep.
//!
//! Trunk-relative paths resolve against the per-window trunk root the file tree
//! published (`crate::file_tree::window_trunk_root`) — a file is only reachable
//! by clicking it in the tree, so the root is always resolved first. Read +
//! highlight run on the background executor (Tree-sitter over a whole file is
//! heavy); only the cheap swap-to-`Ready` runs on the UI thread.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::{
    div, px, size, AnyElement, App, ClickEvent, FocusHandle, Focusable, HighlightStyle,
    InteractiveElement as _, IntoElement, ParentElement, Pixels, Render, ScrollHandle, SharedString,
    Size, Styled, StyledText, Window, WindowId,
};
use gpui_component::highlighter::{HighlightTheme, LanguageRegistry, SyntaxHighlighter};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    scroll::{ScrollableElement as _, ScrollbarAxis},
    tab::{Tab, TabBar},
    v_flex, v_virtual_list, ActiveTheme as _, Icon, IconName, Sizable as _, Size as ComponentSize,
    VirtualListScrollHandle,
};
use ropey::Rope;

use crate::file_tree::{self, OpenTerminalHere, MAX_VIEWER_BYTES};
use crate::navigation::{self, Screen};

/// Compact code metrics (mirror of the diff view: fixed-height mono rows so the
/// virtual list can pre-size).
const CODE_TEXT_SIZE: f32 = 12.0;
const LINE_ROW_H: f32 = 18.0;

/// Open-tab cap (§8.10 "~20 tabs, evict oldest inactive").
const MAX_TABS: usize = 20;

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
    /// No trunk root resolved yet (should not happen via the tree click path).
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

/// One open file: its trunk-relative path, containing directory (the "Open
/// terminal here" placeholder target), load phase, and its own scroll position
/// (preserved across tab switches).
struct FileTab {
    /// Trunk-relative path of the open file.
    path: String,
    /// Absolute directory of the open file (the "Open terminal here" target).
    parent_dir: Option<PathBuf>,
    phase: Phase,
    scroll: VirtualListScrollHandle,
    /// Stale-load guard for THIS tab (bumped on each (re)load).
    load_gen: u64,
    /// LRU marker — bumped whenever the tab becomes active (the smallest value
    /// among inactive tabs is evicted at capacity).
    activated: u64,
}

/// The read-only trunk file viewer center screen. Owns the open-tab set; screens
/// reuse one instance (like the issue-detail view) and re-point it via
/// [`set_path`](Self::set_path) on navigation.
pub struct FileViewerView {
    /// This window (for the trunk-root registry lookup — `set_path` has no
    /// `&Window`, so the id is captured at construction).
    window_id: WindowId,
    /// Open files, in tab order. Empty until the first file is navigated.
    tabs: Vec<FileTab>,
    /// Index into `tabs` of the active tab (meaningful only when non-empty).
    active: usize,
    /// Monotonic tick feeding both per-tab load generations and LRU order.
    tick: u64,
    /// Horizontal scroll for the tab strip (overflow past ~20 tabs).
    tab_scroll: ScrollHandle,
    focus_handle: FocusHandle,
}

impl FileViewerView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self {
            window_id: window.window_handle().window_id(),
            tabs: Vec::new(),
            active: 0,
            tick: 0,
            tab_scroll: ScrollHandle::new(),
            focus_handle: cx.focus_handle(),
        }
    }

    /// Open (or activate) the tab for a trunk-relative `path` — called from the
    /// screens panel on `Screen::FileViewer` navigation. A new tab reads +
    /// highlights off the foreground.
    pub fn set_path(&mut self, path: String, cx: &mut gpui::Context<Self>) {
        if path.is_empty() {
            return;
        }
        if let Some(ix) = self.tabs.iter().position(|tab| tab.path == path) {
            if ix != self.active {
                self.activate(ix, cx);
            }
            return;
        }
        self.open(path, cx);
    }

    /// Make tab `ix` active (bumping its LRU marker).
    fn activate(&mut self, ix: usize, cx: &mut gpui::Context<Self>) {
        if ix >= self.tabs.len() {
            return;
        }
        self.active = ix;
        self.tick += 1;
        self.tabs[ix].activated = self.tick;
        cx.notify();
    }

    /// Push a fresh tab for `path` (evicting the least-recently-activated tab at
    /// capacity) and kick its background load.
    fn open(&mut self, path: String, cx: &mut gpui::Context<Self>) {
        if self.tabs.len() >= MAX_TABS {
            if let Some(evict) = self
                .tabs
                .iter()
                .enumerate()
                .min_by_key(|(_, tab)| tab.activated)
                .map(|(ix, _)| ix)
            {
                self.tabs.remove(evict);
            }
        }
        self.tick += 1;
        self.tabs.push(FileTab {
            path,
            parent_dir: None,
            phase: Phase::Loading,
            scroll: VirtualListScrollHandle::new(),
            load_gen: self.tick,
            activated: self.tick,
        });
        let ix = self.tabs.len() - 1;
        self.active = ix;
        self.start_load(ix, cx);
        cx.notify();
    }

    /// Close tab `ix`, keeping `active` valid and the window's `Screen` in sync
    /// (a closed path must never stay the navigated screen, or the next screen
    /// sync would reopen it).
    fn close(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if ix >= self.tabs.len() {
            return;
        }
        let was_active = ix == self.active;
        self.tabs.remove(ix);

        if self.tabs.is_empty() {
            self.active = 0;
            // Leave the file viewer entirely (a file tab is always reached from
            // another screen, so the back stack has somewhere to land).
            navigation::go_back(window, cx);
            cx.notify();
            return;
        }
        if was_active {
            let new_ix = ix.min(self.tabs.len() - 1);
            self.active = new_ix;
            self.tick += 1;
            self.tabs[new_ix].activated = self.tick;
            let path = self.tabs[new_ix].path.clone();
            navigation::navigate(window, cx, Screen::FileViewer { path });
        } else if ix < self.active {
            self.active -= 1;
        }
        cx.notify();
    }

    /// Read + highlight tab `ix` off the foreground, swapping in the result on
    /// the UI thread (guarded against a superseded load).
    fn start_load(&mut self, ix: usize, cx: &mut gpui::Context<Self>) {
        let Some(path) = self.tabs.get(ix).map(|tab| tab.path.clone()) else {
            return;
        };
        let Some(root) = file_tree::window_trunk_root(self.window_id, cx) else {
            // The tree hasn't resolved a trunk root for this window — nothing to
            // read against (should not happen via the tree click path).
            if let Some(tab) = self.tabs.get_mut(ix) {
                tab.parent_dir = None;
                tab.phase = Phase::Idle;
            }
            cx.notify();
            return;
        };
        let abs = root.join(&path);
        self.tick += 1;
        let generation = self.tick;
        if let Some(tab) = self.tabs.get_mut(ix) {
            tab.parent_dir = abs.parent().map(Path::to_path_buf);
            tab.phase = Phase::Loading;
            tab.load_gen = generation;
        }
        let theme = cx.theme().highlight_theme.clone();
        cx.spawn(async move |this, cx| {
            let read_path = path.clone();
            let loaded = cx
                .background_executor()
                .spawn(async move { read_file(&abs, &read_path, &theme) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(tab) = this.tabs.iter_mut().find(|tab| tab.path == path) else {
                    return; // tab was closed
                };
                if tab.load_gen != generation {
                    return; // superseded by a newer load of the same path
                }
                tab.phase = match loaded {
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
    fn render_unviewable(
        &self,
        headline: &str,
        bytes: u64,
        dir: Option<PathBuf>,
        cx: &App,
    ) -> AnyElement {
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

    /// The header: a tab strip (filename + close ✕ per open file) plus the
    /// read-only marker. With no open file it degrades to a muted placeholder.
    fn render_header(&mut self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let border = cx.theme().border;
        let muted = cx.theme().muted;
        let muted_fg = cx.theme().muted_foreground;

        let bar = h_flex()
            .flex_shrink_0()
            .w_full()
            .h(px(32.))
            .items_center()
            .border_b_1()
            .border_color(border)
            .bg(muted.opacity(0.3));

        if self.tabs.is_empty() {
            return bar
                .gap_2()
                .px_3()
                .child(Icon::new(IconName::File).xsmall().text_color(muted_fg))
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(muted_fg)
                        .child("No file open"),
                )
                .child(div().text_xs().text_color(muted_fg).child("Read-only"))
                .into_any_element();
        }

        // Owned tab metadata so `cx.listener` (below) doesn't overlap the
        // `self.tabs` borrow while building the strip.
        let metas: Vec<(usize, SharedString)> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(ix, tab)| (ix, basename(&tab.path)))
            .collect();

        let tab_bar = TabBar::new("file-viewer-tabs")
            .with_size(ComponentSize::Small)
            .track_scroll(&self.tab_scroll)
            .selected_index(self.active)
            .on_click(cx.listener(|this, ix: &usize, window, cx| {
                if let Some(path) = this.tabs.get(*ix).map(|tab| tab.path.clone()) {
                    navigation::navigate(window, cx, Screen::FileViewer { path });
                }
            }))
            .children(metas.into_iter().map(|(ix, name)| {
                Tab::new().icon(Icon::new(IconName::File)).label(name).suffix(
                    Button::new(("file-viewer-close", ix))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.close(ix, window, cx);
                        })),
                )
            }));

        bar.child(div().flex_1().min_w_0().overflow_hidden().child(tab_bar))
            .child(
                div()
                    .flex_shrink_0()
                    .px_3()
                    .text_xs()
                    .text_color(muted_fg)
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
        let header = self.render_header(cx);

        let body: AnyElement = match self.tabs.get(self.active).map(|tab| &tab.phase) {
            None | Some(Phase::Idle) => {
                self.render_notice("Open a file from the Files panel.", cx)
            }
            Some(Phase::Loading) => self.render_notice("Loading…", cx),
            Some(Phase::Error(message)) => {
                let message = message.clone();
                self.render_notice(&format!("Couldn’t open file: {message}"), cx)
            }
            Some(Phase::TooLarge(bytes)) => {
                let bytes = *bytes;
                let dir = self.tabs[self.active].parent_dir.clone();
                self.render_unviewable("File is too large to preview", bytes, dir, cx)
            }
            Some(Phase::Binary(bytes)) => {
                let bytes = *bytes;
                let dir = self.tabs[self.active].parent_dir.clone();
                self.render_unviewable("Binary file", bytes, dir, cx)
            }
            Some(Phase::Ready { lines, .. }) if lines.is_empty() => {
                self.render_notice("Empty file.", cx)
            }
            Some(Phase::Ready { sizes, .. }) => {
                let sizes = sizes.clone();
                let scroll = self.tabs[self.active].scroll.clone();
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
                                let Some(tab) = this.tabs.get(this.active) else {
                                    return Vec::new();
                                };
                                let Phase::Ready { lines, .. } = &tab.phase else {
                                    return Vec::new();
                                };
                                visible_range
                                    .map(|ix| this.render_row(ix, lines, cx))
                                    .collect::<Vec<_>>()
                            },
                        )
                        .track_scroll(&scroll)
                        .py_1(),
                    )
                    .scrollbar(&scroll, ScrollbarAxis::Vertical)
                    .into_any_element()
            }
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(header)
            .child(div().flex_1().min_h_0().child(body))
    }
}

// ---------------------------------------------------------------------------
// Background read + highlight (pure, off the UI thread)
// ---------------------------------------------------------------------------

/// The tab label for a trunk-relative path (its final path segment).
fn basename(path: &str) -> SharedString {
    SharedString::from(path.rsplit('/').next().unwrap_or(path).to_string())
}

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
