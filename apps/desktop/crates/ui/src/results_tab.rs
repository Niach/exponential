//! #FEED-191 THE RESULTS TAB (`fixtures/results-tab.json`, its `$comment` is the
//! spec; web `packages/ui/src/results-tab.tsx`, iOS ExpUI ResultsTab, Android
//! ui/results ResultsTab): the session screen's last tab. The model is
//! `domain::results_tab::results_tab`; these only paint it (the screen and the
//! styleguide's `results-tab` draw them).
//!
//! - [`results_tab_body`]: the PR block slot (`lead`), `tile.leadGap` under it
//!   the sections `tile.listGap` apart, else THE EmptyState.
//! - [`result_section`]: the title (semibold, one line) + the muted age, the
//!   description (muted, wrapping), then ONE row of thumbnails with a muted
//!   caption-or-label under each, else THE surface card inline. A picture's press
//!   opens THE lightbox over that result's pictures.
//! - [`changes_row`]: the PR block's ONE `Changes` row, opening THE review pane;
//!   [`review_header`] = that pane's `‹ Results` back row.

use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, img, px, AnyElement, App, Div, Entity, FontWeight, InteractiveElement as _, IntoElement as _, ObjectFit,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, StyledImage as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::results_tab::{is_result_card, results_tab, ResultPicture, ResultsMessageInput, ResultsSessionInput, ResultsTabInput, TabResult, RESULTS_TAB};

use crate::icons::registry;
use crate::image_preview::GalleryPicture;
use crate::markdown::{ImageCache, ImageSlot};

/// A thumbnail's width (the IDE = `wide`): by the picture's aspect at
/// `tile.thumbHeight`, capped at `tile.thumbMaxWidth`; unknown = 4:3 (web
/// `resultThumbWidth`).
pub(crate) fn result_thumb_width(picture: &ResultPicture) -> f32 {
    let tile = &RESULTS_TAB.tab.tile;
    let aspect = match (picture.width, picture.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => w as f32 / h as f32,
        _ => 4. / 3.,
    };
    tile.thumb_max_width.wide.min((tile.thumb_height.wide * aspect).round())
}

fn picture_text(picture: &ResultPicture) -> String {
    picture.caption.clone().unwrap_or_else(|| picture.label.clone())
}

fn picture_url(picture: &ResultPicture) -> String {
    format!("/api/attachments/{}", picture.attachment_id)
}

/// THE lightbox over `result`'s pictures at `index`.
fn open_lightbox(result: &TabResult, index: usize, images: Entity<ImageCache>, window: &mut Window, cx: &mut App) {
    let pictures = result.pictures.iter().map(|picture| GalleryPicture { url: picture_url(picture), alt: picture_text(picture) }).collect();
    crate::image_preview::open_picture_gallery(pictures, index, images, window, cx);
}

/// A result's inline surface, `None` = none.
pub(crate) type SurfaceFor<'a> = &'a dyn Fn(&TabResult, &mut Window, &mut App) -> Option<AnyElement>;

/// THE result section.
pub(crate) fn result_section(
    result: &TabResult,
    images: &Entity<ImageCache>,
    surface_for: SurfaceFor<'_>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let tile = &RESULTS_TAB.tab.tile;
    let (muted, foreground, ground) = {
        let theme = cx.theme();
        (theme.muted_foreground, theme.foreground, theme.muted)
    };
    let thumbs: Vec<AnyElement> = result
        .pictures
        .iter()
        .enumerate()
        .map(|(index, picture)| {
            let width = result_thumb_width(picture);
            let url = picture_url(picture);
            let ready = match images.update(cx, |cache, cx| cache.slot(&url, cx)) {
                ImageSlot::Ready(image) => Some(image),
                _ => None,
            };
            let (open, images) = (result.clone(), images.clone());
            v_flex()
                .id(SharedString::from(format!("result-picture-{}-{index}", result.id)))
                .role(gpui::Role::Button)
                .aria_label(SharedString::from(picture_text(picture)))
                .flex_shrink_0()
                .w(px(width))
                .gap_1p5()
                .cursor_pointer()
                .on_click(move |_, window, cx| open_lightbox(&open, index, images.clone(), window, cx))
                .child(
                    div()
                        .w(px(width))
                        .h(px(tile.thumb_height.wide))
                        .rounded(px(tile.radius))
                        .border_1()
                        .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                        .bg(ground)
                        .overflow_hidden()
                        .when_some(ready, |frame, image| frame.child(img(image).size_full().object_fit(ObjectFit::Cover))),
                )
                .child(div().truncate().text_xs().text_color(muted).child(SharedString::from(picture_text(picture))))
                .into_any_element()
        })
        .collect();
    // Pictures win, else the surface.
    let surface = (result.pictures.is_empty() && result.surface).then(|| surface_for(result, window, cx)).flatten();
    v_flex()
        .id(SharedString::from(format!("{}-{}", RESULTS_TAB.tab.test_ids.section, result.id)))
        .min_w_0()
        .gap(px(tile.gap))
        .child(
            v_flex()
                .min_w_0()
                .gap(px(tile.text_gap))
                .child(
                    h_flex()
                        .min_w_0()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(foreground)
                                .child(SharedString::from(result.title.clone())),
                        )
                        .child(div().flex_shrink_0().text_xs().text_color(muted).child(SharedString::from(result.age.clone()))),
                )
                .when_some(result.description.clone(), |text, description| {
                    text.child(div().min_w_0().text_sm().text_color(muted).child(SharedString::from(description)))
                }),
        )
        .when(!thumbs.is_empty(), |column| {
            column.child(
                h_flex()
                    .id(SharedString::from(format!("result-pictures-{}", result.id)))
                    .min_w_0()
                    .gap(px(tile.gap))
                    .overflow_x_scroll()
                    .children(thumbs),
            )
        })
        .children(surface)
        .into_any_element()
}

