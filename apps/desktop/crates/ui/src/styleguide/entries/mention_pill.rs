//! Thread e4a04d22: `mention-pill` — THE round `@member` pill the comments
//! paint, now in every chat text too (the twin of web `mention-pill.tsx`).
//! A chat line through the REAL [`MarkdownView`] at the chat rhythm: a team
//! member's `@email` paints as the pill with the member's name, beside an
//! unresolved `@x@y.com` that stays the written text. No store here, so the
//! resolver is a one-member stand-in for `RefResolver::from_store`.

use std::rc::Rc;

use gpui::{div, px, App, Div, ParentElement as _, Styled as _, Window};
use gpui_component::v_flex;

use crate::markdown::{MarkdownView, RefResolver};

pub(crate) const ID: &str = "mention-pill";
pub(crate) const OWNER: &str = "EXP-1373";

/// One member, `ada@example.com` = Ada Lovelace; nothing else resolves.
fn demo_resolver() -> RefResolver {
    RefResolver {
        member_name: Rc::new(|email, _| {
            email.eq_ignore_ascii_case("ada@example.com").then(|| "Ada Lovelace".to_string())
        }),
        issue_chip: Rc::new(|_, _| None),
        bare: true,
    }
}

pub(crate) fn render(_window: &mut Window, _cx: &mut App) -> Div {
    let line = |id: &'static str, body: &'static str| {
        MarkdownView::new(id, body).chat(true).resolver(demo_resolver())
    };
    div().w(px(420.)).child(
        v_flex()
            .gap(px(8.))
            .child(line("sg-mention-pill-resolved", "Ready for review, @ada@example.com"))
            .child(line("sg-mention-pill-unresolved", "cc @x@y.com")),
    )
}
