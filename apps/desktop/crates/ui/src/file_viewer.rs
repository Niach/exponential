//! Read-only file viewer (masterplan v4 §4.5): opens ONE trunk-relative file,
//! syntax highlighted and **selectable** (mouse selection + ctrl/cmd-C via the
//! shared `TextView` selection layer — the same machinery the issue
//! description uses). Binary/oversized (>2 MB) files show a placeholder with
//! size + "Open file" (system default app) / "Show in files" / "Open in
//! terminal" (EXP-473 — the app ships no viewers of its own, so PDFs and
//! friends are handed to the OS).
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
    h_flex,
    text::TextView,
    v_flex, ActiveTheme as _, Sizable as _,
};

use crate::file_tree::{self, OpenTerminalHere, RevealInFileManager, MAX_VIEWER_BYTES};
use crate::icons::registry;

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
    /// Absolute path of the open file (the "Open file" / "Show in files"
    /// targets on the unviewable placeholder).
    abs_path: Option<PathBuf>,
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
            abs_path: None,
            phase: Phase::Idle,
            load_gen: 0,
            focus_handle: cx.focus_handle(),
        }
    }

    /// Point the viewer at a trunk-relative `path` — called from the screens
    /// panel when the Files tree selection changes (EXP-288). Same path =
    /// no-op (re-activation must not re-read).
    pub fn set_path(&mut self, path: String, cx: &mut gpui::Context<Self>) {
        if path.is_empty() || self.path.as_deref() == Some(path.as_str()) {
            return;
        }
        self.path = Some(path);
        self.start_load(cx);
    }

    /// Drop the open file (EXP-288 — the selection was cleared by a
    /// board/team switch); back to the Idle "open a file" notice.
    pub fn clear(&mut self, cx: &mut gpui::Context<Self>) {
        if self.path.is_none() {
            return;
        }
        self.path = None;
        self.parent_dir = None;
        self.abs_path = None;
        self.load_gen += 1; // supersede any in-flight load
        self.phase = Phase::Idle;
        cx.notify();
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
            self.abs_path = None;
            self.phase = Phase::Idle;
            cx.notify();
            return;
        };
        let abs = root.join(&path);
        self.load_gen += 1;
        let generation = self.load_gen;
        self.parent_dir = abs.parent().map(Path::to_path_buf);
        self.abs_path = Some(abs.clone());
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

    /// Binary / oversized placeholder (§4.5, EXP-473): the human-readable
    /// size + OS hand-offs — "Open file" (system default app), "Show in
    /// files" (reveal in the file manager) and "Open in terminal" (the `+`
    /// shell tab at the file's directory).
    fn render_unviewable(
        &self,
        headline: &str,
        bytes: u64,
        dir: Option<PathBuf>,
        cx: &App,
    ) -> AnyElement {
        let open_target = self.abs_path.clone();
        let reveal_target = self.abs_path.clone();
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
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("file-viewer-open-file")
                            .icon(registry::UI_EXTERNAL_LINK)
                            .label("Open file")
                            .ghost()
                            .small()
                            .on_click(move |_, _window, cx| {
                                if let Some(file) = &open_target {
                                    cx.open_with_system(file);
                                }
                            }),
                    )
                    .child(
                        Button::new("file-viewer-show-in-files")
                            .icon(registry::UI_FOLDER)
                            .label("Show in files")
                            .ghost()
                            .small()
                            .on_click(move |_, window, cx| {
                                if let Some(file) = &reveal_target {
                                    window.dispatch_action(
                                        Box::new(RevealInFileManager {
                                            path: file.to_string_lossy().into_owned(),
                                        }),
                                        cx,
                                    );
                                }
                            }),
                    )
                    .child(
                        Button::new("file-viewer-open-terminal")
                            .icon(registry::NAV_TERMINAL)
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
                    ),
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
                // EXP-282: the code block's own chrome (opaque `tokens.muted`
                // panel, radius, `p_3`) is stripped — the file sits directly
                // on the page gradient. The block's padding moves to this
                // wrapper so the code keeps its inset off the pane edges.
                div()
                    .size_full()
                    .px_3()
                    .py_2()
                    .child(
                        TextView::markdown(id, source.clone())
                            .selectable(true)
                            .scrollable(true)
                            .style(crate::surface::bare_code_markdown_style()),
                    )
                    .into_any_element()
            }
        };

        // EXP-269: no opaque fill — code floats on the page gradient (the
        // syntax palette reads on both gradient endpoints).
        v_flex()
            .size_full()
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
    if looks_binary(rel, &bytes) {
        return Loaded::Binary(size);
    }

    let text = String::from_utf8_lossy(&bytes);
    let lang = crate::diff::language_for_filename(rel);
    Loaded::Text(SharedString::from(fence_code(&text, lang)))
}

