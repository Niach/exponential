//! EXP-1031 — `toast` (2 General components): THE toast, drawn AT REST.
//!
//! A live toast rides the window's `toast::ToastLayer` (`toast::show`), and
//! the gallery paints with no window root, so this entry draws the SAME face
//! as plain elements (`toast::card`, the layer's own face) and places the
//! stacks by `toast::stack_layout` BOTTOM-anchored, as the layer does (the
//! older cards peek ABOVE the front one) — the fixture's geometry
//! (`toast-stack.json`), locked by `toast.rs`'s tests. The width factor is
//! drawn the way the layer draws it: an inset of `width × (1 − scale) / 2`
//! on both sides.

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::toast::{self, constants, Toast, ToastKind};

pub(crate) const ID: &str = "toast";
pub(crate) const OWNER: &str = "EXP-1031";

/// The fixture's "three equal toasts" height: every card in a stack demo is
/// drawn this tall so the offsets (from the box top, oldest first) read
/// exactly 0/14/28 collapsed and 0/74/148 expanded.
const STACK_CARD_HEIGHT: f32 = 60.;

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let stack_cards = [
        Toast::info("Session started on studio-mac"),
        Toast::error("Could not merge the pull request"),
        Toast::success("Issue EXP-1031 created"),
    ];
    let kinds = ToastKind::ALL.map(|kind| {
        let title = match kind {
            ToastKind::Success => "Invite sent to ada@example.com",
            ToastKind::Error => "Could not save comment",
            ToastKind::Info => "Update ready — restart to apply",
            ToastKind::Warning => "Board limit reached",
        };
        toast::card(&Toast::new(kind, title), cx)
    });
    let with_action = Toast::success("Issue EXP-1031 created")
        .description("Filed on the Desktop board.")
        .action("Open", |_, _| {});

    v_flex()
        .gap_6()
        .child(labelled(
            "Collapsed — newest in front, 3 visible, 14 peek, −5% per rank",
            stack(&stack_cards, false, cx),
            cx,
        ))
        .child(labelled(
            "Expanded (hover) — full size, 14 apart, the clock paused",
            stack(&stack_cards, true, cx),
            cx,
        ))
        .child(labelled(
            "Kinds — the colour sits on the icon alone",
            v_flex().gap_3().children(kinds),
            cx,
        ))
        .child(labelled("Description + action", toast::card(&with_action, cx), cx))
}

/// A static stack: the cards oldest first, placed by `stack_layout`.
fn stack(cards: &[Toast], expanded: bool, cx: &App) -> Div {
    let heights = vec![STACK_CARD_HEIGHT; cards.len()];
    let (height, items) = toast::stack_layout(&heights, expanded, true);
    let front = cards.len().saturating_sub(1);
    let mut el = div().relative().w(px(constants::WIDTH)).h(px(height));
    for (ix, (card, item)) in cards.iter().zip(items).enumerate() {
        if !item.visible {
            continue;
        }
        let inset = constants::WIDTH * (1. - item.scale) / 2.;
        // Collapsed, a card behind the front shows only its edge.
        let face = if expanded || ix == front {
            toast::card(card, cx)
        } else {
            toast::card_back(card, cx)
        };
        el = el.child(
            div()
                .absolute()
                .top(px(item.offset))
                .left(px(inset))
                .right(px(inset))
                .child(face.w_full().h(px(STACK_CARD_HEIGHT))),
        );
    }
    el
}

fn labelled(caption: &'static str, body: impl gpui::IntoElement, cx: &App) -> Div {
    v_flex()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(caption),
        )
        .child(body.into_any_element())
}
