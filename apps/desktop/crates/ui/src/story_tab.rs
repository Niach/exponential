//! STORIES TAB (`fixtures/stories.json` STORIES TAB; web
//! `packages/ui/src/story-tab.tsx` `StoryTab` / `StoryTile`, iOS ExpUI
//! StoryTab, Android ui/stories StoryTab): the session screen's Stories tab
//! body. The model is `domain::stories::story_tab`'s ring; these only paint
//! it (the screen and the styleguide's `story-tab` draw them).
//!
//! - [`story_tab_body`]: the PR block slot (`lead`), `tile.leadGap` under
//!   it the tiles in fold order `tile.listGap` apart, else THE EmptyState.
//! - [`story_tile`]: #EXP-1462 one story's section, drawn INLINE (the
//!   Guide's way, never a list of title rows): line 1 = the title (one
//!   line; none = the kind word) with `meta` muted at the end (a waiting one
//!   leads with the plan/question tint dot and tints its meta), the
//!   description (muted, wrapping) `tile.textGap` under it, then its body in
//!   place `tile.gap` under that: ONE row of pictures (`tile.thumbHeight.wide`
//!   tall, width by aspect capped at `tile.thumbMaxWidth.wide`, a muted
//!   caption-or-label under each), else THE surface card (no title; a reply
//!   press goes to the story's session as in the chat), else the plan
//!   markdown.
//! - [`changes_row`]: the PR block's ONE `Changes` row (the diff glyph,
//!   `n files +a −d`, a chevron) opening THE review pane; [`review_header`]
//!   = that pane's `‹ Stories` back row.

use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, img, px, AnyElement, App, Div, Entity, FontWeight, Hsla, InteractiveElement as _, IntoElement as _,
    ObjectFit, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, StyledImage as _,
    Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use domain::stories::{Story, StoryKind, StoryPicture, StoryRing, STORIES};

use crate::icons::registry;
use crate::markdown::{ImageCache, ImageSlot};
use crate::thread_screen::ThreadText;

/// How the tab draws a story's body in place (#EXP-1462).
pub(crate) struct TabBody<'a> {
    /// The pictures' (and the plan markdown's) cache.
    pub images: &'a Entity<ImageCache>,
    /// `false` = the thumbnails' muted fill alone (the styleguide).
    pub load_pictures: bool,
    /// The plan markdown's mention/link context.
    pub text: &'a ThreadText,
    /// The story's raw show card (its message's `card`), `None` = unknown.
    pub card_for: &'a dyn Fn(&Story, &App) -> Option<serde_json::Value>,
}

/// The live tab's [`TabBody::card_for`]: the synced message's card.
pub(crate) fn store_story_card(story: &Story, cx: &App) -> Option<serde_json::Value> {
    let store = sync::Store::try_global(cx)?;
    store.collections().messages.read(cx).get(&story.id)?.card.clone()
}

/// A tile or a thumbnail was clicked: the story, the picture's index.
pub(crate) type OnOpenStory = Rc<dyn Fn(&Story, usize, &mut Window, &mut App)>;

/// The waiting tint (ask-dock: plan primary, question yellow).
fn waiting_tint(story: &Story, cx: &App) -> Hsla {
    match story.kind {
        StoryKind::Question => theme::tokens::YELLOW.to_hsla(),
        _ => cx.theme().primary,
    }
}

/// A thumbnail's width (px, the IDE = `wide`): by the picture's aspect at
/// `tile.thumbHeight`, capped at `tile.thumbMaxWidth`; unknown = 4:3 (web
/// `storyThumbWidth`).
pub(crate) fn story_thumb_width(picture: &StoryPicture) -> f32 {
    let tile = &STORIES.tab.tile;
    let aspect = match (picture.width, picture.height) {
        (Some(w), Some(h)) if w > 0 && h > 0 => w as f32 / h as f32,
        _ => 4. / 3.,
    };
    tile.thumb_max_width.wide.min((tile.thumb_height.wide * aspect).round())
}

