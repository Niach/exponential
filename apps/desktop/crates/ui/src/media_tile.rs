//! EXP-824 inline media tiles — the desktop's rendering of a video/audio
//! attachment wherever one appears: a standalone attachment link in rendered
//! markdown ([`crate::markdown::MarkdownView`]), the comment attachment strip
//! and the lightbox.
//!
//! The cross-client contract: the markdown form is a PLAIN link on its own
//! paragraph (`[clip.mp4](/api/attachments/{id})`, optionally `?w=480`),
//! upgraded to a tile ONLY when the referenced synced `attachments` row has a
//! `video/*` or `audio/*` content type. A video tile shows the poster frame
//! (`/api/attachments/{id}?poster=1`, fetched through the same auth-gated
//! [`ImageCache`] as any image) or a neutral box, a centered play glyph and a
//! duration chip; an audio tile is a card with the play glyph, the filename
//! and the duration.
//!
//! Desktop v1 decodes NO media in-process (no GStreamer, no wry): a click
//! downloads the bytes through the bearer-auth transport into the app's
//! `{data_dir}/media-cache` ([`crate::issue_files::fetch_media_to_cache`]:
//! one file per attachment, reused on re-open, pruned by age and size) and
//! hands the PATH to the system player — never the bare URL, which the OS
//! could not authenticate. The tile shows a busy state while the download
//! runs.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use gpui::{
    div, img, prelude::FluentBuilder as _, px, App, ElementId, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Window,
};
use gpui_component::{
    h_flex,
    menu::{ContextMenuExt as _, PopupMenuItem},
    notification::Notification,
    spinner::Spinner,
    v_flex, ActiveTheme as _, Icon, Sizable as _, WindowExt as _,
};

use domain::rows::Attachment;

use crate::icons::{registry, ExpIcon};
use crate::issue_files::{
    attachment_label, fetch_media_to_cache, format_duration, is_inline_audio, is_inline_video,
};
use crate::markdown::{image_url, ImageCache, ImageSlot};
use crate::queries;

/// The video tile's height cap — the same 480px the comment image tiles use
/// (web `max-h-[480px]`, iOS/Android 480).
pub(crate) const VIDEO_TILE_MAX_H: f32 = 480.;

/// The box a video tile takes when the row carries no probed dimensions: a
/// 16:9 default rather than a full-width band.
pub(crate) const VIDEO_TILE_DEFAULT_W: f32 = 480.;
pub(crate) const VIDEO_TILE_DEFAULT_H: f32 = 270.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MediaKind {
    Video,
    Audio,
}

/// Everything a media tile renders, derived from one synced attachment row
/// (plus the link URL's `?w=` display hint).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MediaTile {
    pub attachment_id: String,
    pub kind: MediaKind,
    /// The row's filename (the link label is display-only; the row wins so
    /// a renamed attachment reads the same everywhere).
    pub label: String,
    pub duration_ms: Option<i64>,
    /// `/api/attachments/{id}?poster=1` when the row holds a poster frame.
    pub poster_url: Option<String>,
    /// Probed `(width, height)` of the video (None for audio / unprobed).
    pub natural: Option<(f32, f32)>,
    /// The link's `?w=` display width, if any.
    pub display_width: Option<f32>,
}

impl MediaTile {
    /// `Some` iff the row is inline media. `display_width` starts unset —
    /// [`Self::with_display_width_from`] reads it off a markdown URL.
    pub(crate) fn from_attachment(row: &Attachment) -> Option<Self> {
        let content_type = row.content_type.as_deref();
        let kind = if is_inline_video(content_type) {
            MediaKind::Video
        } else if is_inline_audio(content_type) {
            MediaKind::Audio
        } else {
            return None;
        };
        let natural = match (kind, row.width, row.height) {
            (MediaKind::Video, Some(width), Some(height)) if width > 0 && height > 0 => {
                Some((width as f32, height as f32))
            }
            _ => None,
        };
        Some(Self {
            attachment_id: row.id.clone(),
            kind,
            label: attachment_label(row),
            duration_ms: row.duration_ms.filter(|ms| *ms >= 0),
            poster_url: row
                .poster_storage_key
                .as_deref()
                .filter(|key| !key.trim().is_empty())
                .map(|_| poster_url(&row.id)),
            natural,
            display_width: None,
        })
    }

