//! EXP-879 — the Guide: the pictures a coding run published while it
//! worked. EXP-1294: read off the chat root's `sessions.guide`, the
//! chat's Guide tab.
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
//! EXP-1172: a topic's INLINE pictures (`exponential_sessions_show` shots
//! the run filed while it worked) fold under the group's tiles into a
//! collapsed `Earlier · N` disclosure that expands IN PLACE to the same
//! tiles; the same tile ([`tile`]) also renders one such picture under its
//! call in the transcript.
//!
//! EXP-1154/EXP-1251: the page is the GUIDE (Changes + Results merged into
//! ONE face): the Stack card, the `Summary` lead, numbered sections with ONE
//! Changes row each (the shared coverage rule), the automatic `Other
//! changes` section and `Show complete diff`; a row opens the diff page
//! filtered to its files.

use gpui::{
    div, hsla, img, linear_color_stop, linear_gradient, prelude::FluentBuilder as _, px,
    AnyElement, App, Entity, InteractiveElement as _, IntoElement, ObjectFit, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, StyledImage as _, Window,
};
use gpui_component::{v_flex, ActiveTheme as _, Sizable as _};

use domain::session_results::{session_result_is_tall, session_result_tile_width, SessionResultEntry};

use crate::markdown::{placeholder_box, ImageCache, ImageSlot};

/// EXP-1283: what a Guide tile hands THE picture viewer — the raw Guide and
/// the team its text pills resolve against.
#[derive(Clone)]
pub(crate) struct GuideViewerSource {
    pub(crate) guide: std::rc::Rc<serde_json::Value>,
    pub(crate) team_id: Option<String>,
}

/// A stack row was picked (its pull request id, EXP-1294).
pub(crate) type OnPickStackRow = std::rc::Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// The pointer entered a stack row (`Some(pull request id)`) or left it
/// (`None`).
pub(crate) type OnHoverStackRow = std::rc::Rc<dyn Fn(Option<String>, &mut Window, &mut App)>;

/// EXP-1248 — the Guide's Stack card (web `GuideBody`'s Stack card): a
/// `Stack` band (the `pr-stack` glyph, NO count) over the shared stack rail
/// ([`crate::pr_rows::stack_rail`]) TOP first, the current member wearing
/// the active wash, down to the muted base-branch row. A click REPLACES the
/// subject (`on_pick`); the HOVERED member (`hovered`, the host's view
/// state written by `on_hover`) offers the ghost `Merge through here`.
/// Web `STACK_BASE_FALLBACK`: never an assumed `main` (default branches
/// resolve live).
pub(crate) const STACK_BASE_FALLBACK: &str = "default branch";

pub(crate) fn stack_card(
    view: &domain::pr_stack::StackView,
    // The board's default branch: the base row's label when the bottom
    // pull request's `base` is not synced (then `default branch`).
    default_branch: Option<String>,
    on_pick: Option<OnPickStackRow>,
    on_merge_through: Option<OnPickStackRow>,
    hovered: Option<String>,
    on_hover: Option<OnHoverStackRow>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let members = view
        .rows
        .iter()
        .map(|row| crate::pr_rows::StackRailMember {
            key: row.pull_request_id.clone(),
            identifier: SharedString::from(row.identifier.clone()),
            title: SharedString::from(row.title.clone()),
            current: row.is_current,
            on_open: on_pick.clone().filter(|_| !row.is_current).map(|on_pick| {
                let pull_request_id = row.pull_request_id.clone();
                Box::new(move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
                    on_pick(&pull_request_id, window, cx)
                }) as crate::run_rows::RunRowAction
            }),
            trailing: None,
        })
        .collect();
    let rail = crate::pr_rows::stack_rail(
        crate::pr_rows::StackRailSpec {
            id_prefix: SharedString::from("guide-stack"),
            members,
            base_branch: SharedString::from(
                view.base_branch
                    .clone()
                    .or(default_branch)
                    .unwrap_or_else(|| STACK_BASE_FALLBACK.to_string()),
            ),
            word: None,
            hovered,
            on_hover,
            on_merge_through,
        },
        cx,
    );
    v_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .child(crate::surface::glass_section_band(
            Some(
                gpui_component::Icon::from(crate::icons::registry::PR_STACK)
                    .with_size(px(14.))
                    .text_color(muted)
                    .into_any_element(),
            ),
            SharedString::from("Stack"),
            None,
            cx,
        ))
        .child(rail)
        .into_any_element()
}

/// One tile: the picture at the page's shared `height`, `caption` under it
/// (a Results tile's label, the Earlier band's included; EXP-1172: a
/// transcript tile's `session_result_tile_caption`). `id` is unique per position — an
/// attachment may be published under two topics.
/// The loading and unavailable states paint the neutral placeholder at the
/// SAME box, so the row never reflows when the bytes land.
///
/// EXP-1128: a TALL picture ([`session_result_is_tall`], the shared ×4 rule)
/// gets the 4:3 frame from [`session_result_tile_width`] and is laid out at
/// the tile's WIDTH, so its top shows (a top crop, never a sliver), under a
/// bottom fade and a `Tall` pill; the lightbox scrolls the whole of it.
pub(crate) fn tile(
    entry: &SessionResultEntry,
    height: f32,
    id: SharedString,
    caption: String,
    images: &Entity<ImageCache>,
    viewer: Option<&GuideViewerSource>,
    cx: &mut App,
) -> AnyElement {
    let viewer = viewer.cloned();
    let attachment_id = entry.attachment_id.clone();
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
    let label = caption;
    let muted = cx.theme().muted_foreground;
    v_flex()
        .flex_shrink_0()
        .w(px(width))
        .gap_1()
        .child(
            div()
                .id(id)
                .role(gpui::Role::Button)
                .aria_label(label.clone())
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
                    ImageSlot::Loading | ImageSlot::Failed(_) | ImageSlot::Unrenderable => {
                        tile.child(placeholder_box("", cx))
                    }
                })
                .when(tall, |tile| tile.child(tall_fade()).child(tall_pill()))
                .on_click({
                    let images = images.clone();
                    let url = url.clone();
                    let label = label.clone();
                    move |_, window, cx| {
                        // EXP-1283: a Guide picture opens THE viewer.
                        if let Some(viewer) = viewer.clone() {
                            crate::image_preview::open_guide_viewer(
                                viewer,
                                attachment_id.clone(),
                                images.clone(),
                                window,
                                cx,
                            );
                            return;
                        }
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