/// THE story section (#EXP-1462: inline).
pub(crate) fn story_tile(
    story: &Story,
    body: &TabBody<'_>,
    on_open: Option<OnOpenStory>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let images = body.load_pictures.then_some(body.images);
    let tile = &STORIES.tab.tile;
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let foreground = theme.foreground;
    let tint = story.waiting.then(|| waiting_tint(story, cx));
    let title = story.title.clone().unwrap_or_else(|| story.kind.label().to_string());
    let line_open = on_open.clone();
    let line_story = story.clone();
    let head = h_flex()
        .min_w_0()
        .items_center()
        .gap_2()
        .when_some(tint, |line, tint| line.child(div().flex_shrink_0().size(px(6.)).rounded_full().bg(tint)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(foreground)
                .child(SharedString::from(title)),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(tint.unwrap_or(muted))
                .child(SharedString::from(story.meta.clone())),
        );
    // Line 1 + the description: a tap opens THE viewer at the story.
    let line = v_flex()
        .id(SharedString::from(format!("story-tile-line-{}", story.id)))
        .min_w_0()
        .gap(px(tile.text_gap))
        .rounded_md()
        .when(on_open.is_some(), |line| line.cursor_pointer())
        .on_click(move |_, window, cx| {
            if let Some(open) = &line_open {
                open(&line_story, 0, window, cx);
            }
        })
        .child(head)
        .when_some(story.description.clone().filter(|text| !text.trim().is_empty()), |line, description| {
            line.child(
                div()
                    .id(SharedString::from(format!("story-tile-description-{}", story.id)))
                    .min_w_0()
                    .text_sm()
                    .text_color(muted)
                    .child(SharedString::from(description)),
            )
        });
    // The frame rule: pictures win, else the surface, else the plan text.
    let surface = (story.pictures.is_empty() && story.surface)
        .then(|| (body.card_for)(story, cx))
        .flatten()
        .and_then(|card| domain::session_tree::card_surface(card.get("surface")));
    let inline: Option<AnyElement> = match (&surface, story.text.as_deref()) {
        _ if !story.pictures.is_empty() => None,
        (Some(surface), _) => Some(
            div()
                .id(SharedString::from(format!("story-tile-surface-{}", story.id)))
                .w_full()
                .min_w_0()
                .child(crate::surface_card::surface_row(&story.id, &story.session_id, None, surface, window, cx))
                .into_any_element(),
        ),
        (None, Some(markdown)) => Some(
            crate::steer_viewer::ask_body_text(cx)
                .id(SharedString::from(format!("story-tile-text-{}", story.id)))
                .text_sm()
                .child(crate::thread_screen::message_markdown(
                    SharedString::from(format!("story-tab-text-{}", story.id)),
                    &crate::plan_card::strip_pending_uploads(markdown),
                    body.text,
                    body.images,
                ))
                .into_any_element(),
        ),
        _ => None,
    };    let thumbs: Vec<AnyElement> = story
        .pictures
        .iter()
        .enumerate()
        .map(|(index, picture)| {
            let width = story_thumb_width(picture);
            let url = format!("/api/attachments/{}", picture.attachment_id);
            let ready = images.and_then(|images| match images.update(cx, |cache, cx| cache.slot(&url, cx)) {
                ImageSlot::Ready(image) => Some(image),
                _ => None,
            });
            let frame = div()
                .w(px(width))
                .h(px(tile.thumb_height.wide))
                .rounded(px(tile.radius))
                .border_1()
                .border_color(theme::tokens::glass::STROKE_CARD.to_hsla())
                .bg(cx.theme().muted)
                .overflow_hidden()
                .when_some(ready, |frame, image| {
                    frame.child(img(image).size_full().object_fit(ObjectFit::Cover))
                });
            let open = on_open.clone();
            let thumb_story = story.clone();
            v_flex()
                .id(SharedString::from(format!("story-tile-picture-{}-{index}", story.id)))
                .flex_shrink_0()
                .w(px(width))
                .gap_1p5()
                .when(open.is_some(), |thumb| thumb.cursor_pointer())
                .on_click(move |_, window, cx| {
                    if let Some(open) = &open {
                        open(&thumb_story, index, window, cx);
                    }
                })
                .child(frame)
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(muted)
                        .child(SharedString::from(picture.caption.clone().unwrap_or_else(|| picture.label.clone()))),
                )
                .into_any_element()
        })
        .collect();
    v_flex()
        .id(SharedString::from(format!("{}-{}", STORIES.tab.test_ids.tile, story.id)))
        .min_w_0()
        .gap(px(tile.gap))
        .child(line)
        .when(!thumbs.is_empty(), |column| {
            column.child(
                h_flex()
                    .id(SharedString::from(format!("story-tile-pictures-{}", story.id)))
                    .min_w_0()
                    .gap(px(tile.gap))
                    .overflow_x_scroll()
                    .children(thumbs),
            )
        })
        .children(inline)
        .into_any_element()
}

