//! #FEED-191: `results-tab` (`fixtures/results-tab.json` `tab.specimen`; web
//! entry `results-tab`, iOS + Android twins): THE session screen's Results tab
//! body over the specimen case's fold, the IDE's `wide` frame, no PR block.
//! The pictures are the committed samples (EXP-1654:
//! [`crate::specimen_pictures`] seeds the entry's cache, nothing is fetched); a
//! press opens THE lightbox over that result's pictures. Like web's
//! `ResultsTabSpecimen` (no `surfaceFor`) a surface result draws its text alone.

use gpui::{
    div, img, prelude::FluentBuilder as _, px, AnyElement, App, Div, Entity, FontWeight, InteractiveElement as _,
    IntoElement as _, ObjectFit, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon};

use domain::results_tab::{results_tab_specimen, ResultPicture, TabResult, RESULTS_TAB};

use crate::icons::registry;
use crate::image_preview::GalleryPicture;
use crate::markdown::{ImageCache, ImageSlot};

pub(crate) const ID: &str = "results-tab";
pub(crate) const OWNER: &str = "FEED-191";

/// The specimen's card width (web's `ResultsTabSpecimen` default).
const WIDTH: f32 = 640.;

/// A thumbnail's width (the IDE = `wide`): by the picture's aspect at
/// `tile.thumbHeight`, capped at `tile.thumbMaxWidth`; unknown = 4:3.
fn thumb_width(picture: &ResultPicture) -> f32 {
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

fn open_lightbox(result: &TabResult, index: usize, images: Entity<ImageCache>, window: &mut Window, cx: &mut App) {
    let pictures = result
        .pictures
        .iter()
        .map(|picture| GalleryPicture { url: format!("/api/attachments/{}", picture.attachment_id), alt: picture_text(picture) })
        .collect();
    crate::image_preview::open_picture_gallery(pictures, index, images, window, cx);
}

fn section(result: &TabResult, images: &Entity<ImageCache>, cx: &mut App) -> AnyElement {
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
            let width = thumb_width(picture);
            let url = format!("/api/attachments/{}", picture.attachment_id);
            let ready = match images.update(cx, |cache, cx| cache.slot(&url, cx)) {
                ImageSlot::Ready(image) => Some(image),
                _ => None,
            };
            let (result, images) = (result.clone(), images.clone());
            v_flex()
                .id(SharedString::from(format!("result-picture-{}-{index}", result.id)))
                .flex_shrink_0()
                .w(px(width))
                .gap_1p5()
                .cursor_pointer()
                .on_click(move |_, window, cx| open_lightbox(&result, index, images.clone(), window, cx))
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
        .into_any_element()
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let results = results_tab_specimen();
    let images = window
        .use_keyed_state("sg-results-tab-images", cx, |_, _| {
            let mut cache = ImageCache::new(None);
            crate::specimen_pictures::seed_attachments(&mut cache);
            cache
        })
        .clone();
    let body: AnyElement = if results.is_empty() {
        crate::controls::empty_state(Icon::new(registry::THREAD_RESULTS), RESULTS_TAB.tab.copy.empty.clone(), None, cx)
            .into_any_element()
    } else {
        let sections: Vec<AnyElement> = results.iter().map(|result| section(result, &images, cx)).collect();
        v_flex().min_w_0().gap(px(RESULTS_TAB.tab.tile.list_gap)).children(sections).into_any_element()
    };
    let background = cx.theme().background;
    v_flex().items_start().child(
        v_flex()
            .id(SharedString::from(RESULTS_TAB.tab.test_ids.list.clone()))
            .w(px(WIDTH))
            .p(px(20.))
            .rounded(px(24.))
            .border_1()
            .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
            .bg(background)
            .child(body),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// web `resultThumbWidth`: by aspect at 240 tall, capped at 320; 4:3 unknown.
    #[test]
    fn thumbs_size_by_aspect_capped() {
        let picture = |width, height| ResultPicture { label: String::new(), attachment_id: "a".into(), width, height, caption: None };
        assert_eq!(thumb_width(&picture(Some(1000), Some(1000))), 240.);
        assert_eq!(thumb_width(&picture(Some(1179), Some(2556))), 111.);
        assert_eq!(thumb_width(&picture(Some(4000), Some(1000))), 320.);
        assert_eq!(thumb_width(&picture(None, None)), 320.);
    }

    /// The specimen is the fixture's fold, newest first.
    #[test]
    fn the_specimen_is_the_fold() {
        let results = results_tab_specimen();
        assert!(!results.is_empty());
        assert!(results.windows(2).all(|pair| pair[0].created_at >= pair[1].created_at));
    }
}
