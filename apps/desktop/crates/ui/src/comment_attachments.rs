//! EXP-554 comment attachments: the read-side strip under a comment body, the
//! composer's pending strip, and the upload leg that turns picked paths into
//! `attachments` rows.
//!
//! Web parity target: `apps/web/src/components/comment-rows/attachments.tsx` +
//! `comment-composer.tsx`. The cross-client rules:
//!
//! - Attachments are NEVER inlined into the comment markdown — a comment's
//!   rows carry `attachments.comment_id` and render as a strip below the body:
//!   inline-image types as squared 64px center-cropped thumbs (click → the
//!   in-app lightbox), everything else as a file chip (click → fetch + hand to
//!   the OS), exactly like the Files rail's rows.
//! - Upload happens ON SEND, sequentially, and each item stamps its
//!   `uploaded_id` so a retry after a mid-batch failure never re-uploads.
//! - At most [`MAX_COMMENT_ATTACHMENTS`] per comment (the server enforces the
//!   same cap).
//!
//! State (the pending vectors, the edit-mode removals) lives on
//! [`crate::timeline::IssueTimeline`]; this module is the rendering + upload
//! half, mirroring the `timeline.rs` / `comments.rs` split.

use gpui::{
    div, img, px, App, ElementId, Entity, InteractiveElement as _, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement as _, Styled, StyledImage as _, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    notification::Notification,
    v_flex, ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _,
};
use sync::Store;

use domain::rows::Attachment;

use crate::controls::WebControl as _;
use crate::icons::{registry, ExpIcon};
use crate::issue_files::{
    attachment_label, fetch_attachment_to_temp, format_bytes, icon_for_content_type,
    is_inline_image,
};
use crate::markdown::{placeholder_box, AttachmentTransport, ImageCache, ImageSlot};
use crate::queries;
use crate::timeline::{IssueTimeline, PendingCommentAttachment, PendingScope};

/// Server cap (`MAX_COMMENT_ATTACHMENTS` in `packages/db-schema/src/domain.ts`)
/// — the picker refuses extra files instead of letting the mutation fail.
pub(crate) const MAX_COMMENT_ATTACHMENTS: usize = 10;

/// The squared thumb edge (web `size-16`, iOS/Android 64pt/dp).
const THUMB: f32 = 64.;

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

/// One comment's synced attachment rows, oldest first. `created_at` is an
/// ISO-8601 UTC string, so lexicographic order is chronological; the id
/// breaks ties so the strip never reshuffles between frames (same ordering
/// rule as [`crate::issue_files::file_attachments`]).
pub(crate) fn comment_attachments(comment_id: &str, cx: &App) -> Vec<Attachment> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let mut rows: Vec<Attachment> = store
        .collections()
        .attachments
        .read(cx)
        .iter()
        .filter(|attachment| attachment.comment_id.as_deref() == Some(comment_id))
        .cloned()
        .collect();
    rows.sort_by(|a, b| {
        a.created_at
            .as_deref()
            .unwrap_or_default()
            .cmp(b.created_at.as_deref().unwrap_or_default())
            .then_with(|| a.id.cmp(&b.id))
    });
    rows
}

// ---------------------------------------------------------------------------
// Uploads (background executor only — blocking network)
// ---------------------------------------------------------------------------

/// Upload every pending item that has no `uploaded_id` yet, in order.
/// Returns `(key, attachment id)` for every item that IS uploaded (including
/// the ones that already were) plus the first error message, if any — the
/// caller stamps the ids back onto its pending rows so a retry resumes where
/// this left off instead of re-uploading.
///
/// BLOCKING. Everything posts to the one route `/api/issues/{id}/files`;
/// the server applies the cap of the content type (10 MB for the five
/// inline-image types, 50 MB for everything else).
pub(crate) fn upload_pending_attachments(
    transport: &dyn AttachmentTransport,
    issue_id: &str,
    pending: &[PendingCommentAttachment],
) -> (Vec<(u64, String)>, Option<String>) {
    let mut uploaded = Vec::with_capacity(pending.len());
    for item in pending {
        if let Some(id) = item.uploaded_id.clone() {
            uploaded.push((item.key, id));
            continue;
        }
        let result = crate::markdown::read_any_file(&item.path).and_then(|(
            filename,
            content_type,
            bytes,
        )| transport.upload(issue_id, &filename, &content_type, &bytes));
        match result {
            Ok(row) => uploaded.push((item.key, row.id)),
            Err(error) => return (uploaded, Some(error.to_string())),
        }
    }
    (uploaded, None)
}

