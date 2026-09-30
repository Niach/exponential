//! EXP-879 — the RESULTS face: the pictures a coding run published while it
//! worked, read off `coding_sessions.results`.
//!
//! ONE scrolling page, byte-for-byte the web/iOS/Android shape: per topic a
//! group band ([`crate::surface::glass_section_band`], the EXP-818 list
//! design) over a WRAPPING row of EQUAL-HEIGHT tiles — every tile
//! [`SESSION_RESULT_TILE_HEIGHT`] tall, or the one smaller height that makes
//! the widest of them fit a narrow window
//! ([`session_result_tile_height_fitting`], the shared ×4 rule), its width
//! from the probed aspect ([`session_result_tile_width`], 4:3 without one) —
//! each with a muted one-line caption under it. A shot of the same screen on
//! web, iOS and Android therefore reads side by side on one baseline.
//!
//! The image is the ordinary member-gated attachment route
//! (`/api/attachments/{id}`) fetched through the owning view's shared
//! [`ImageCache`], exactly like a comment's attachment; a click opens the
//! in-app lightbox at full size, never the browser.
//!
//! EXP-933: the page is the run's REPORT — a topic may carry GFM TEXT
//! (read-only [`crate::markdown::MarkdownView`], `#IDENT`/`@email` pills
//! resolved against `team_id`) rendered between its band and its tiles; a
//! text-only topic is a band + text.
//!
//! This face carries NO Stop/Resume (the Run face owns it) and NO merge bar
//! (Changes owns it) — it is a page you look at.

use gpui::{
    div, hsla, img, linear_color_stop, linear_gradient, prelude::FluentBuilder as _, px,
    AnyElement, App, Entity, InteractiveElement as _, IntoElement, ObjectFit, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use domain::session_results::{
    session_result_is_tall, session_result_tile_height_fitting, session_result_tile_width,
    SessionResultEntry, SessionResultGroup, SESSION_RESULT_TILE_HEIGHT,
};

use crate::issue_detail::{centered_column, DETAIL_GUTTER};
use crate::markdown::{placeholder_box, ImageCache, ImageSlot, MarkdownView, RefResolver};

/// The whole page for one run's results, ready to drop into the pane slot
/// under the work header. Never called with an empty `groups` — an empty
/// Results face is not a face (the caller falls back to the transcript).
///
/// `available_width` is the width the TILES ROW actually gets (the centered
/// column's inner width, gutters already taken off). The page's tile height
/// is derived from it ONCE, by the shared ×4 rule
/// ([`session_result_tile_height_fitting`]): a narrow window scales the whole
/// page down by one factor rather than clipping its widest tile.
pub(crate) fn render(
    groups: &[SessionResultGroup],
    available_width: f32,
    images: &Entity<ImageCache>,
    team_id: Option<&str>,
    cx: &mut App,
) -> AnyElement {
    // ONE height for the page, not one per band — a shot's counterpart in the
    // NEXT topic has to sit on the same baseline too. The per-group minimum
    // IS the whole page's factor: the rule never grows a tile, so the group
    // holding the widest tile is the one that decides.
    let height = groups
        .iter()
        .map(|group| session_result_tile_height_fitting(&group.entries, available_width))
        .fold(SESSION_RESULT_TILE_HEIGHT, f32::min);
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
            tiles = tiles.child(tile(entry, height, (group_ix, tile_ix), images, cx));
        }
        let text = group.text.as_ref().map(|text| {
            let mut view = MarkdownView::new(
                SharedString::from(format!("session-result-text-{group_ix}")),
                text.clone(),
            )
            .selectable(true)
            .images(images.clone());
            if let Some(team_id) = team_id {
                let team = team_id.to_string();
                view = view
                    .resolver(RefResolver::from_store(team_id))
                    .on_open_issue(move |identifier, window, cx| {
                        crate::description_editor::open_issue_by_identifier(
                            &team, identifier, window, cx,
                        );
                    });
            }
            div().w_full().min_w_0().text_sm().child(view)
        });
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
                .children(text)
                .when(!group.entries.is_empty(), |band| band.child(tiles)),
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

/// One tile: the picture at the page's shared `height`, its label under it.
/// The loading and unavailable states paint the neutral placeholder at the
/// SAME box, so the row never reflows when the bytes land.
///
/// EXP-1128: a TALL picture ([`session_result_is_tall`], the shared ×4 rule)
/// gets the 4:3 frame from [`session_result_tile_width`] and is laid out at
/// the tile's WIDTH, so its top shows (a top crop, never a sliver), under a
/// bottom fade and a `Tall` pill; the lightbox scrolls the whole of it.
fn tile(
    entry: &SessionResultEntry,
    height: f32,
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
    let width = session_result_tile_width(entry, height);
    let tall = session_result_is_tall(entry);
    // Tall implies probed dims; the fallback only guards a zero height.
    let aspect = natural
        .filter(|(_, h)| *h > 0.)
        .map(|(w, h)| w / h)
        .unwrap_or(4. / 3.);
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
                .relative()
                .map(|tile| match slot {
                    // Tall: laid out at the tile's width, aspect-true, so the
                    // box's overflow clip shows its TOP.
                    ImageSlot::Ready(image) if tall => tile.child(
                        img(image)
                            .flex_shrink_0()
                            .w(px(width))
                            .h(px(width / aspect))
                            .object_fit(ObjectFit::Fill),
                    ),
                    // Cover: every tile is the same height, so a shot whose
                    // probe was wrong crops rather than letterboxes.
                    ImageSlot::Ready(image) => {
                        tile.child(img(image).size_full().object_fit(ObjectFit::Cover))
                    }
                    // Over the texture cap: strips at the tile's width — the
                    // same top crop (a >16k px WIDE picture just shows its
                    // top-left band at this width).
                    ImageSlot::ReadyTall(strips) => {
                        tile.child(crate::tall_image::render_tall(&strips, width))
                    }
                    // A label would only clip — the neutral box IS the
                    // loading/unavailable state here.
                    ImageSlot::Loading | ImageSlot::Failed(_) => {
                        tile.child(placeholder_box("", cx))
                    }
                })
                .when(tall, |tile| tile.child(tall_fade()).child(tall_pill()))
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

/// The bottom fade over a tall tile: the picture continues below the crop.
/// Paint-only (no mouse handlers), so the click reaches the tile.
fn tall_fade() -> impl IntoElement {
    div()
        .absolute()
        .bottom_0()
        .left_0()
        .right_0()
        .h(px(48.))
        .bg(linear_gradient(
            180.,
            linear_color_stop(hsla(0., 0., 0., 0.), 0.),
            linear_color_stop(hsla(0., 0., 0., 0.5), 1.),
        ))
}

/// The `Tall` pill (same copy ×4) in the tile's bottom-right corner.
fn tall_pill() -> impl IntoElement {
    div()
        .absolute()
        .bottom_1p5()
        .right_1p5()
        .rounded_full()
        .px_1p5()
        .py_0p5()
        .text_xs()
        .text_color(hsla(0., 0., 1., 0.9))
        .bg(hsla(0., 0., 0., 0.6))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
        .child("Tall")
}
