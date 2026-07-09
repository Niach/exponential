//! In-app image lightbox (EXP-33): clicking an attachment chip or an inline
//! description/comment image opens this modal preview — never the web
//! browser. Built on the shared gpui-component dialog layer (same overlay
//! machinery as the duplicate picker), so the dark scrim, Esc-close,
//! click-outside-close and the ✕ button all come from the one modal pattern
//! the app already uses. A small "Open in browser" affordance inside the
//! preview keeps the old behavior reachable for URLs that resolve against
//! the instance base.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, img, px, App, AppContext as _, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, StyledImage as _, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
};

use crate::icons::ExpIcon;
use crate::markdown::{placeholder_box, ImageCache, ImageSlot};
use crate::queries;

/// Open the lightbox for one image. `url` is the image's canonical (usually
/// relative `/api/attachments/{id}`) form — the same key the [`ImageCache`]
/// fetches by. Pass the owning surface's cache when it has one (editor /
/// rendered view) so already-decoded bytes show instantly; `None` builds a
/// fresh cache over the active account's attachment transport (chips).
pub(crate) fn open_image_preview(
    url: String,
    label: String,
    images: Option<Entity<ImageCache>>,
    window: &mut Window,
    cx: &mut App,
) {
    let images = match images {
        Some(images) => images,
        None => {
            let transport = queries::attachment_transport(cx);
            cx.new(|_| ImageCache::new(transport))
        }
    };
    let open_url = queries::absolute_api_url(cx, &url);
    let label = preview_label(&label, &url);
    let preview = cx.new(|cx| ImagePreview::new(url, label, open_url, images, cx));

    // Mostly-viewport lightbox: the dialog is horizontally centered by the
    // layer; the image scales down inside (natural aspect, never upscaled).
    let width = (window.viewport_size().width * 0.8).min(px(1100.));
    window.open_dialog(cx, move |dialog, _, _| {
        let preview = preview.clone();
        dialog
            .w(width)
            .margin_top(px(48.))
            .content(move |content, _, _| content.child(preview.clone()))
    });
}

/// Chip/alt label → filename fallback → generic.
fn preview_label(label: &str, url: &str) -> String {
    let trimmed = label.trim();
    if !trimmed.is_empty() && trimmed != "image" {
        return trimmed.to_string();
    }
    let filename = url
        .rsplit('/')
        .next()
        .and_then(|segment| segment.split('?').next())
        .unwrap_or_default();
    if !filename.is_empty() {
        filename.to_string()
    } else {
        "Image".to_string()
    }
}

struct ImagePreview {
    url: String,
    label: SharedString,
    open_url: Option<String>,
    images: Entity<ImageCache>,
    /// Re-render when the cache resolves the async fetch.
    _images_changed: Subscription,
}

impl ImagePreview {
    fn new(
        url: String,
        label: String,
        open_url: Option<String>,
        images: Entity<ImageCache>,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let images_changed = cx.observe(&images, |_, _, cx| cx.notify());
        Self {
            url,
            label: SharedString::from(label),
            open_url,
            images,
            _images_changed: images_changed,
        }
    }
}

impl Render for ImagePreview {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let url = self.url.clone();
        let slot = self.images.update(cx, |cache, cx| cache.slot(&url, cx));
        let max_h = window.viewport_size().height * 0.72;

        let body = match slot {
            ImageSlot::Ready(image) => img(image)
                .max_w_full()
                .max_h(max_h)
                .object_fit(gpui::ObjectFit::ScaleDown)
                .rounded(px(6.))
                .into_any_element(),
            ImageSlot::Loading => placeholder_box("Loading image…", cx),
            ImageSlot::Failed(_) => placeholder_box("Image unavailable", cx),
        };

        v_flex()
            .w_full()
            .gap_2()
            .child(div().w_full().flex().justify_center().child(body))
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(self.label.clone()),
                    )
                    .when_some(self.open_url.clone(), |el, open_url| {
                        el.child(
                            Button::new("image-preview-open-browser")
                                .ghost()
                                .xsmall()
                                .icon(
                                    Icon::from(ExpIcon::ArrowUpRight)
                                        .text_color(cx.theme().muted_foreground),
                                )
                                .label("Open in browser")
                                .on_click(move |_, _, _| {
                                    if let Err(error) = api::opener::open_in_browser(&open_url) {
                                        log::warn!(
                                            "[ui] image preview: open in browser failed: {error}"
                                        );
                                    }
                                }),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_labels_fall_back_alt_then_filename_then_generic() {
        assert_eq!(preview_label(" shot ", "/api/attachments/a.png"), "shot");
        // The editor's synthetic "image" alt is not a real label.
        assert_eq!(
            preview_label("image", "/api/attachments/a-photo.png?w=1"),
            "a-photo.png"
        );
        assert_eq!(preview_label("", "/api/attachments/xyz"), "xyz");
        assert_eq!(preview_label("", ""), "Image");
    }
}
