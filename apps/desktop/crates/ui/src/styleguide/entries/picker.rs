//! EXP-1021 — the `picker` styleguide entry: THE picker primitive itself,
//! its two surfaces and its selection language.
//!
//! This file also holds the small demo kit the ten TYPED picker entries
//! share (`demo`, `chip`, the fixture rows) — an entry's `render` is a bare
//! `fn() -> Div` with no window and no theme handle, and a picker needs
//! both, so a demo builds its element at PAINT time through [`PickerDemo`].
//! Filled by EXP-1021; no entry file edits `entries/mod.rs` nor the index.

use std::rc::Rc;

use gpui::{
    div, px, AnyElement, App, Div, IntoElement, ParentElement as _, RenderOnce, SharedString,
    Styled as _, Window,
};
use gpui_component::{v_flex, ActiveTheme as _};

use crate::picker::{OnPickerChange, Picker, PickerItem, PickerMode};

pub(crate) const ID: &str = "picker";
pub(crate) const OWNER: &str = "EXP-1021";

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

pub(crate) fn render() -> Div {
    column(vec![
        demo(
            "single, short — the PopupMenu surface; the picked row wears the check",
            |window, cx| {
                Picker::single(
                    vec![
                        PickerItem::new("a".to_string(), "Alpha"),
                        PickerItem::new("b".to_string(), "Beta").description("second"),
                        PickerItem::new("c".to_string(), "Gamma").disabled(true),
                    ],
                    Some("a".to_string()),
                    chip("sg-picker-single", "Alpha", cx),
                    inert(),
                )
                .id("sg-picker-single-surface")
                .render(window, cx)
            },
        ),
        demo("search — the filter field over the rows", |window, cx| {
            Picker::single(
                vec![
                    PickerItem::new("a".to_string(), "Alpha").keywords(vec!["APP-1".into()]),
                    PickerItem::new("b".to_string(), "Beta").keywords(vec!["APP-2".into()]),
                ],
                None,
                chip("sg-picker-search", "Search…", cx),
                inert(),
            )
            .search(true)
            .empty_text("No options")
            .id("sg-picker-search-surface")
            .render(window, cx)
        }),
        demo(
            "multi — a picked row IS its highlight (fill + active stroke), never a circle",
            |window, cx| {
                Picker::multi(
                    vec![
                        PickerItem::new("a".to_string(), "Alpha"),
                        PickerItem::new("b".to_string(), "Beta"),
                        PickerItem::new("c".to_string(), "Gamma"),
                    ],
                    vec!["a".to_string(), "c".to_string()],
                    chip("sg-picker-multi", "Alpha, Gamma", cx),
                    inert(),
                )
                .search(true)
                .id("sg-picker-multi-surface")
                .render(window, cx)
            },
        ),
        demo("empty — one copy for no rows and for no matches", |window, cx| {
            Picker::<String>::single(
                vec![],
                None,
                chip("sg-picker-empty", "Nothing", cx),
                inert(),
            )
            .search(true)
            .empty_text("No boards")
            .id("sg-picker-empty-surface")
            .render(window, cx)
        }),
        demo("disabled — the trigger, inert; no surface is mounted", |window, cx| {
            Picker::single(
                vec![PickerItem::new("a".to_string(), "Alpha")],
                None,
                chip("sg-picker-disabled", "Unavailable", cx),
                inert(),
            )
            .disabled(true)
            .id("sg-picker-disabled-surface")
            .render(window, cx)
        }),
    ])
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
