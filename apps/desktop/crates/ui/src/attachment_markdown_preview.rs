//! EXP-1003 — the in-app preview for MARKDOWN attachments (web EXP-955,
//! `components/attachment-markdown-preview.tsx`): a `.md` row in the Files
//! rail, a comment's file chip or a Settings → Storage row opens the file in
//! a native dialog rendered by the read-only [`MarkdownView`] (the renderer
//! that draws descriptions and comments) instead of handing it to the OS.
//!
//! The rules mirror the web ×4: the title is the filename ("Preview"
//! without one), the subtitle `Markdown · <size>`; the size is checked TWICE
//! — a synced `size_bytes` above [`MARKDOWN_PREVIEW_MAX_BYTES`] shows the
//! download hint WITHOUT fetching, and a fetched body above it does too
//! (legacy rows carry `size_bytes = 0`). A 404 reads "no longer available",
//! any other status names it. The footer's Download is the Files rail's
//! "Save as…" flow ([`save_attachment_as`]).
//!
//! The phase logic is pure and gpui-free ([`phase_before_fetch`],
//! [`phase_from_fetch`], [`preview_subtitle`]); the view only fetches.

use std::sync::Arc;

use gpui::{
    div, px, size, App, AppContext as _, Entity, IntoElement,
    ParentElement as _, Render, ScrollHandle, SharedString, Styled as _, Subscription, Task,
    Window,
};
use gpui_component::{
    button::Button,
    h_flex,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use domain::rows::Attachment;

use crate::comment_attachments::save_attachment_as;
use crate::controls::WebControl as _;
use crate::icons::registry;
use crate::issue_files::{format_bytes, MARKDOWN_PREVIEW_MAX_BYTES};
use crate::markdown::{
    AttachmentFetchStatus, AttachmentTransport, ImageCache, MarkdownView, RefResolver,
};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

/// Web copy, byte for byte.
const TOO_LARGE_COPY: &str = "This file is too large to preview here. Download it to read it.";
const NOT_FOUND_COPY: &str = "This file is no longer available.";
const LOAD_FAILED_COPY: &str = "Couldn't load this file.";

/// Where the preview is in its one fetch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PreviewPhase {
    Loading,
    /// The decoded markdown source.
    Ready(String),
    TooLarge,
    /// The user-facing message.
    Error(String),
}

/// The pre-fetch size check: a synced size over the ceiling goes straight to
/// the download hint (no request); `None` = fetch.
pub(crate) fn phase_before_fetch(size_bytes: i64) -> Option<PreviewPhase> {
    (size_bytes > MARKDOWN_PREVIEW_MAX_BYTES as i64).then_some(PreviewPhase::TooLarge)
}

/// The post-fetch phase: the body's own size (legacy rows have
/// `size_bytes = 0`), lossy UTF-8, and the web's error copy.
pub(crate) fn phase_from_fetch(result: anyhow::Result<Vec<u8>>) -> PreviewPhase {
    match result {
        // Decode first: the web measures `text.length` (UTF-16 units), so a
        // non-ASCII file is judged by its characters, not its bytes (×4).
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes).into_owned();
            if text.encode_utf16().count() > MARKDOWN_PREVIEW_MAX_BYTES {
                PreviewPhase::TooLarge
            } else {
                PreviewPhase::Ready(text)
            }
        }
        Err(error) => PreviewPhase::Error(match error.downcast_ref::<AttachmentFetchStatus>() {
            Some(AttachmentFetchStatus(404)) => NOT_FOUND_COPY.to_string(),
            Some(AttachmentFetchStatus(status)) => {
                format!("Couldn't load this file (HTTP {status}).")
            }
            None => {
                let message = error.to_string();
                if message.trim().is_empty() {
                    LOAD_FAILED_COPY.to_string()
                } else {
                    message
                }
            }
        }),
    }
}

/// `Markdown · 12.3 KB`, or `Markdown` without a known size.
pub(crate) fn preview_subtitle(size_bytes: i64) -> String {
    if size_bytes > 0 {
        format!("Markdown · {}", format_bytes(size_bytes))
    } else {
        "Markdown".to_string()
    }
}

/// What the preview needs of one attachment — built from a synced
/// [`Attachment`] or a Storage row.
#[derive(Clone, Debug)]
pub(crate) struct MarkdownPreviewTarget {
    pub attachment_id: String,
    /// The stored filename (may be blank — see [`Self::title`]).
    pub filename: String,
    /// `0` when unknown (legacy rows).
    pub size_bytes: i64,
    /// Resolves `#IDENT`/`@email` pills in the rendered file.
    pub team_id: Option<String>,
}