    /// The tile for a markdown link URL: `Some` only when the URL names a
    /// SYNCED media row (an unsynced or non-media row renders as a link).
    pub(crate) fn for_url(url: &str, cx: &App) -> Option<Self> {
        let id = image_url::attachment_id_from_src(url)?;
        let row = sync::Store::try_global(cx)?
            .collections()
            .attachments
            .read(cx)
            .get(id)
            .cloned()?;
        Some(Self::from_attachment(&row)?.with_display_width_from(url))
    }

    pub(crate) fn with_display_width_from(mut self, url: &str) -> Self {
        self.display_width = image_url::width_param_from_src(url);
        self
    }

    /// The video tile's `(width, height)`: the probed size scaled down to
    /// the height cap (never up), a 16:9 default without one; a `?w=`
    /// display width overrides the width and keeps the aspect.
    pub(crate) fn video_box(&self) -> (f32, f32) {
        video_tile_box(self.natural, self.display_width)
    }

    pub(crate) fn duration_chip(&self) -> Option<String> {
        self.duration_ms.map(format_duration)
    }
}

/// The poster-frame URL of an attachment (same bearer auth as the bytes).
pub(crate) fn poster_url(attachment_id: &str) -> String {
    format!("/api/attachments/{attachment_id}?poster=1")
}

/// Pure box arithmetic behind [`MediaTile::video_box`] (testable without a
/// gpui App).
pub(crate) fn video_tile_box(natural: Option<(f32, f32)>, display_width: Option<f32>) -> (f32, f32) {
    let (width, height) = match natural {
        Some((width, height)) if width > 0. && height > 0. => {
            if height <= VIDEO_TILE_MAX_H {
                (width, height)
            } else {
                let scale = VIDEO_TILE_MAX_H / height;
                (width * scale, VIDEO_TILE_MAX_H)
            }
        }
        _ => (VIDEO_TILE_DEFAULT_W, VIDEO_TILE_DEFAULT_H),
    };
    match display_width {
        Some(target) if target > 0. && width > 0. => {
            let scale = target / width;
            (target, height * scale)
        }
        _ => (width, height),
    }
}

// ---------------------------------------------------------------------------
// Markdown surgery
// ---------------------------------------------------------------------------

/// Is `line` (trimmed) exactly one plain link `[label](src)` to `src`?
fn is_standalone_link_to(line: &str, src: &str) -> bool {
    let line = line.trim();
    let Some(rest) = line.strip_prefix('[') else {
        return false;
    };
    let Some(label_end) = rest.find("](") else {
        return false;
    };
    let label = &rest[..label_end];
    let target = &rest[label_end + 2..];
    !label.is_empty() && !label.contains(']') && target.strip_suffix(')') == Some(src)
}

/// Remove the standalone link paragraph whose destination is exactly `src`
/// (the "Remove from description" of a media tile) — the line goes, and the
/// blank line that separated it from a neighbour goes with it so no double
/// blank (or leading/trailing blank) is left behind. `None` when no such
/// paragraph exists; text-level so nothing else in the document is
/// re-serialized.
pub(crate) fn remove_media_link_paragraph(markdown: &str, src: &str) -> Option<String> {
    let lines: Vec<&str> = markdown.split('\n').collect();
    let index = lines
        .iter()
        .position(|line| is_standalone_link_to(line, src))?;
    let mut head: Vec<&str> = lines[..index].to_vec();
    let mut tail: &[&str] = &lines[index + 1..];
    let blank = |line: &&str| line.trim().is_empty();
    if head.last().is_some_and(blank) && tail.first().is_some_and(blank) {
        tail = &tail[1..];
    } else if head.is_empty() {
        while tail.first().is_some_and(blank) {
            tail = &tail[1..];
        }
    } else if tail.is_empty() {
        while head.last().is_some_and(blank) {
            head.pop();
        }
    }
    head.extend_from_slice(tail);
    Some(head.join("\n"))
}

// ---------------------------------------------------------------------------
// Open in the system player
// ---------------------------------------------------------------------------

fn opening() -> &'static Mutex<HashSet<String>> {
    static OPENING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    OPENING.get_or_init(Default::default)
}

