//! Read-only file viewer (masterplan v4 §4.5): opens ONE trunk-relative file,
//! syntax highlighted and **selectable** (mouse selection + ctrl/cmd-C via the
//! shared `TextView` selection layer — the same machinery the issue
//! description uses). Binary/oversized (>2 MB) files show a placeholder with
//! size + "Open in terminal".
//!
//! Multi-file is the CENTER TAB STRIP's job now (`screens.rs` — one
//! `Screen::FileViewer { path }` tab per file), so this view is deliberately
//! single-file: the screens panel re-points it via [`set_path`] on tab
//! switches, exactly like the issue detail.
//!
//! Trunk-relative paths resolve against the per-window trunk root the file
//! tree published (`crate::file_tree::window_trunk_root`) — a file is only
//! reachable by clicking it in the tree, so the root is always resolved
//! first. The read runs on the background executor; rendering/highlighting
//! is the `TextView` code block's own (virtualized `gpui::list` in
//! `scrollable` mode).
//!
//! [`set_path`]: FileViewerView::set_path

use std::path::{Path, PathBuf};

use gpui::{
    div, AnyElement, App, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    SharedString, Styled, Window, WindowId,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    text::TextView,
    v_flex, ActiveTheme as _, IconName, Sizable as _,
};

use crate::file_tree::{self, OpenTerminalHere, MAX_VIEWER_BYTES};

/// The completed background read for a file — `Send` (built off the UI thread).
enum Loaded {
    /// A readable text file, already fenced as a markdown code block.
    Text(SharedString),
    /// Too large to view (>2 MB) — carries the byte size for the placeholder.
    TooLarge(u64),
    /// Binary content — carries the byte size for the placeholder.
    Binary(u64),
    /// The file could not be read (missing / permission).
    Error(String),
}

enum Phase {
    /// No file navigated yet (or no trunk root resolved).
    Idle,
    Loading,
    /// The fenced source, ready for the selectable `TextView`.
    Ready { source: SharedString },
    TooLarge(u64),
    Binary(u64),
    Error(SharedString),
}

/// The read-only trunk file viewer center screen. One instance per window,
/// re-pointed by the screens panel on tab switches.
pub struct FileViewerView {
    /// This window (for the trunk-root registry lookup — `set_path` has no
    /// `&Window`, so the id is captured at construction).
    window_id: WindowId,
    /// Trunk-relative path of the open file (`None` until first navigation).
    path: Option<String>,
    /// Absolute directory of the open file (the "Open terminal here" target).
    parent_dir: Option<PathBuf>,
    phase: Phase,
    /// Stale-load guard (bumped on each (re)load).
    load_gen: u64,
    focus_handle: FocusHandle,
}

impl FileViewerView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        Self {
            window_id: window.window_handle().window_id(),
            path: None,
            parent_dir: None,
            phase: Phase::Idle,
            load_gen: 0,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Point the viewer at a trunk-relative `path` — called from the screens
    /// panel on `Screen::FileViewer` navigation. Same path = no-op (tab
    /// re-activation must not re-read).
    pub fn set_path(&mut self, path: String, cx: &mut gpui::Context<Self>) {
        if path.is_empty() || self.path.as_deref() == Some(path.as_str()) {
            return;
        }
        self.path = Some(path);
        self.start_load(cx);
    }

    /// Read `self.path` off the foreground, swapping in the result on the UI
    /// thread (guarded against a superseded load).
    fn start_load(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let Some(root) = file_tree::window_trunk_root(self.window_id, cx) else {
            // The tree hasn't resolved a trunk root for this window — nothing
            // to read against (should not happen via the tree click path).
            self.parent_dir = None;
            self.phase = Phase::Idle;
            cx.notify();
            return;
        };
        let abs = root.join(&path);
        self.load_gen += 1;
        let generation = self.load_gen;
        self.parent_dir = abs.parent().map(Path::to_path_buf);
        self.phase = Phase::Loading;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let read_path = path.clone();
            let loaded = cx
                .background_executor()
                .spawn(async move { read_file(&abs, &read_path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.load_gen != generation || this.path.as_deref() != Some(path.as_str()) {
                    return; // superseded by a newer load
                }
                this.phase = match loaded {
                    Loaded::Text(source) => Phase::Ready { source },
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
                let message = message.clone();
                self.render_notice(&format!("Couldn’t open file: {message}"), cx)
            }
            Phase::TooLarge(bytes) => {
                self.render_unviewable(
                    "File is too large to preview",
                    *bytes,
                    self.parent_dir.clone(),
                    cx,
                )
            }
            Phase::Binary(bytes) => {
                self.render_unviewable("Binary file", *bytes, self.parent_dir.clone(), cx)
            }
            Phase::Ready { source } => {
                // Selectable + copyable through the shared TextView selection
                // layer; `scrollable` renders the code block virtualized
                // (gpui::list) so large files stay cheap.
                let id: SharedString =
                    format!("file-view-{}", self.path.as_deref().unwrap_or("")).into();
                div()
                    .size_full()
                    .child(
                        TextView::markdown(id, source.clone())
                            .selectable(true)
                            .scrollable(true),
                    )
                    .into_any_element()
            }
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .child(div().flex_1().min_h_0().child(body))
    }
}

// ---------------------------------------------------------------------------
// Background read (pure, off the UI thread)
// ---------------------------------------------------------------------------

/// Read `abs`, classify (oversized / binary / text), and fence text as a
/// markdown code block (language from the filename, the diff view's mapping)
/// so the `TextView` renders it highlighted.
fn read_file(abs: &std::path::Path, rel: &str) -> Loaded {
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
    let lang = crate::diff::language_for_filename(rel);
    Loaded::Text(SharedString::from(fence_code(&text, lang)))
}

/// Wrap `text` in a markdown code fence long enough that backtick runs inside
/// the file can never terminate it early.
fn fence_code(text: &str, lang: &str) -> String {
    let longest_run = text
        .split(|c| c != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    let fence = "`".repeat((longest_run + 1).max(3));
    let lang = if lang == "text" { "" } else { lang };
    format!("{fence}{lang}\n{text}\n{fence}")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fence_grows_past_embedded_backtick_runs() {
        let fenced = fence_code("let s = ```` four ticks ````;", "rust");
        assert!(fenced.starts_with("`````rust\n"), "{fenced}");
        assert!(fenced.ends_with("\n`````"));
    }

    #[test]
    fn fence_defaults_to_three_ticks_and_drops_the_text_lang() {
        let fenced = fence_code("plain contents", "text");
        assert_eq!(fenced, "```\nplain contents\n```");
    }
}
