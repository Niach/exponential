//! EXP-879 — the RESULTS face: the pictures a coding run published while it
//! worked, read off `coding_sessions.results`.
//!
//! ONE scrolling page, byte-for-byte the web/iOS/Android shape: per topic a
//! group band ([`crate::surface::glass_section_band`], the EXP-818 list
//! design) over a WRAPPING row of EQUAL-HEIGHT tiles — every tile
//! [`SESSION_RESULT_TILE_HEIGHT`] tall, its width from the probed aspect
//! ([`session_result_tile_width`], 4:3 without one) — each with a muted
//! one-line caption under it. A shot of the same screen on web, iOS and
//! Android therefore reads side by side on one baseline.
//!
//! The image is the ordinary member-gated attachment route
//! (`/api/attachments/{id}`) fetched through the owning view's shared
//! [`ImageCache`], exactly like a comment's attachment; a click opens the
//! in-app lightbox at full size, never the browser.
//!
//! This face carries NO Stop/Resume (the Run face owns it) and NO merge bar
//! (Changes owns it) — it is a page you look at.

use gpui::{
    div, img, prelude::FluentBuilder as _, px, AnyElement, App, Entity, InteractiveElement as _,
    IntoElement, ObjectFit, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, StyledImage as _,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use domain::session_results::{
    session_result_tile_width, SessionResultEntry, SessionResultGroup, SESSION_RESULT_TILE_HEIGHT,
};

use crate::issue_detail::{centered_column, DETAIL_GUTTER};
use crate::markdown::{placeholder_box, ImageCache, ImageSlot};

/// The whole page for one run's results, ready to drop into the pane slot
/// under the work header. Never called with an empty `groups` — an empty
/// Results face is not a face (the caller falls back to the transcript).
pub(crate) fn render(
    groups: &[SessionResultGroup],
    images: &Entity<ImageCache>,
    cx: &mut App,
) -> AnyElement {
    let mut page = v_flex()
        .w_full()
        .min_w_0()
        // The work column's own gutter, so the bands line up with the
        // header's title above them.
        .px(px(DETAIL_GUTTER))
        .pt_4()
        .pb_8()
        .gap_5();
    for (group_ix, group) in groups.iter().enumerate() {
        let mut tiles = h_flex().w_full().min_w_0().flex_wrap().items_start().gap_3();
        for (tile_ix, entry) in group.entries.iter().enumerate() {
            tiles = tiles.child(tile(entry, (group_ix, tile_ix), images, cx));
        }
        page = page.child(
            v_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .child(crate::surface::glass_section_band(
                    None,
                    SharedString::from(group.topic.clone()),
                    None,
                    cx,
                ))
                .child(tiles),
        );
    }
    div()
        .id("session-results")
        .size_full()
        .min_h_0()
        .overflow_y_scroll()
        .child(centered_column(page))
        .into_any_element()
}

/// One tile: the picture at the shared height, its label under it. The
/// loading and unavailable states paint the neutral placeholder at the SAME
/// box, so the row never reflows when the bytes land.
fn tile(
    entry: &SessionResultEntry,
    index: (usize, usize),
    images: &Entity<ImageCache>,
    cx: &mut App,
) -> AnyElement {
    // The canonical relative form — the same key the cache fetches by and
    // the lightbox opens.
    let url = format!("/api/attachments/{}", entry.attachment_id);
    let natural = match (entry.width, entry.height) {
        (Some(width), Some(height)) => Some((width as f32, height as f32)),
        _ => None,
    };
    let height = SESSION_RESULT_TILE_HEIGHT;
    let width = session_result_tile_width(entry, height);
    let slot = images.update(cx, |cache, cx| cache.slot(&url, cx));
    let label = entry.label.clone();
    // The attachment may be published under two topics, so the id carries
    // the position rather than the attachment alone.
    let (group_ix, tile_ix) = index;
    let muted = cx.theme().muted_foreground;
    v_flex()
        .flex_shrink_0()
        .w(px(width))
        .gap_1()
        .child(
            div()
                .id(SharedString::from(format!(
                    "session-result-{group_ix}-{tile_ix}"
                )))
                .w(px(width))
                .h(px(height))
                .flex_shrink_0()
                .rounded(px(theme::tokens::radius::LG))
                .overflow_hidden()
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .cursor_pointer()
                .map(|tile| match slot {
                    // Cover: every tile is the same height, so a shot whose
                    // probe was wrong crops rather than letterboxes.
                    ImageSlot::Ready(image) => {
                        tile.child(img(image).size_full().object_fit(ObjectFit::Cover))
                    }
                    // A label would only clip — the neutral box IS the
                    // loading/unavailable state here.
                    _ => tile.child(placeholder_box("", cx)),
                })
                .on_click({
                    let images = images.clone();
                    let url = url.clone();
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
                }),
        )
        .child(
            div()
                .w_full()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(label)),
        )
        .into_any_element()
}
