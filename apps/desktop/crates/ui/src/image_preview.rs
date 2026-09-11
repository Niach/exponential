//! In-app image lightbox (EXP-33): clicking an attachment chip or an inline
//! description/comment image opens this modal preview — never the web
//! browser. Built on the shared native dialog window (EXP-284 — same modal
//! shape as the duplicate picker), so Esc-close and the dismiss semantics
//! come from the one dialog pattern the app uses. EXP-426: the window sizes
//! itself to the image's aspect ratio when the natural dimensions are known,
//! the in-window title is gone (the filename stays the OS window title), and
//! "Open in browser" sits in the header next to the ✕. EXP-824 adds the
//! media variant ([`open_media_preview`]): the poster (or a neutral box)
//! behind a big "Open in player" button — the desktop decodes no media, so
//! the preview is a launcher for the system player, not a player.

use gpui::{
    div, img, px, size, App, AppContext as _, Entity, IntoElement, ParentElement, Pixels, Render,
    SharedString, Size, Styled, StyledImage as _, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    ActiveTheme as _, Disableable as _, Icon,
};

use crate::controls::WebControl as _;
use crate::icons::ExpIcon;
use crate::markdown::{attachment_natural_size, placeholder_box, ImageCache, ImageSlot};
use crate::media_tile::{
    self, MediaKind, MediaTile, VIDEO_TILE_DEFAULT_H, VIDEO_TILE_DEFAULT_W,
};
use crate::native_dialog::{self, DialogContent, DialogSpec};
use crate::queries;

/// The chromeless header row's height share of the window (px_4/pt_3 padding
/// plus the xsmall button line) — [`preview_window_size`] reserves it so the
/// image itself gets the aspect-true remainder.
const PREVIEW_HEADER_H: f32 = 44.;

/// Open the lightbox for one image. `url` is the image's canonical (usually
/// relative `/api/attachments/{id}`) form — the same key the [`ImageCache`]
/// fetches by. Pass the owning surface's cache when it has one (editor /
/// rendered view) so already-decoded bytes show instantly; `None` builds a
/// fresh cache over the active account's attachment transport (chips).
/// `natural_size` is the image's probed pixel size when the caller knows it
/// (attachment rows, the WYSIWYG layout probe); attachment URLs fall back to
/// the synced probe, anything else to a generic viewport-share window.
pub(crate) fn open_image_preview(
    url: String,
    label: String,
    natural_size: Option<(f32, f32)>,
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

    let natural = natural_size.or_else(|| attachment_natural_size(&url, cx));
    let window_size = preview_window_size(natural, window.viewport_size());
    // EXP-285: chromeless — traffic lights over the image corner read as
    // dirt; the header ✕ stays the dismissal. The label still names the OS
    // window even though the in-content header shows no title.
    let spec = DialogSpec::new(label, window_size).chromeless();
    native_dialog::open_dialog_window(window, cx, spec, move |_, cx| {
        let preview = cx.new(|cx| ImagePreview::new(url, images, cx));
        // The header's ✕ is the only mouse dismissal a lightbox has (there is
        // no Cancel footer) — without it the modal window is a dead end.
        let mut content = DialogContent::new(preview).chromeless_header("").padless();
        if let Some(open_url) = open_url {
            content = content.chromeless_header_actions(move |_, cx| {
                open_in_browser_button("image-preview-open-browser", open_url.clone(), cx)
            });
        }
        content
    });
}

/// The header's "Open in browser" affordance, shared by both lightboxes.
fn open_in_browser_button(id: &'static str, open_url: String, cx: &App) -> gpui::AnyElement {
    Button::new(id)
        .ghost()
        .web_xs()
        .icon(Icon::from(ExpIcon::ArrowUpRight).text_color(cx.theme().muted_foreground))
        .label("Open in browser")
        .on_click(move |_, _, _| {
            if let Err(error) = api::opener::open_in_browser(&open_url) {
                log::warn!("[ui] preview: open in browser failed: {error}");
            }
        })
        .into_any_element()
}

/// EXP-824: the lightbox for a video/audio attachment. Sized like an image
/// preview from the poster's box (16:9 default); the body is the poster or
/// a neutral box with a big "Open in player" button that runs the same
/// temp-download-and-open path as the tile click. "Open in browser" stays
/// in the header.
pub(crate) fn open_media_preview(
    tile: MediaTile,
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
    let open_url = queries::absolute_api_url(
        cx,
        &format!("/api/attachments/{}", tile.attachment_id),
    );
    let natural = match tile.kind {
        MediaKind::Video => Some(tile.video_box()),
        MediaKind::Audio => Some((VIDEO_TILE_DEFAULT_W, VIDEO_TILE_DEFAULT_H)),
    };
    let window_size = preview_window_size(natural, window.viewport_size());
    let spec = DialogSpec::new(tile.label.clone(), window_size).chromeless();
    native_dialog::open_dialog_window(window, cx, spec, move |_, cx| {
        let preview = cx.new(|cx| MediaPreview::new(tile, images, cx));
        let mut content = DialogContent::new(preview).chromeless_header("").padless();
        if let Some(open_url) = open_url {
            content = content.chromeless_header_actions(move |_, cx| {
                open_in_browser_button("media-preview-open-browser", open_url.clone(), cx)
            });
        }
        content
    });
}

struct MediaPreview {
    tile: MediaTile,
    images: Entity<ImageCache>,
    _images_changed: Subscription,
}

