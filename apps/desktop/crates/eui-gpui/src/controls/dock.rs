//! N-12: THE DockCard and THE Dock pile of the gpui primitive set (web
//! `@exponential_app/eui/web/primitives/dock` → `eui-react` `DockCard` /
//! `DockPile`, the same numbers): a one-row floating card (the kind glyph
//! and the tinted label, a muted caption, trailing meta, ONE title line and
//! the open chevron or a trailing action) and the pile several of them make,
//! placed like the toasts (anchored at the bottom: the front card last, the
//! older ones peeking out above it, `visible` of them drawn; expanded =
//! fanned `gap` apart). The Exponential IDE's Dock (`crates/ui/src/dock.rs`)
//! paints through these; its band, sheet, auto-open and state are its own.
//! The pile's motion (the animated top / inset / opacity) is the host's:
//! [`dock_pile_slot`] takes the values it eases.

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, Div, ElementId, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Stateful, Styled as _,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};

use crate::chrome::{composite, Chrome};

/// Tailwind `yellow-400`: a question's tint (web `text-yellow-400`).
const YELLOW: u32 = 0xfacc15;
/// Tailwind `amber-400`: a warning banner's tint (web `text-amber-400`).
const AMBER: u32 = 0xfbbf24;
/// The kind glyph and the chevron (web `size-3.5`).
const GLYPH: f32 = 14.;

/// The pile's numbers (web `DOCK_PILE_CONSTANTS`; the IDE's dock test holds
/// them equal to `fixtures/dock.json` `stack`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DockPileConstants {
    /// How far an older card peeks out above the one in front of it.
    pub peek: f32,
    /// How much narrower each rank behind the front draws.
    pub scale_step: f32,
    /// How many cards draw at all while piled.
    pub visible: usize,
    /// The space between cards while fanned.
    pub gap: f32,
}

pub const DOCK_PILE_CONSTANTS: DockPileConstants = DockPileConstants { peek: 14., scale_step: 0.05, visible: 3, gap: 6. };

/// One card's place in the pile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DockPileItem {
    /// The card's top inside the pile box.
    pub offset: f32,
    /// Its width factor (it narrows about its centre).
    pub scale: f32,
    /// Whether it draws at all.
    pub visible: bool,
}

/// The pile's geometry (web `dockPileGeometry`: the toast stack geometry
/// with the Dock's numbers, anchored at the bottom). `heights` oldest
/// first, the LAST is the front card; returns the box's height and one item
/// per height in the same order.
pub fn dock_pile_geometry(heights: &[f32], expanded: bool) -> (f32, Vec<DockPileItem>) {
    let c = DOCK_PILE_CONSTANTS;
    let n = heights.len();
    let rank = |i: usize| n - 1 - i;
    // Distance of each card's bottom edge from the box's bottom edge.
    let lead: Vec<f32> = if expanded {
        (0..n).map(|i| heights[i + 1..].iter().map(|h| h + c.gap).sum::<f32>()).collect()
    } else {
        (0..n).map(|i| c.peek * rank(i) as f32).collect()
    };
    let height = (0..n).map(|i| lead[i] + heights[i]).fold(0f32, f32::max);
    let items = (0..n)
        .map(|i| {
            let r = rank(i);
            let (scale, visible) =
                if expanded { (1.0, true) } else { (1.0 - c.scale_step * r.min(c.visible - 1) as f32, r < c.visible) };
            DockPileItem { offset: height - lead[i] - heights[i], scale, visible }
        })
        .collect();
    (height, items)
}

/// A card's tint: it sits on the glyph and the label alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockCardTint {
    Plan,
    Question,
    Muted,
    Warning,
}

impl DockCardTint {
    pub fn color(self, cx: &App) -> Hsla {
        match self {
            Self::Plan => cx.theme().primary,
            Self::Question => gpui::rgb(YELLOW).into(),
            Self::Muted => cx.theme().muted_foreground,
            Self::Warning => gpui::rgb(AMBER).into(),
        }
    }
}

