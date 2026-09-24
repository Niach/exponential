//! EXP-1019/EXP-1037 — the `composer-dialog` entry (3 Special components):
//! the LAUNCHER as a play button opens it, in a dialog window.
//!
//! What the real thing is: `chat_screen::ChatScreenView` in its `Dialog`
//! presentation, hosted by [`crate::composer_dialog`] — the headline as the
//! main element ("Run" beside an action chip, "Implement" beside the issue
//! chips), the composer card with the now-secondary text field under it, and
//! the launch options row below that.
//!
//! The demo below is a STATIC sketch of that stack: the entry table hands a
//! renderer no `Window`/`App`, so it cannot call the live recipes (they all
//! take `&App` for the theme). It paints from the very same generated design
//! tokens the recipes paint from (`theme::tokens`), and it says the same
//! words the composer says (`domain::contract::COMPOSER_UI_*`), so the copy
//! and the colours can never drift; the LIVE demo of the same stack is the
//! `native-launcher-headline` + `native-composer-card` entries of this
//! section (see [`crate::styleguide::native`]).

use gpui::{div, px, Div, FontWeight, ParentElement as _, Styled as _};
use theme::tokens as t;

pub(crate) const ID: &str = "composer-dialog";
pub(crate) const OWNER: &str = "EXP-1019";

pub(crate) fn render() -> Div {
    div()
        .flex()
        .flex_col()
        .w(px(420.))
        .gap_2()
        // The headline: the verb plus the subject's chip, the launcher's main
        // element.
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t::FOREGROUND.to_hsla())
                        .child(domain::contract::COMPOSER_UI_IMPLEMENT_HEADLINE),
                )
                .child(chip("EXP-1037")),
        )
        // The composer card: the field reads as the secondary element, with
        // the tool row and the round send under it.
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1p5()
                .w_full()
                .p_2()
                .rounded(px(t::radius::XL))
                .border_1()
                .border_color(t::glass::STROKE_CARD.to_hsla())
                .bg(t::glass::FILL_CARD.to_hsla())
                .child(
                    div()
                        .px_1()
                        .py_1()
                        .text_sm()
                        .text_color(t::MUTED_FOREGROUND.to_hsla())
                        .child(domain::contract::COMPOSER_UI_INSTRUCTIONS_PLACEHOLDER),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .px_1()
                        .child(tool())
                        .child(tool())
                        .child(div().flex_1())
                        .child(send()),
                ),
        )
        // Options row B, muted under the card.
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_1()
                .px_1()
                .text_xs()
                .text_color(t::MUTED_FOREGROUND.to_hsla())
                .child("This machine")
                .child("·")
                .child("Claude")
                .child("·")
                .child("Plan")
                .child("·")
                .child("⋯"),
        )
}

/// A subject chip's box (the real one is `issue_chip`).
fn chip(label: &'static str) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(t::size::CONTROL_SM))
        .px(px(8.))
        .rounded_full()
        .border_1()
        .border_color(t::glass::STROKE_CARD.to_hsla())
        .bg(t::glass::FILL_CARD.to_hsla())
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(t::FOREGROUND.to_hsla())
        .child(label)
}

/// One of the card's leading glyph tools (the real one is `composer_tool`).
fn tool() -> Div {
    div()
        .size(px(24.))
        .rounded_full()
        .bg(t::glass::FILL_ROW.to_hsla())
}

/// The round send (the real one is `composer_submit`).
fn send() -> Div {
    div()
        .size(px(32.))
        .rounded_full()
        .border_1()
        .border_color(t::PRIMARY.to_hsla())
        .bg(t::glass::FILL_CARD.to_hsla())
}