impl MediaPreview {
    fn new(tile: MediaTile, images: Entity<ImageCache>, cx: &mut gpui::Context<Self>) -> Self {
        let images_changed = cx.observe(&images, |_, _, cx| cx.notify());
        Self {
            tile,
            images,
            _images_changed: images_changed,
        }
    }
}

impl Render for MediaPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let busy = media_tile::is_opening(&self.tile.attachment_id);
        let poster = self
            .tile
            .poster_url
            .clone()
            .map(|url| self.images.update(cx, |cache, cx| cache.slot(&url, cx)));
        let backdrop = match poster {
            Some(ImageSlot::Ready(image)) => img(image)
                .max_w_full()
                .max_h_full()
                .object_fit(gpui::ObjectFit::ScaleDown)
                .rounded(cx.theme().radius)
                .into_any_element(),
            _ => div()
                .size_full()
                .rounded(cx.theme().radius)
                .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
                .into_any_element(),
        };
        let attachment_id = self.tile.attachment_id.clone();
        let label = self.tile.label.clone();
        let is_audio = self.tile.kind == MediaKind::Audio;
        let caption = self.tile.duration_chip();

        div()
            .size_full()
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .p_2()
            .child(backdrop)
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(media_tile::play_badge(72., busy))
                    .child(
                        Button::new("media-preview-open-player")
                            .primary()
                            .icon(Icon::from(ExpIcon::Play))
                            .label(if busy { "Opening…" } else { "Open in player" })
                            .disabled(busy)
                            .on_click(move |_, window, cx| {
                                media_tile::open_media_in_player(
                                    attachment_id.clone(),
                                    label.clone(),
                                    window,
                                    cx,
                                );
                            }),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(SharedString::from(match (is_audio, caption) {
                                (true, Some(duration)) => {
                                    format!("{} · {duration}", self.tile.label)
                                }
                                (true, None) => self.tile.label.clone(),
                                (false, Some(duration)) => duration,
                                (false, None) => String::new(),
                            })),
                    ),
            )
    }
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

/// The lightbox window's initial size. With known natural dimensions the
/// window matches the IMAGE's aspect ratio (plus the header strip): the image
/// scales to fit 90% of the viewport, never upscales, and the window floors
/// at 480×320 so the header controls always fit. Without dimensions, the old
/// generic viewport share.
fn preview_window_size(natural: Option<(f32, f32)>, viewport: Size<Pixels>) -> Size<Pixels> {
    let (vw, vh) = (f32::from(viewport.width), f32::from(viewport.height));
    match natural {
        Some((width, height)) if width > 0. && height > 0. => {
            let scale = (vw * 0.9 / width)
                .min((vh * 0.9 - PREVIEW_HEADER_H) / height)
                .min(1.0);
            size(
                px((width * scale).max(480.)),
                px((height * scale + PREVIEW_HEADER_H).max(320.)),
            )
        }
        _ => size(px((vw * 0.8).min(1100.)), px(vh * 0.8)),
    }
}

struct ImagePreview {
    url: String,
    images: Entity<ImageCache>,
    /// Re-render when the cache resolves the async fetch.
    _images_changed: Subscription,
}

impl ImagePreview {
    fn new(url: String, images: Entity<ImageCache>, cx: &mut gpui::Context<Self>) -> Self {
        let images_changed = cx.observe(&images, |_, _, cx| cx.notify());
        Self {
            url,
            images,
            _images_changed: images_changed,
        }
    }
}

impl Render for ImagePreview {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let url = self.url.clone();
        let slot = self.images.update(cx, |cache, cx| cache.slot(&url, cx));

        let body = match slot {
            ImageSlot::Ready(image) => img(image)
                .max_w_full()
                .max_h_full()
                .object_fit(gpui::ObjectFit::ScaleDown)
                .rounded(cx.theme().radius)
                .into_any_element(),
            ImageSlot::Loading => placeholder_box("Loading image…", cx),
            ImageSlot::Failed(_) => placeholder_box("Image unavailable", cx),
        };

        // One centered fill — the window already matches the image's aspect
        // ratio, so the image reads near-fullscreen with a slim margin.
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p_2()
            .child(body)
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

    #[test]
    fn preview_window_matches_image_aspect_within_viewport() {
        let viewport = size(px(2000.), px(1000.));
        // Landscape image fits the height budget: 900*0.9-44=856 tall.
        let sized = preview_window_size(Some((1600., 1200.)), viewport);
        let scale: f32 = (1000. * 0.9 - PREVIEW_HEADER_H) / 1200.;
        assert_eq!(f32::from(sized.width), 1600. * scale);
        assert_eq!(f32::from(sized.height), 1200. * scale + PREVIEW_HEADER_H);
    }

    #[test]
    fn preview_window_never_upscales_and_floors() {
        let viewport = size(px(2000.), px(1000.));
        // A small image keeps scale 1 and floors to the 480×320 minimum.
        let sized = preview_window_size(Some((200., 100.)), viewport);
        assert_eq!(f32::from(sized.width), 480.);
        assert_eq!(f32::from(sized.height), 320.);
    }

    #[test]
    fn preview_window_without_dims_keeps_viewport_share() {
        let viewport = size(px(2000.), px(1000.));
        let sized = preview_window_size(None, viewport);
        assert_eq!(f32::from(sized.width), 1100.);
        assert_eq!(f32::from(sized.height), 800.);
    }
}