// ---------------------------------------------------------------------------
// Read-side strip
// ---------------------------------------------------------------------------

/// The strip under a comment body. `removals` is `Some` only in edit mode:
/// the ids in it are already staged for removal (hidden), and every remaining
/// tile grows the ✕ that stages it.
pub(crate) fn comment_attachments_strip(
    comment_id: &str,
    images: &Entity<ImageCache>,
    removals: Option<&std::collections::HashSet<String>>,
    cx: &mut gpui::Context<IssueTimeline>,
) -> Option<gpui::AnyElement> {
    let rows: Vec<Attachment> = comment_attachments(comment_id, cx)
        .into_iter()
        .filter(|row| removals.is_none_or(|staged| !staged.contains(&row.id)))
        .collect();
    // `None` (not an empty element) so callers can `.children(…)` it without
    // a flex `gap` opening a hole where the strip isn't.
    if rows.is_empty() {
        return None;
    }
    let removable = removals.is_some();

    let mut strip = h_flex().w_full().mt_1p5().gap_2().flex_wrap().items_start();
    for row in &rows {
        let remove = removable.then(|| {
            let id = row.id.clone();
            cx.listener(
                move |this: &mut IssueTimeline, _: &gpui::ClickEvent, _window: &mut Window, cx| {
                    this.stage_attachment_removal(id.clone(), cx);
                },
            )
        });
        strip = if is_inline_image(row.content_type.as_deref()) {
            strip.child(image_tile(row, images, remove, cx))
        } else {
            strip.child(file_chip(row, remove, cx))
        };
    }
    Some(strip.into_any_element())
}

/// One squared center-cropped thumb over the shared [`ImageCache`]; click
/// opens the in-app lightbox at full size (never the browser). Loading and
/// failed slots render the shared neutral placeholder, clipped to the tile.
fn image_tile(
    attachment: &Attachment,
    images: &Entity<ImageCache>,
    remove: Option<impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static>,
    cx: &mut App,
) -> gpui::AnyElement {
    // The stored URL is the canonical relative form; a row that predates it
    // still resolves by id.
    let url = attachment
        .url
        .clone()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| format!("/api/attachments/{}", attachment.id));
    let fetch_url = crate::markdown::image_url::strip_query(&url).to_string();
    let label = attachment_label(attachment);
    let natural = match (attachment.width, attachment.height) {
        (Some(width), Some(height)) if width > 0 && height > 0 => {
            Some((width as f32, height as f32))
        }
        _ => None,
    };
    let slot = images.update(cx, |cache, cx| cache.slot(&fetch_url, cx));
    let body = match slot {
        ImageSlot::Ready(image) => img(image)
            .size_full()
            .object_fit(gpui::ObjectFit::Cover)
            .into_any_element(),
        // A label would only clip inside a 64px tile — the neutral box IS the
        // loading/unavailable state here.
        _ => placeholder_box("", cx),
    };

    let tile = div()
        .id(SharedString::from(format!(
            "comment-attachment-image-{}",
            attachment.id
        )))
        .size(px(THUMB))
        .flex_shrink_0()
        .rounded_md()
        .overflow_hidden()
        .border_1()
        .border_color(cx.theme().border)
        .cursor_pointer()
        .child(body)
        .on_click({
            let images = images.clone();
            let url = fetch_url.clone();
            let label = label.clone();
            move |_, window, cx| {
                crate::image_preview::open_image_preview(
                    url.clone(),
                    label.clone(),
                    natural,
                    Some(images.clone()),
                    window,
                    cx,
                );
            }
        });

    match remove {
        // The ✕ overlays the tile's top-right corner (web's
        // `-right-1.5 -top-1.5`); `stop_propagation` keeps a remove from also
        // opening the lightbox.
        Some(on_remove) => div()
            .relative()
            .flex_shrink_0()
            .child(tile)
            .child(
                Button::new(SharedString::from(format!(
                    "comment-attachment-remove-{}",
                    attachment.id
                )))
                .ghost()
                .web_icon_xs()
                .icon(Icon::new(registry::UI_CLOSE).xsmall())
                .absolute()
                .top(px(-4.))
                .right(px(-4.))
                .on_click(move |event, window, cx| {
                    cx.stop_propagation();
                    on_remove(event, window, cx);
                }),
            )
            .into_any_element(),
        None => tile.into_any_element(),
    }
}