/// One DockCard's facts (web `DockCardProps`).
pub struct DockCard {
    pub id: ElementId,
    pub tint: DockCardTint,
    /// The kind glyph (the card sizes and tints it).
    pub icon: Icon,
    pub label: gpui::SharedString,
    pub caption: Option<gpui::SharedString>,
    pub meta: Option<gpui::SharedString>,
    pub title: gpui::SharedString,
    /// The card opens: the trailing chevron, the pointer, the hover wash.
    /// The caller wires the click.
    pub pressable: bool,
    /// Without `pressable`: a trailing node (a banner's action).
    pub trailing: Option<AnyElement>,
    /// 0..1: a card behind the front one while piled (its chrome peeks, its
    /// words do not); a float so the host can ease it.
    pub hidden: f32,
}

/// THE DockCard (web `DockCard`): the glass card, `p-3`, the glyph + text
/// row over the header (label, caption, meta) and the title line.
pub fn dock_card(card: DockCard, cx: &App) -> Stateful<Div> {
    let chrome = Chrome::global(cx);
    let muted = cx.theme().muted_foreground;
    let color = card.tint.color(cx);
    let header = h_flex()
        .w_full()
        .min_w_0()
        .mb_1()
        .gap_2()
        .items_center()
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_xs()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(color)
                .child(card.label),
        )
        .when_some(card.caption, |header, caption| {
            header.child(div().min_w_0().truncate().text_xs().text_color(muted).child(caption))
        })
        .when_some(card.meta, |header, meta| {
            header.child(div().ml_auto().flex_shrink_0().text_xs().text_color(muted).child(meta))
        });
    let trailing = if card.pressable {
        Some(
            Icon::empty()
                .path(chrome.icons.chevron_right.clone())
                .with_size(px(GLYPH))
                .flex_shrink_0()
                .text_color(muted)
                .into_any_element(),
        )
    } else {
        card.trailing.map(|action| div().flex_shrink_0().child(action).into_any_element())
    };
    let title = h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .items_center()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_sm()
                .text_color(cx.theme().foreground.opacity(0.9))
                .child(card.title),
        )
        .children(trailing);
    let hover = composite(chrome.fill_card_opaque, chrome.fill_active);
    super::glass_card(chrome)
        .id(card.id)
        .w_full()
        .min_w_0()
        .p_3()
        .when(card.pressable, |root| root.cursor_pointer().hover(move |style| style.bg(hover)))
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .gap_2()
                .items_start()
                .opacity(1. - card.hidden)
                .child(div().flex_shrink_0().mt(px(1.)).child(card.icon.with_size(px(GLYPH)).text_color(color)))
                .child(v_flex().flex_1().min_w_0().child(header).child(title)),
        )
}

/// One card's slot in the pile box (web `DOCK_PILE_CLASSES.item`): placed
/// at `top`, narrowed by `inset` on each side (`width * (1 - scale) / 2`),
/// faded by `shown`. The host passes the eased values of a
/// [`DockPileItem`].
pub fn dock_pile_slot(top: Pixels, inset: Pixels, shown: f32) -> Div {
    div().absolute().left(inset).right(inset).top(top).opacity(shown)
}

/// The pile box (web `DOCK_PILE_CLASSES.root`): `height` tall (the
/// geometry's, eased by the host), the slots inside it.
pub fn dock_pile(id: impl Into<ElementId>, height: Pixels) -> Stateful<Div> {
    div().id(id.into()).relative().w_full().h(height)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The web's `dockPileGeometry` numbers: piled, the older cards peek 14
    /// above the front and narrow by 0.05 per rank, three drawn; fanned,
    /// every card full size 6 apart.
    #[test]
    fn the_pile_matches_the_web_geometry() {
        let (height, items) = dock_pile_geometry(&[40., 40., 40., 40.], false);
        assert_eq!(height, 40. + 14. * 3.);
        assert_eq!(items.iter().map(|i| i.offset).collect::<Vec<_>>(), vec![0., 14., 28., 42.]);
        assert_eq!(items.iter().map(|i| i.visible).collect::<Vec<_>>(), vec![false, true, true, true]);
        assert!((items[1].scale - 0.9).abs() < 1e-6 && (items[0].scale - 0.9).abs() < 1e-6 && items[3].scale == 1.);
        let (height, items) = dock_pile_geometry(&[40., 30.], true);
        assert_eq!(height, 76.);
        assert_eq!((items[0].offset, items[1].offset), (0., 46.));
        assert!(items.iter().all(|i| i.visible && i.scale == 1.));
    }
}
