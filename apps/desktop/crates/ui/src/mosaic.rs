//! #EXP-1493 THE Mosaic (eUI `Mosaic`; web `packages/ui/src/picture-group.tsx`
//! `Mosaic`): THE wide picture group, `domain::plan_page::mosaic_tiles`
//! geometry (`plan-page.json` `mosaicCases` + `mosaic`): one = one large
//! tile, two = two halves side by side, three or more = the first large on
//! the left (both rows, `largeFraction` of the width), the next two small
//! stacked on the right, the last carrying "+N" over a black 0.55 scrim;
//! `gap` between tiles, the whole group `aspect` w/h, each tile `radius` +
//! the glass-stroke hairline over the muted fill, pictures cover. A press
//! on a tile = `on_open(index)`.

use std::rc::Rc;

use gpui::{
    div, img, prelude::FluentBuilder as _, px, relative, AnyElement, App, Div, Entity, InteractiveElement as _,
    IntoElement as _, ObjectFit, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    StyledImage as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use domain::plan_page::{mosaic_tiles, MosaicTile, MosaicTileSize, PLAN_PAGE};

use crate::markdown::{ImageCache, ImageSlot};

/// A tile press: the picture's index in the group.
pub(crate) type OnMosaicOpen = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// THE Mosaic over `urls` (canonical `/api/attachments/<id>` keys).
/// `images` = `None` paints every tile as the muted placeholder (the
/// styleguide specimen).
pub(crate) fn mosaic(
    id: &str,
    urls: &[String],
    images: Option<&Entity<ImageCache>>,
    on_open: Option<OnMosaicOpen>,
    cx: &mut App,
) -> AnyElement {
    let spec = &PLAN_PAGE.mosaic;
    let tiles = mosaic_tiles(urls.len());
    let tile = |at: usize, cx: &mut App| -> AnyElement {
        match tiles.get(at) {
            Some(entry) => mosaic_tile(id, entry, urls.get(entry.index).map(String::as_str), images, on_open.clone(), cx),
            None => div().into_any_element(),
        }
    };
    let group = h_flex()
        .id(SharedString::from(format!("{}-{id}", PLAN_PAGE.test_ids.mosaic)))
        .w_full()
        .aspect_ratio(spec.aspect)
        .gap(px(spec.gap));
    match tiles.len() {
        0 => div().into_any_element(),
        1 => group.child(tile(0, cx)).into_any_element(),
        2 => group.child(tile(0, cx)).child(tile(1, cx)).into_any_element(),
        _ => {
            let large = tile(0, cx);
            let (top, bottom) = (tile(1, cx), tile(2, cx));
            group
                .child(grow(spec.large_fraction).h_full().flex().child(large))
                .child(grow(1. - spec.large_fraction).h_full().child(v_flex().size_full().gap(px(spec.gap)).child(top).child(bottom)))
                .into_any_element()
        }
    }
}

/// A flex child taking `fraction` of what the gaps leave.
fn grow(fraction: f32) -> Div {
    let mut column = div().min_w_0().flex_basis(relative(0.));
    column.style().flex_grow = Some(fraction);
    column.style().flex_shrink = Some(1.);
    column
}

fn mosaic_tile(
    group: &str,
    entry: &MosaicTile,
    url: Option<&str>,
    images: Option<&Entity<ImageCache>>,
    on_open: Option<OnMosaicOpen>,
    cx: &mut App,
) -> AnyElement {
    let spec = &PLAN_PAGE.mosaic;
    let picture = match (url, images) {
        (Some(url), Some(images)) => match images.update(cx, |cache, cx| cache.slot(url, cx)) {
            ImageSlot::Ready(image) => Some(img(image).size_full().object_fit(ObjectFit::Cover).into_any_element()),
            _ => None,
        },
        _ => None,
    };
    let index = entry.index;
    let size = match entry.size {
        MosaicTileSize::Large => "large",
        MosaicTileSize::Half => "half",
        MosaicTileSize::Small => "small",
    };
    div()
        .id(SharedString::from(format!("mosaic-tile-{group}-{index}-{size}")))
        .relative()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .size_full()
        .overflow_hidden()
        .rounded(px(spec.radius))
        .border_1()
        .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
        .bg(cx.theme().muted)
        .cursor_pointer()
        .children(picture)
        .when(entry.more > 0, |tile| {
            tile.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(gpui::black().opacity(0.55))
                    .text_lg()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(gpui::white())
                    .child(SharedString::from(format!("+{}", entry.more))),
            )
        })
        .when_some(on_open, move |tile, on_open| {
            tile.on_click(move |_, window, cx| {
                cx.stop_propagation();
                on_open(index, window, cx);
            })
        })
        .into_any_element()
}