/// One non-image attachment chip: type glyph · filename · size (+ the edit
/// mode ✕). Clicking fetches the bytes through the auth-gated transport into
/// a temp file and hands the path to the OS — the desktop ships no viewers of
/// its own (EXP-297).
fn file_chip(
    attachment: &Attachment,
    remove: Option<impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static>,
    cx: &App,
) -> gpui::AnyElement {
    let label = attachment_label(attachment);
    let attachment_id = attachment.id.clone();
    let mut chip = h_flex()
        .id(SharedString::from(format!(
            "comment-attachment-file-{}",
            attachment.id
        )))
        .flex_shrink_0()
        .gap_1p5()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .items_center()
        .cursor_pointer()
        .hover(|el| el.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla()))
        .child(
            Icon::from(icon_for_content_type(attachment.content_type.as_deref()))
                .xsmall()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .max_w(px(160.))
                .text_xs()
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(SharedString::from(label.clone())),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(SharedString::from(format_bytes(
                    attachment.size_bytes.unwrap_or_default(),
                ))),
        )
        .on_click({
            let label = label.clone();
            move |_, window, cx| {
                open_attachment_file(attachment_id.clone(), label.clone(), window, cx);
            }
        });

    if let Some(on_remove) = remove {
        chip = chip.child(
            Button::new(SharedString::from(format!(
                "comment-attachment-remove-{}",
                attachment.id
            )))
            .ghost()
            .web_icon_xs()
            .icon(
                Icon::new(registry::UI_CLOSE)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .on_click(move |event, window, cx| {
                cx.stop_propagation();
                on_remove(event, window, cx);
            }),
        );
    }
    chip.into_any_element()
}

/// Fetch one attachment into its temp path and open it with the OS. Failures
/// surface as a window notification (same wording shape as the Files rail).
pub(crate) fn open_attachment_file(
    attachment_id: String,
    label: String,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(transport) = queries::attachment_transport(cx) else {
        return;
    };
    let handle = window.window_handle();
    cx.spawn(async move |cx| {
        let fetch_id = attachment_id.clone();
        let fetch_label = label.clone();
        let result = cx
            .background_executor()
            .spawn(async move {
                fetch_attachment_to_temp(transport.as_ref(), &fetch_id, &fetch_label)
            })
            .await;
        let _ = handle.update(cx, |_, window, cx| match result {
            Ok(path) => cx.open_with_system(&path),
            Err(error) => {
                log::warn!("[ui] comment attachment open failed for {attachment_id}: {error}");
                window.push_notification(
                    Notification::error(SharedString::from(format!(
                        "Could not open {label}: {error}"
                    ))),
                    cx,
                );
            }
        });
    })
    .detach();
}

// ---------------------------------------------------------------------------
// Composer strip (pending, not yet uploaded)
// ---------------------------------------------------------------------------

/// The strip of picked-but-unsent items above the composer / edit actions.
///
/// Desktop v1 renders pendings as CHIPS (type glyph · filename · error), not
/// as thumbs: decoding an arbitrary local pick into a gpui image before the
/// upload would mean routing it through the `draft://` staging machinery the
/// description editor owns, which buys one frame of preview at a
/// disproportionate cost. The moment the comment posts, the synced rows
/// render as real 64px thumbs via [`comment_attachments_strip`].
pub(crate) fn pending_attachments_strip(
    pending: &[PendingCommentAttachment],
    scope: PendingScope,
    cx: &mut gpui::Context<IssueTimeline>,
) -> Option<gpui::AnyElement> {
    if pending.is_empty() {
        return None;
    }
    let mut strip = h_flex().w_full().gap_2().flex_wrap().items_start();
    for item in pending {
        strip = strip.child(pending_chip(item, scope, cx));
    }
    Some(strip.into_any_element())
}