/// Is a system-player open (temp download) in flight for this attachment?
pub(crate) fn is_opening(attachment_id: &str) -> bool {
    opening()
        .lock()
        .map(|set| set.contains(attachment_id))
        .unwrap_or(false)
}

/// Materialize the attachment in the media cache (a re-open reuses the
/// cached clip) and hand the PATH to the OS player. The tile renders busy
/// until the download settles; a failure surfaces as a window notification.
pub(crate) fn open_media_in_player(
    attachment_id: String,
    label: String,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(transport) = queries::attachment_transport(cx) else {
        return;
    };
    if is_opening(&attachment_id) {
        return;
    }
    if let Ok(mut set) = opening().lock() {
        set.insert(attachment_id.clone());
    }
    window.refresh();
    let handle = window.window_handle();
    let data_dir = crate::coding_flow::coding_data_dir(cx);
    cx.spawn(async move |cx| {
        let fetch_id = attachment_id.clone();
        let fetch_label = label.clone();
        let result = cx
            .background_executor()
            .spawn(async move {
                fetch_media_to_cache(transport.as_ref(), &data_dir, &fetch_id, &fetch_label)
            })
            .await;
        if let Ok(mut set) = opening().lock() {
            set.remove(&attachment_id);
        }
        match result {
            // Open on the APP, not the originating window: a lightbox that
            // was closed while the download ran must still hand the file
            // to the player.
            Ok(path) => {
                let _ = cx.update(|cx| cx.open_with_system(&path));
            }
            Err(error) => {
                log::warn!("[ui] media open failed for {attachment_id}: {error}");
                let _ = handle.update(cx, |_, window, cx| {
                    window.push_notification(
                        Notification::error(SharedString::from(format!(
                            "Could not open {label}: {error}"
                        ))),
                        cx,
                    );
                });
            }
        }
        let _ = handle.update(cx, |_, window, _| window.refresh());
    })
    .detach();
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// The centered play glyph (or the busy spinner while a download runs) on a
/// translucent disc — the one overlay every media surface shares.
pub(crate) fn play_badge(diameter: f32, busy: bool) -> gpui::AnyElement {
    let glyph_size = (diameter * 0.46).round();
    div()
        .size(px(diameter))
        .rounded_full()
        .bg(gpui::black().opacity(0.6))
        .border_1()
        .border_color(gpui::white().opacity(0.25))
        .flex()
        .items_center()
        .justify_center()
        .child(if busy {
            Spinner::new()
                .icon(registry::UI_LOADING)
                .with_size(px(glyph_size))
                .color(gpui::white())
                .into_any_element()
        } else {
            Icon::from(ExpIcon::Play)
                .with_size(px(glyph_size))
                .text_color(gpui::white())
                .into_any_element()
        })
        .into_any_element()
}

/// The duration chip (bottom-right of a video tile).
fn duration_chip(text: String) -> gpui::AnyElement {
    div()
        .absolute()
        .bottom_2()
        .right_2()
        .px_1p5()
        .py_0p5()
        .rounded(px(4.))
        .bg(gpui::black().opacity(0.65))
        .text_xs()
        .text_color(gpui::white())
        .child(SharedString::from(text))
        .into_any_element()
}

/// One media tile. `images` fetches the poster through the shared cache
/// (`None` = no poster rendering, neutral box only). Click opens the system
/// player; right-click offers Preview · Open in player · Open in browser.
pub(crate) fn render_media_tile(
    id: impl Into<ElementId>,
    tile: &MediaTile,
    images: Option<&Entity<ImageCache>>,
    cx: &mut App,
) -> gpui::AnyElement {
    let busy = is_opening(&tile.attachment_id);
    let body = match tile.kind {
        MediaKind::Video => {
            let (width, height) = tile.video_box();
            let poster = match (&tile.poster_url, images) {
                (Some(url), Some(images)) => {
                    Some(images.update(cx, |cache, cx| cache.slot(url, cx)))
                }
                _ => None,
            };
            div()
                .id(id.into())
                .relative()
                .flex_shrink_0()
                .w(px(width))
                .h(px(height))
                .max_w_full()
                .rounded(px(theme::tokens::radius::LG))
                .overflow_hidden()
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .bg(theme::tokens::glass::FILL_SECTION.to_hsla())
                .cursor_pointer()
                .when_some(poster, |el, slot| match slot {
                    ImageSlot::Ready(image) => el.child(
                        img(image)
                            .size_full()
                            .object_fit(gpui::ObjectFit::Cover),
                    ),
                    _ => el,
                })
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(play_badge(56., busy)),
                )
                .children(tile.duration_chip().map(duration_chip))
        }
        MediaKind::Audio => {
            let muted = cx.theme().muted_foreground;
            h_flex()
                .id(id.into())
                .w_full()
                .max_w(px(VIDEO_TILE_DEFAULT_W))
                .gap_3()
                .px_3()
                .py_2()
                .rounded(px(theme::tokens::radius::LG))
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .bg(theme::tokens::glass::FILL_CARD.to_hsla())
                .cursor_pointer()
                .child(play_badge(40., busy))
                .child(
                    v_flex()
                        .min_w_0()
                        .flex_1()
                        .gap_0p5()
                        .child(
                            div()
                                .text_sm()
                                .whitespace_nowrap()
                                .overflow_hidden()
                                .text_ellipsis()
                                .child(SharedString::from(tile.label.clone())),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(muted)
                                .child(SharedString::from(
                                    tile.duration_chip().unwrap_or_else(|| "Audio".to_string()),
                                )),
                        ),
                )
        }
    };

    let open_id = tile.attachment_id.clone();
    let open_label = tile.label.clone();
    let clickable = body.on_click(move |_, window, cx| {
        // Inside a blurred-editor preview a bubbling click would also start
        // editing behind the player.
        cx.stop_propagation();
        open_media_in_player(open_id.clone(), open_label.clone(), window, cx);
    });

    let menu_tile = tile.clone();
    let menu_images = images.cloned();
    clickable
        .context_menu(move |menu, _window, _cx| {
            let mut menu = menu.item(
                PopupMenuItem::new("Open in player")
                    .icon(Icon::from(ExpIcon::Play))
                    .on_click({
                        let id = menu_tile.attachment_id.clone();
                        let label = menu_tile.label.clone();
                        move |_, window, cx| {
                            open_media_in_player(id.clone(), label.clone(), window, cx);
                        }
                    }),
            );
            menu = menu.item(
                PopupMenuItem::new("Preview")
                    .icon(Icon::from(registry::UI_WATCH))
                    .on_click({
                        let tile = menu_tile.clone();
                        let images = menu_images.clone();
                        move |_, window, cx| {
                            crate::image_preview::open_media_preview(
                                tile.clone(),
                                images.clone(),
                                window,
                                cx,
                            );
                        }
                    }),
            );
            menu = menu.item(
                PopupMenuItem::new("Open in browser")
                    .icon(Icon::from(ExpIcon::ArrowUpRight))
                    .on_click({
                        let id = menu_tile.attachment_id.clone();
                        move |_, _, cx| {
                            let Some(url) =
                                queries::absolute_api_url(cx, &format!("/api/attachments/{id}"))
                            else {
                                return;
                            };
                            if let Err(error) = api::opener::open_in_browser(&url) {
                                log::warn!("[ui] media open in browser failed: {error}");
                            }
                        }
                    }),
            );
            if let Some(images) = menu_images.clone() {
                menu = menu.item(
                    PopupMenuItem::new("Download")
                        .icon(Icon::from(registry::UI_DOWNLOAD))
                        .on_click({
                            let id = menu_tile.attachment_id.clone();
                            move |_, window, cx| {
                                crate::markdown::download_image(
                                    format!("/api/attachments/{id}"),
                                    &images,
                                    window,
                                    cx,
                                );
                            }
                        }),
                );
            }
            menu
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(content_type: &str, extra: serde_json::Value) -> Attachment {
        let mut value = json!({
            "id": "att-1",
            "issue_id": "i-1",
            "filename": "clip.mp4",
            "content_type": content_type,
        });
        if let (Some(base), Some(extra)) = (value.as_object_mut(), extra.as_object()) {
            for (key, val) in extra {
                base.insert(key.clone(), val.clone());
            }
        }
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn only_media_rows_become_tiles() {
        assert!(MediaTile::from_attachment(&row("image/png", json!({}))).is_none());
        assert!(MediaTile::from_attachment(&row("application/pdf", json!({}))).is_none());
        let video = MediaTile::from_attachment(&row("video/mp4", json!({}))).unwrap();
        assert_eq!(video.kind, MediaKind::Video);
        assert_eq!(video.label, "clip.mp4");
        assert_eq!(video.poster_url, None);
        assert_eq!(video.natural, None);
        let audio = MediaTile::from_attachment(&row("audio/mpeg", json!({}))).unwrap();
        assert_eq!(audio.kind, MediaKind::Audio);
    }

    #[test]
    fn a_probed_video_row_carries_poster_size_and_duration() {
        let tile = MediaTile::from_attachment(&row(
            "video/quicktime",
            json!({
                "width": 1920,
                "height": 1080,
                "duration_ms": 154000,
                "poster_storage_key": "posters/att-1.jpg",
            }),
        ))
        .unwrap();
        assert_eq!(tile.poster_url.as_deref(), Some("/api/attachments/att-1?poster=1"));
        assert_eq!(tile.natural, Some((1920., 1080.)));
        assert_eq!(tile.duration_chip().as_deref(), Some("2:34"));
        // A blank poster key is no poster.
        let blank = MediaTile::from_attachment(&row(
            "video/mp4",
            json!({ "poster_storage_key": "  " }),
        ))
        .unwrap();
        assert_eq!(blank.poster_url, None);
        // Audio never carries a natural size even if the row has dims.
        let audio = MediaTile::from_attachment(&row(
            "audio/wav",
            json!({ "width": 10, "height": 10 }),
        ))
        .unwrap();
        assert_eq!(audio.natural, None);
    }

    #[test]
    fn the_display_width_comes_off_the_link_url() {
        let tile = MediaTile::from_attachment(&row("video/mp4", json!({})))
            .unwrap()
            .with_display_width_from("/api/attachments/att-1?w=320");
        assert_eq!(tile.display_width, Some(320.));
        assert_eq!(tile.video_box(), (320., 180.));
    }

    /// The tile menu's "Remove from description": exactly the link paragraph
    /// goes, and no blank residue is left in any position.
    #[test]
    fn removing_a_media_link_paragraph_leaves_no_blank_residue() {
        let src = "/api/attachments/abc";
        assert_eq!(
            remove_media_link_paragraph("before\n\n[clip.mp4](/api/attachments/abc)\n\nafter", src)
                .as_deref(),
            Some("before\n\nafter")
        );
        assert_eq!(
            remove_media_link_paragraph("[clip.mp4](/api/attachments/abc)\n\nafter", src).as_deref(),
            Some("after")
        );
        assert_eq!(
            remove_media_link_paragraph("before\n\n[clip.mp4](/api/attachments/abc)", src).as_deref(),
            Some("before")
        );
        assert_eq!(
            remove_media_link_paragraph("[clip.mp4](/api/attachments/abc)", src).as_deref(),
            Some("")
        );
        // The `?w=` form is a different destination string — the caller
        // passes the runtime's exact src.
        assert_eq!(
            remove_media_link_paragraph("[clip.mp4](/api/attachments/abc?w=320)", "/api/attachments/abc?w=320")
                .as_deref(),
            Some("")
        );
        // Not a standalone link to that src: untouched.
        assert_eq!(remove_media_link_paragraph("see [clip.mp4](/api/attachments/abc)", src), None);
        assert_eq!(remove_media_link_paragraph("![clip](/api/attachments/abc)", src), None);
        assert_eq!(remove_media_link_paragraph("[clip.mp4](/api/attachments/other)", src), None);
    }

    #[test]
    fn video_boxes_scale_down_to_the_cap_and_default_to_16_9() {
        assert_eq!(video_tile_box(None, None), (480., 270.));
        assert_eq!(video_tile_box(Some((640., 360.)), None), (640., 360.));
        assert_eq!(video_tile_box(Some((1080., 1920.)), None), (270., 480.));
        assert_eq!(video_tile_box(Some((0., 100.)), None), (480., 270.));
        // A `?w=` keeps the aspect ratio.
        assert_eq!(video_tile_box(Some((1920., 1080.)), Some(960.)), (960., 540.));
        assert_eq!(video_tile_box(None, Some(240.)), (240., 135.));
    }
}