/// THE Results tab body (none = the empty state).
pub(crate) fn results_tab_body(
    results: &[TabResult],
    lead: Option<AnyElement>,
    images: &Entity<ImageCache>,
    surface_for: SurfaceFor<'_>,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let tile = &RESULTS_TAB.tab.tile;
    let list: AnyElement = if results.is_empty() {
        crate::controls::empty_state(Icon::from(registry::CHAT_RESULTS), RESULTS_TAB.tab.copy.empty.clone(), None, cx).into_any_element()
    } else {
        let sections: Vec<AnyElement> = results.iter().map(|result| result_section(result, images, surface_for, window, cx)).collect();
        v_flex().min_w_0().gap(px(tile.list_gap)).children(sections).into_any_element()
    };
    v_flex().min_w_0().w_full().gap(px(tile.lead_gap)).children(lead).child(list)
}

/// The live tab's surface: the synced message's card through THE surface
/// renderer (presses behave as in the chat).
pub(crate) fn store_surface(result: &TabResult, window: &mut Window, cx: &mut App) -> Option<AnyElement> {
    let store = sync::Store::try_global(cx)?;
    let card = store.collections().messages.read(cx).get(&result.id)?.card.clone()?;
    let surface = domain::session_tree::card_surface(card.get("surface"))?;
    Some(
        div()
            .id(SharedString::from(format!("result-surface-{}", result.id)))
            .w_full()
            .min_w_0()
            .child(crate::surface_card::surface_row(&result.id, &result.session_id, None, &surface, window, cx))
            .into_any_element(),
    )
}

// ---------------------------------------------------------------------------
// The model
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq)]
struct ResultsKey {
    revisions: [u64; 2],
    /// The ages move with the minute.
    minute: i64,
}

#[derive(Default)]
struct ResultsMemo(HashMap<String, (ResultsKey, Rc<Vec<TabResult>>)>);

impl gpui::Global for ResultsMemo {}

/// Every result of `root_id`'s tree (THE fold over the synced session-authored
/// cards; only the open tab asks), memoised per store change and minute.
pub(crate) fn results_of_root(root_id: &str, cx: &mut App) -> Rc<Vec<TabResult>> {
    let Some(store) = sync::Store::try_global(cx) else {
        return Rc::new(Vec::new());
    };
    let collections = store.collections();
    let now = crate::session_data::now_ms();
    let key = ResultsKey {
        revisions: [
            collections.sessions.read(cx).revision(),
            collections.messages.read(cx).revision(),
        ],
        minute: now / 60_000,
    };
    if let Some((cached, results)) = cx.default_global::<ResultsMemo>().0.get(root_id) {
        if *cached == key {
            return results.clone();
        }
    }
    let results = Rc::new(results_tab(&results_input(root_id, now, cx)));
    cx.global_mut::<ResultsMemo>().0.insert(root_id.to_string(), (key, results.clone()));
    results
}

fn results_input(root_id: &str, now: i64, cx: &App) -> ResultsTabInput {
    let sessions: Vec<ResultsSessionInput> = crate::session_data::tree_of_root(root_id, cx)
        .into_iter()
        .map(|session| ResultsSessionInput {
            root_id: session.root_id_or_self().to_string(),
            name: crate::session_data::session_title_of(&session.id, cx).unwrap_or_default(),
            id: session.id,
        })
        .collect();
    let messages: Vec<ResultsMessageInput> = crate::session_data::keyed_messages(cx, sessions.iter().map(|session| session.id.as_str()), |owner, message| {
        message.author_session_id.as_deref() == Some(owner)
            && message.card.as_ref().is_some_and(is_result_card)
    })
    .into_iter()
    .filter_map(|message| {
        Some(ResultsMessageInput {
            created_at: message.created_at.as_deref().and_then(domain::session_tree::iso_ms)?,
            id: message.id,
            session_id: message.session_id,
            author_session_id: message.author_session_id,
            card: message.card?,
        })
    })
    .collect();
    ResultsTabInput { now, root_id: root_id.to_string(), sessions, messages }
}

