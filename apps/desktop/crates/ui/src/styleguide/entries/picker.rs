//! EXP-1021 — the small demo kit the TYPED picker entries share (`demo`,
//! `chip`, the fixture rows): each demo cell builds its live picker at PAINT
//! time through [`PickerDemo`], under its caption. Not an entry itself since
//! VAPP-93: the generic picker primitive (the `picker` entry) moved to
//! ui.exponential.at with the rest of the core catalog's specimens.

use std::rc::Rc;

use gpui::{
    div, px, AnyElement, App, Div, IntoElement, ParentElement as _, RenderOnce, SharedString,
    Styled as _, Window,
};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::picker::{OnPickerChange, PickerMode};

/// One demo cell: a muted caption over a LIVE element the entry builds at
/// paint time.
#[derive(IntoElement)]
pub(crate) struct PickerDemo {
    caption: SharedString,
    build: Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>,
}

impl RenderOnce for PickerDemo {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let element = (self.build)(window, cx);
        v_flex()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.caption),
            )
            .child(element)
    }
}

/// A demo cell (the ten typed entries build theirs through this).
pub(crate) fn demo(
    caption: impl Into<SharedString>,
    build: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
) -> PickerDemo {
    PickerDemo {
        caption: caption.into(),
        build: Rc::new(build),
    }
}

/// The demo column every entry returns: its cells, spaced.
pub(crate) fn column(cells: Vec<PickerDemo>) -> Div {
    let mut column = div().flex().flex_col().gap_4().w(px(320.));
    for cell in cells {
        column = column.child(cell);
    }
    column
}

/// THE trigger a picker demo opens from — the shared chip every property
/// row already uses, so the styleguide shows the real pairing.
pub(crate) fn chip(id: &'static str, label: impl Into<SharedString>, cx: &App) -> AnyElement {
    crate::pickers::chip_button(id, cx)
        .label(label.into())
        .into_any_element()
}

/// A demo picker never writes anything.
pub(crate) fn inert<T: Clone>() -> OnPickerChange<T> {
    Rc::new(|_, _, _| {})
}

/// The demo rows the typed entries share.
pub(crate) fn demo_boards() -> Vec<domain::rows::Board> {
    vec![
        domain::rows::Board::seeded("board-1", "team-1", "Mobile Bugs"),
        domain::rows::Board::seeded("board-2", "team-1", "Platform"),
    ]
}

pub(crate) fn demo_issues() -> Vec<domain::rows::Issue> {
    ["EXP-1021", "EXP-1045"]
        .into_iter()
        .enumerate()
        .map(|(ix, identifier)| {
            serde_json::from_value(serde_json::json!({
                "id": format!("issue-{ix}"),
                "board_id": "board-1",
                "number": ix + 1,
                "identifier": identifier,
                "title": "One picker everywhere",
                "status": "backlog",
            }))
            .expect("demo issue")
        })
        .collect()
}

pub(crate) fn demo_labels() -> Vec<domain::rows::Label> {
    [("label-1", "bug", "#ef4444"), ("label-2", "design", "#8b5cf6")]
        .into_iter()
        .map(|(id, name, color)| {
            serde_json::from_value(serde_json::json!({
                "id": id, "team_id": "team-1", "name": name, "color": color,
            }))
            .expect("demo label")
        })
        .collect()
}

pub(crate) fn demo_members() -> Vec<domain::rows::User> {
    [("user-1", "Ada Lovelace", "ada@example.com"), ("user-2", "Grace Hopper", "grace@example.com")]
        .into_iter()
        .map(|(id, name, email)| {
            serde_json::from_value(serde_json::json!({
                "id": id, "name": name, "email": email,
            }))
            .expect("demo member")
        })
        .collect()
}

/// A demo picker's mode, spelled out so each typed entry reads it.
pub(crate) const SINGLE: PickerMode = PickerMode::Single;