/// Extensions that are always binary — the NUL/UTF-8 probes below miss
/// ASCII-heavy formats (a linearized PDF's first 8 KB is header + xref
/// tables), and rendering megabytes of such "text" as ONE code block is a
/// freeze/OOM (`TextView` virtualizes top-level blocks, and a file is exactly
/// one). EXP-473.
const BINARY_EXTENSIONS: &[&str] = &[
    "pdf", // documents
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "icns", "tiff", "heic", // images
    "zip", "gz", "bz2", "xz", "zst", "7z", "tar", "jar", // archives
    "exe", "dll", "so", "dylib", "a", "o", "wasm", "pyc", "class", "bin", // executables/objects
    "woff", "woff2", "ttf", "otf", "eot", // fonts
    "mp3", "mp4", "mov", "avi", "mkv", "webm", "wav", "ogg", "flac", // media
    "sqlite", "db", // databases
];

/// Classify `bytes` (already size-capped) as unviewable binary: a known
/// binary extension, a `%PDF` magic (mis-named PDFs — the reported crash), a
/// NUL byte in the first 8 KB (the classic editor signal), or invalid UTF-8
/// in that window (anything else would render as U+FFFD soup).
fn looks_binary(rel: &str, bytes: &[u8]) -> bool {
    let ext = Path::new(rel)
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase());
    if ext.is_some_and(|ext| BINARY_EXTENSIONS.contains(&ext.as_str())) {
        return true;
    }
    if bytes.starts_with(b"%PDF") {
        return true;
    }
    let probe = &bytes[..bytes.len().min(8192)];
    if probe.contains(&0) {
        return true;
    }
    // `error_len() == None` means the probe merely cut a multi-byte char at
    // its edge — that is not a binary signal.
    std::str::from_utf8(probe)
        .err()
        .is_some_and(|err| err.error_len().is_some())
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

    #[test]
    fn ascii_only_pdf_is_binary() {
        // A linearized PDF's first 8 KB can be pure ASCII — the NUL probe
        // alone misses it (the EXP-473 crash).
        let ascii_pdf = b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\nendobj\n".to_vec();
        assert!(looks_binary("docs/report.pdf", &ascii_pdf));
        // Magic alone catches a mis-named PDF too.
        assert!(looks_binary("docs/report.txt", &ascii_pdf));
    }

    #[test]
    fn binary_extension_wins_regardless_of_content() {
        assert!(looks_binary("logo.PNG", b"plain ascii"));
        assert!(looks_binary("vendor/lib.so", b"plain ascii"));
    }

    #[test]
    fn text_sources_are_not_binary() {
        assert!(!looks_binary("src/main.rs", "fn main() {} // ünïcode".as_bytes()));
        assert!(!looks_binary("README", b"no extension at all"));
    }

    #[test]
    fn nul_byte_is_binary() {
        assert!(looks_binary("data.dat", b"ascii\0more"));
    }

    #[test]
    fn invalid_utf8_is_binary_but_a_probe_cut_char_is_not() {
        assert!(looks_binary("data.dat", &[b'a', 0xFF, 0xFE, b'b']));
        // 8 KB of ASCII, then a 3-byte char ("€") cut by the probe edge —
        // `error_len() == None`, not a binary signal.
        let mut cut = vec![b'a'; 8190];
        cut.extend_from_slice(&[0xE2, 0x82]);
        assert!(!looks_binary("notes.txt", &cut));
    }
}