// ---------------------------------------------------------------------------
// The PR block
// ---------------------------------------------------------------------------

/// The PR block's ONE Changes row (web `GuideChangesRow` with the tab's label):
/// the diff glyph, `Changes`, the muted `N files`, `+A −D` and a chevron over a
/// hairline; a click opens THE review pane.
pub(crate) fn changes_row(
    files: usize,
    additions: u32,
    deletions: u32,
    on_open: Option<Rc<dyn Fn(&mut Window, &mut App)>>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let hover = cx.theme().list_hover;
    let mut line = crate::surface::flat_row()
        .id(SharedString::from(RESULTS_TAB.tab.test_ids.changes.clone()))
        .flex()
        .w_full()
        .min_w_0()
        .h(px(36.))
        .px_3()
        .gap_3()
        .items_center()
        .border_b_1()
        .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
        .text_sm()
        .child(Icon::from(registry::GUIDE_CHANGES).with_size(px(14.)).text_color(muted))
        .child(div().flex_1().min_w_0().truncate().child(SharedString::from(RESULTS_TAB.tab.copy.changes.clone())))
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(domain::session_results::guide_file_count_label(files))),
        )
        .child(crate::diff_pane::counts(additions, deletions, cx))
        .child(Icon::from(registry::UI_CHEVRON_RIGHT).with_size(px(14.)).text_color(muted));
    if let Some(on_open) = on_open {
        line = line
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(RESULTS_TAB.tab.copy.changes.clone()))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(move |_, window, cx| on_open(window, cx));
    }
    line.into_any_element()
}

/// THE review pane's back row: `‹ Results` · `Changes` · `+A −D · N files`.
pub(crate) fn review_header(
    files: usize,
    additions: u32,
    deletions: u32,
    on_back: Rc<dyn Fn(&mut Window, &mut App)>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let back = h_flex()
        .id("chat-results-review-back")
        .role(gpui::Role::Button)
        .aria_label(SharedString::from(RESULTS_TAB.tab.copy.label.clone()))
        .flex_shrink_0()
        .gap_1p5()
        .px_2()
        .h(px(28.))
        .items_center()
        .rounded_md()
        .cursor_pointer()
        .hover(|style| style.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
        .on_click(move |_, window, cx| on_back(window, cx))
        .child(Icon::from(registry::UI_BACK).with_size(px(16.)))
        .child(div().text_sm().child(SharedString::from(RESULTS_TAB.tab.copy.label.clone())));
    h_flex()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .px(px(crate::work_header::WORK_GUTTER))
        .pt_4()
        .pb_2()
        .gap_2()
        .items_center()
        .child(back)
        .child(div().w(px(1.)).h(px(16.)).bg(theme::tokens::glass::STROKE_ROW.to_hsla()))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .child(SharedString::from(RESULTS_TAB.tab.copy.changes.clone())),
        )
        .child(crate::diff_pane::counts(additions, deletions, cx))
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(muted)
                .child(SharedString::from(format!("· {}", domain::session_results::guide_file_count_label(files)))),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// web `resultThumbWidth`: by aspect at 240 tall, capped at 320; 4:3 unknown.
    #[test]
    fn thumbs_size_by_aspect_capped() {
        let picture = |width, height| ResultPicture { label: String::new(), attachment_id: "a".into(), width, height, caption: None };
        assert_eq!(result_thumb_width(&picture(Some(1000), Some(1000))), 240.);
        assert_eq!(result_thumb_width(&picture(Some(1179), Some(2556))), 111.);
        assert_eq!(result_thumb_width(&picture(Some(4000), Some(1000))), 320.);
        assert_eq!(result_thumb_width(&picture(None, None)), 320.);
    }

    /// A press names the attachment route, the caption else the label.
    #[test]
    fn a_picture_reads_its_caption_else_its_label() {
        let picture = |caption: Option<&str>| ResultPicture {
            label: "web board".into(),
            attachment_id: "a1".into(),
            width: None,
            height: None,
            caption: caption.map(Into::into),
        };
        assert_eq!(picture_text(&picture(Some("Grouped"))), "Grouped");
        assert_eq!(picture_text(&picture(None)), "web board");
        assert_eq!(picture_url(&picture(None)), "/api/attachments/a1");
    }
}