impl MarkdownPreviewTarget {
    pub(crate) fn from_attachment(attachment: &Attachment) -> Self {
        Self {
            attachment_id: attachment.id.clone(),
            filename: attachment.filename.clone().unwrap_or_default(),
            size_bytes: attachment.size_bytes.unwrap_or(0),
            team_id: attachment.team_id.clone(),
        }
    }

    /// The dialog title: the filename, else "Preview" (web parity).
    fn title(&self) -> String {
        let name = self.filename.trim();
        if name.is_empty() { "Preview" } else { name }.to_string()
    }

    /// The Save-as suggestion: the filename, else the rail's `file` label.
    fn save_label(&self) -> String {
        let name = self.filename.trim();
        if name.is_empty() { "file" } else { name }.to_string()
    }
}

/// Open the preview dialog (native chrome, resizable) for `target`.
pub(crate) fn open_markdown_preview(
    target: MarkdownPreviewTarget,
    window: &mut Window,
    cx: &mut App,
) {
    let viewport = window.viewport_size();
    let spec = DialogSpec::new(
        target.title(),
        size(
            px(760.).min(viewport.width * 0.9),
            px(720.).min(viewport.height * 0.85),
        ),
    )
    .resizable(size(px(420.), px(280.)));
    native_dialog::open_dialog_window(window, cx, spec, move |_window, cx| {
        DialogContent::new(cx.new(|cx| MarkdownPreview::new(target, cx))).self_scrolling()
    });
}

/// The dialog body: pinned subtitle, the scrolling rendered file, a pinned
/// Download footer.
struct MarkdownPreview {
    target: MarkdownPreviewTarget,
    phase: PreviewPhase,
    images: Entity<ImageCache>,
    scroll: ScrollHandle,
    _images_sub: Subscription,
    _load: Option<Task<()>>,
}

impl MarkdownPreview {
    fn new(target: MarkdownPreviewTarget, cx: &mut gpui::Context<Self>) -> Self {
        let transport: Option<Arc<dyn AttachmentTransport>> = queries::attachment_transport(cx);
        let images = cx.new(|_| ImageCache::new(transport.clone()));
        let images_sub = cx.observe(&images, |_, _, cx| cx.notify());
        let mut phase = phase_before_fetch(target.size_bytes).unwrap_or(PreviewPhase::Loading);
        let mut load = None;
        if phase == PreviewPhase::Loading {
            match transport {
                Some(transport) => {
                    let url = format!("/api/attachments/{}", target.attachment_id);
                    load = Some(cx.spawn(async move |this, cx| {
                        let result = cx
                            .background_executor()
                            .spawn(async move { transport.fetch(&url) })
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            this.phase = phase_from_fetch(result);
                            cx.notify();
                        });
                    }));
                }
                None => phase = PreviewPhase::Error(LOAD_FAILED_COPY.to_string()),
            }
        }
        Self {
            target,
            phase,
            images,
            scroll: ScrollHandle::new(),
            _images_sub: images_sub,
            _load: load,
        }
    }

    fn render_body(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        match &self.phase {
            PreviewPhase::Loading => h_flex()
                .gap_1p5()
                .py_6()
                .items_center()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(Spinner::new().xsmall().icon(registry::UI_LOADING))
                .child("Loading...")
                .into_any_element(),
            PreviewPhase::TooLarge => div()
                .py_6()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(TOO_LARGE_COPY)
                .into_any_element(),
            PreviewPhase::Error(message) => div()
                .py_6()
                .text_sm()
                .text_color(cx.theme().danger)
                .child(SharedString::from(message.clone()))
                .into_any_element(),
            PreviewPhase::Ready(source) => {
                let mut view = MarkdownView::new(
                    SharedString::from(format!("md-preview-{}", self.target.attachment_id)),
                    source.clone(),
                )
                .selectable(true)
                .images(self.images.clone());
                if let Some(team_id) = self.target.team_id.clone() {
                    view = view
                        .resolver(RefResolver::from_store(team_id.clone()))
                        .on_open_issue(move |identifier, window, cx| {
                            let (team, identifier) = (team_id.clone(), identifier.to_string());
                            native_dialog::close_then(window, cx, move |window, cx| {
                                crate::description_editor::open_issue_by_identifier(
                                    &team,
                                    &identifier,
                                    window,
                                    cx,
                                );
                            });
                        });
                }
                div().w_full().min_w_0().text_sm().child(view).into_any_element()
            }
        }
    }
}