/// THE Stories tab body (`story_tab`'s ring; `None` = the empty state).
pub(crate) fn story_tab_body(
    ring: Option<&StoryRing>,
    lead: Option<AnyElement>,
    body: &TabBody<'_>,
    on_open: Option<OnOpenStory>,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let tile = &STORIES.tab.tile;
    let stories = ring.map(|ring| ring.stories.as_slice()).unwrap_or_default();
    let list: AnyElement = if stories.is_empty() {
        crate::controls::empty_state(Icon::from(registry::THREAD_STORIES), STORIES.tab.copy.empty.clone(), None, cx)
            .into_any_element()
    } else {
        v_flex()
            .min_w_0()
            .gap(px(tile.list_gap))
            .children(stories.iter().map(|story| story_tile(story, body, on_open.clone(), window, cx)))
            .into_any_element()
    };
    v_flex().min_w_0().w_full().gap(px(tile.lead_gap)).children(lead).child(list)
}

/// The PR block's ONE Changes row (web `GuideChangesRow` with the tab's
/// label): the diff glyph, `Changes`, the muted `N files`, `+A −D` and a
/// chevron over a hairline; a click opens THE review pane.
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
        .id(SharedString::from(STORIES.tab.test_ids.changes.clone()))
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
        .child(div().flex_1().min_w_0().truncate().child(SharedString::from(STORIES.tab.copy.changes.clone())))
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
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(move |_, window, cx| on_open(window, cx));
    }
    line.into_any_element()
}

/// THE review pane's back row (web `StoriesReview`'s header): `‹ Stories` ·
/// `Changes` · `+A −D · N files`.
pub(crate) fn review_header(
    files: usize,
    additions: u32,
    deletions: u32,
    on_back: Rc<dyn Fn(&mut Window, &mut App)>,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let back = h_flex()
        .id("thread-stories-review-back")
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
        .child(div().text_sm().child(SharedString::from(STORIES.tab.copy.label.clone())));
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
                .child(SharedString::from(STORIES.tab.copy.changes.clone())),
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

    /// web `storyThumbWidth`: by aspect at 400 tall, capped at 720; 4:3
    /// without a size.
    #[test]
    fn thumbs_size_by_aspect_capped() {
        let picture = |width, height| StoryPicture {
            label: String::new(),
            attachment_id: "a".into(),
            width,
            height,
            caption: None,
        };
        assert_eq!(story_thumb_width(&picture(Some(1600), Some(1000))), 640.);
        assert_eq!(story_thumb_width(&picture(Some(1179), Some(2556))), 185.);
        assert_eq!(story_thumb_width(&picture(Some(4000), Some(1000))), 720.);
        assert_eq!(story_thumb_width(&picture(None, None)), 533.);
    }
}