fn pending_chip(
    item: &PendingCommentAttachment,
    scope: PendingScope,
    cx: &mut gpui::Context<IssueTimeline>,
) -> gpui::AnyElement {
    let glyph = if item.is_image {
        ExpIcon::Image
    } else {
        icon_for_content_type(Some(item.content_type.as_str()))
    };
    let key = item.key;
    let mut body = v_flex().gap_0p5().child(
        div()
            .max_w(px(160.))
            .text_xs()
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .child(SharedString::from(item.filename.clone())),
    );
    if let Some(error) = item.error.clone() {
        body = body.child(
            div()
                .max_w(px(160.))
                .text_xs()
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .text_color(cx.theme().danger)
                .child(error),
        );
    }

    h_flex()
        .id(SharedString::from(format!(
            "comment-pending-attachment-{}-{key}",
            scope.id_prefix()
        )))
        .flex_shrink_0()
        .gap_1p5()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(theme::tokens::glass::FILL_CARD.to_hsla())
        .items_center()
        .child(
            Icon::from(glyph)
                .xsmall()
                .text_color(cx.theme().muted_foreground),
        )
        .child(body)
        .child(
            Button::new(SharedString::from(format!(
                "comment-pending-remove-{}-{key}",
                scope.id_prefix()
            )))
            .ghost()
            .web_icon_xs()
            .icon(
                Icon::new(registry::UI_CLOSE)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .on_click(cx.listener(
                move |this: &mut IssueTimeline, _: &gpui::ClickEvent, _window: &mut Window, cx| {
                    this.remove_pending_attachment(scope, key, cx);
                },
            )),
        )
        .into_any_element()
}

/// The `+`/paperclip ghost button both composers grow (EXP-554). Disabled at
/// the cap so the click can never produce a rejected mutation.
pub(crate) fn attach_button(
    id: impl Into<ElementId>,
    scope: PendingScope,
    disabled: bool,
    cx: &mut gpui::Context<IssueTimeline>,
) -> Button {
    Button::new(id)
        .ghost()
        .web_icon_sm()
        .icon(Icon::new(registry::UI_ATTACH).text_color(cx.theme().muted_foreground))
        .tooltip("Attach files")
        .disabled(disabled)
        .on_click(cx.listener(
            move |this: &mut IssueTimeline, _: &gpui::ClickEvent, window: &mut Window, cx| {
                this.pick_comment_attachments(scope, window, cx);
            },
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn attachment(id: &str, comment_id: Option<&str>, created_at: &str) -> Attachment {
        serde_json::from_value(json!({
            "id": id,
            "issue_id": "i-1",
            "comment_id": comment_id,
            "created_at": created_at,
        }))
        .unwrap()
    }

    /// The strip's ordering rule, exercised without a gpui App: oldest first,
    /// ties broken by id.
    #[test]
    fn rows_sort_oldest_first_with_an_id_tiebreak() {
        let mut rows = vec![
            attachment("b", Some("c-1"), "2026-07-03T10:00:00Z"),
            attachment("a", Some("c-1"), "2026-07-03T10:00:00Z"),
            attachment("c", Some("c-1"), "2026-07-01T10:00:00Z"),
        ];
        rows.sort_by(|a, b| {
            a.created_at
                .as_deref()
                .unwrap_or_default()
                .cmp(b.created_at.as_deref().unwrap_or_default())
                .then_with(|| a.id.cmp(&b.id))
        });
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["c", "a", "b"]
        );
    }

    /// The cap the picker enforces is the server's.
    #[test]
    fn the_cap_matches_the_server() {
        assert_eq!(MAX_COMMENT_ATTACHMENTS, 10);
    }
}