impl Render for MarkdownPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let (attachment_id, label) = (self.target.attachment_id.clone(), self.target.save_label());
        v_flex()
            .size_full()
            .gap_3()
            .child(
                div()
                    .flex_shrink_0()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(preview_subtitle(self.target.size_bytes))),
            )
            // `v_scroll_pane`, not `overflow_y_scrollbar`: the latter drops the
            // `flex_1`/`min_h_0` this body needs to stop above the footer
            // (EXP-67/771).
            .child(crate::scroll_pane::v_scroll_pane(
                "md-preview-scroll",
                &self.scroll,
                v_flex()
                    .w_full()
                    .min_w_0()
                    .pr_3()
                    .child(self.render_body(cx)),
            ))
            .child(
                h_flex().flex_shrink_0().justify_end().gap_2().child(
                    Button::new("md-preview-download")
                        .outline()
                        .cursor_pointer()
                        .web_sm()
                        .icon(Icon::from(registry::UI_DOWNLOAD))
                        .label("Download")
                        .on_click(move |_, window, cx| {
                            save_attachment_as(attachment_id.clone(), label.clone(), window, cx);
                        }),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_precheck_skips_the_fetch_only_above_the_ceiling() {
        let max = MARKDOWN_PREVIEW_MAX_BYTES as i64;
        assert_eq!(phase_before_fetch(max + 1), Some(PreviewPhase::TooLarge));
        assert_eq!(phase_before_fetch(max), None);
        assert_eq!(phase_before_fetch(0), None);
        assert_eq!(phase_before_fetch(-1), None);
    }

    #[test]
    fn a_fetched_body_is_checked_again_and_decoded_lossily() {
        assert_eq!(
            phase_from_fetch(Ok(b"# Hi".to_vec())),
            PreviewPhase::Ready("# Hi".to_string())
        );
        let at_max = vec![b'a'; MARKDOWN_PREVIEW_MAX_BYTES];
        assert!(matches!(phase_from_fetch(Ok(at_max)), PreviewPhase::Ready(_)));
        let over = vec![b'a'; MARKDOWN_PREVIEW_MAX_BYTES + 1];
        assert_eq!(phase_from_fetch(Ok(over)), PreviewPhase::TooLarge);
        assert_eq!(
            phase_from_fetch(Ok(vec![b'a', 0xff])),
            PreviewPhase::Ready("a\u{fffd}".to_string())
        );
        // Two-byte characters: MAX of them = 2·MAX bytes but MAX UTF-16 units,
        // so they render (the web measures `text.length`); one more tips over.
        let wide = "ä".repeat(MARKDOWN_PREVIEW_MAX_BYTES).into_bytes();
        assert!(matches!(phase_from_fetch(Ok(wide)), PreviewPhase::Ready(_)));
        let wide_over = "ä".repeat(MARKDOWN_PREVIEW_MAX_BYTES + 1).into_bytes();
        assert_eq!(phase_from_fetch(Ok(wide_over)), PreviewPhase::TooLarge);
    }

    #[test]
    fn fetch_errors_read_the_web_copy() {
        assert_eq!(
            phase_from_fetch(Err(AttachmentFetchStatus(404).into())),
            PreviewPhase::Error("This file is no longer available.".to_string())
        );
        assert_eq!(
            phase_from_fetch(Err(AttachmentFetchStatus(500).into())),
            PreviewPhase::Error("Couldn't load this file (HTTP 500).".to_string())
        );
        assert_eq!(
            phase_from_fetch(Err(anyhow::anyhow!("connection refused"))),
            PreviewPhase::Error("connection refused".to_string())
        );
    }

    #[test]
    fn the_subtitle_names_the_size_when_known() {
        assert_eq!(preview_subtitle(0), "Markdown");
        assert_eq!(preview_subtitle(-1), "Markdown");
        assert_eq!(preview_subtitle(2048), "Markdown · 2.0 KB");
    }

    #[test]
    fn title_and_save_label_fall_back() {
        let mut target = MarkdownPreviewTarget {
            attachment_id: "att-1".into(),
            filename: " notes.md ".into(),
            size_bytes: 0,
            team_id: None,
        };
        assert_eq!(target.title(), "notes.md");
        assert_eq!(target.save_label(), "notes.md");
        target.filename = "  ".into();
        assert_eq!(target.title(), "Preview");
        assert_eq!(target.save_label(), "file");
    }
}
