//! # ChoiceTiles (EXP-1280): THE picture-choice control
//!
//! The low-level twin of the catalog's `ChoiceTiles` (the core expands that
//! one into `tile` / `picture` / `label` / `description` parts the surface
//! painter draws through the theme's recipes). This is the same control for
//! a HOST's own chrome: the IDE's plan / question cards paint their picture
//! options with it (`packages/domain-contract/fixtures/plan-card.json`
//! `tiles`). ONE look ×4: `columns` equal tiles per row, `gap` apart; each
//! tile = the picture (`picture_aspect` w / h, cover, rounded) over the label
//! and the muted description, `padding` inside a `radius` frame; a picked
//! tile wears a `selected_ring` px accent (primary) ring; a tile without a
//! picture keeps the frame with a placeholder glyph so a row lines up.

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, Div, ElementId, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon};

use super::sizing::WebText as _;
use crate::chrome::Chrome;

/// The tile grid's geometry (px; `picture_aspect` = width / height). The
/// default = `plan-card.json` `tiles`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChoiceTilesGeometry {
    pub columns: usize,
    pub gap: f32,
    pub picture_aspect: f32,
    pub radius: f32,
    pub selected_ring: f32,
    pub padding: f32,
}

impl Default for ChoiceTilesGeometry {
    fn default() -> Self {
        Self {
            columns: 2,
            gap: 8.,
            picture_aspect: 1.6,
            radius: 10.,
            selected_ring: 2.,
            padding: 8.,
        }
    }
}

/// A tile's picture slot.
pub enum TilePicture {
    /// The host's image element (it owns loading + auth); painted cover.
    Element(AnyElement),
    /// No picture: the frame with this glyph, so a row keeps one height.
    Placeholder(Icon),
}

/// One option as a tile.
pub struct ChoiceTile {
    pub label: SharedString,
    pub description: Option<SharedString>,
    /// `None` = a text-only grid (no picture row at all).
    pub picture: Option<TilePicture>,
    pub selected: bool,
}

/// A press on tile `index`.
pub type OnTilePick = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// The grid: rows of `geometry.columns` equal tiles. `on_pick` = `None`
/// draws it inert (an answered card: the picked tiles keep their ring, the
/// rest dim).
pub fn choice_tiles(
    id: impl Into<SharedString>,
    tiles: Vec<ChoiceTile>,
    geometry: &ChoiceTilesGeometry,
    on_pick: Option<OnTilePick>,
    cx: &App,
) -> Div {
    let id: SharedString = id.into();
    let columns = geometry.columns.max(1);
    let mut grid = v_flex().w_full().min_w_0().gap(px(geometry.gap));
    let mut tiles = tiles.into_iter().enumerate().peekable();
    while tiles.peek().is_some() {
        let mut row = h_flex().w_full().min_w_0().items_stretch().gap(px(geometry.gap));
        for slot in 0..columns {
            match tiles.next() {
                Some((index, tile)) => {
                    let element = choice_tile(
                        ElementId::NamedInteger(id.clone(), index as u64),
                        tile,
                        geometry,
                        on_pick.clone().map(|pick| (index, pick)),
                        cx,
                    );
                    row = row.child(div().flex_1().flex_basis(px(0.)).min_w_0().child(element));
                }
                // The last row's empty slots keep the columns equal.
                None => {
                    let _ = slot;
                    row = row.child(div().flex_1().flex_basis(px(0.)).min_w_0());
                }
            }
        }
        grid = grid.child(row);
    }
    grid
}

/// One tile (see the module docs).
pub fn choice_tile(
    id: ElementId,
    tile: ChoiceTile,
    geometry: &ChoiceTilesGeometry,
    on_pick: Option<(usize, OnTilePick)>,
    cx: &App,
) -> impl IntoElement {
    let chrome = Chrome::global(cx);
    let theme = cx.theme();
    let interactive = on_pick.is_some();
    // The ring grows inward: the padding shrinks by the extra border so the
    // content never shifts when a tile is picked.
    let border = if tile.selected { geometry.selected_ring } else { 1. };
    let padding = (geometry.padding - (border - 1.)).max(0.);
    let picture_radius = (geometry.radius - geometry.padding / 2.).max(2.);
    let picture = tile.picture.map(|picture| {
        let frame = div()
            .w_full()
            .aspect_ratio(geometry.picture_aspect)
            .flex_shrink_0()
            .overflow_hidden()
            .rounded(px(picture_radius))
            .bg(chrome.fill_card);
        match picture {
            TilePicture::Element(element) => frame.child(div().size_full().child(element)),
            TilePicture::Placeholder(icon) => frame
                .flex()
                .items_center()
                .justify_center()
                .child(icon.size(px(20.)).text_color(theme.muted_foreground.opacity(0.6))),
        }
    });
    let fill = if tile.selected { chrome.fill_active } else { chrome.fill_row };
    div()
        .id(id)
        .size_full()
        .flex()
        .flex_col()
        .gap(px(6.))
        .p(px(padding))
        .rounded(px(geometry.radius))
        .border(px(border))
        .border_color(if tile.selected { theme.primary } else { chrome.stroke_card })
        .bg(fill)
        .when(!interactive && !tile.selected, |this| this.opacity(0.6))
        .when_some(on_pick, |this, (index, pick)| {
            this.cursor_pointer()
                .hover(|this| this.bg(chrome.fill_active))
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    pick(index, window, cx);
                })
        })
        .children(picture)
        .child(
            v_flex()
                .w_full()
                .min_w_0()
                .gap(px(2.))
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .child(tile.label),
                )
                .when_some(tile.description, |this, description| {
                    this.child(
                        div()
                            .w_full()
                            .min_w_0()
                            .text_2xs()
                            .text_color(theme.muted_foreground)
                            .child(description),
                    )
                }),
        )
}

/// The rows a grid of `count` tiles takes at `columns` per row.
pub fn choice_tile_rows(count: usize, columns: usize) -> usize {
    count.div_ceil(columns.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_geometry_is_the_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../packages/domain-contract/fixtures/plan-card.json"
        ))
        .unwrap();
        let tiles = &fixture["tiles"];
        let geometry = ChoiceTilesGeometry::default();
        assert_eq!(geometry.columns as u64, tiles["columns"].as_u64().unwrap());
        for (value, key) in [
            (geometry.gap, "gap"),
            (geometry.picture_aspect, "pictureAspect"),
            (geometry.radius, "radius"),
            (geometry.selected_ring, "selectedRing"),
            (geometry.padding, "padding"),
        ] {
            assert_eq!(value, tiles[key].as_f64().unwrap() as f32, "{key}");
        }
    }

    #[test]
    fn rows_round_up() {
        assert_eq!(choice_tile_rows(0, 2), 0);
        assert_eq!(choice_tile_rows(3, 2), 2);
        assert_eq!(choice_tile_rows(4, 2), 2);
        assert_eq!(choice_tile_rows(1, 0), 1);
    }
}
