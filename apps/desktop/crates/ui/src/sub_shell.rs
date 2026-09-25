//! EXP-1029 contract — sub-shell navigation for the settings shell
//! (`surface::glass_group` rows, EXP-994). EXP-1020 implements it and uses
//! it for "Workflow settings" inside the Agents pane.
//!
//! A sub-shell is a ROW ENTRY inside a card. Opening it slides a child page
//! in place of the WHOLE card — not a nested card, not a dialog — with a
//! back button on top (`controls::back_glyph`); the child page is the same
//! shell (its own groups of rows), so a sub-shell may hold another
//! sub-shell. [`SubShellHost`] is the card boundary the page replaces: it
//! renders its card at rest and, once a row inside opened, that row's page.
//!
//! Siblings (same names): web `packages/ui/src/sub-shell.tsx`, iOS
//! `ExpUI/Sources/SubShell.swift`, Android `ui/components/SubShell.kt`.
//!
//! This file is the CONTRACT: the props, a host that only ever renders its
//! card, and a row stub that never opens. The ignored tests are the
//! behaviour EXP-1020 un-ignores (same case names as the web table).
#![allow(dead_code)]

use gpui::{
    div, AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement as _,
    ParentElement as _, SharedString, Styled as _, Window,
};
use gpui_component::Icon;

use crate::surface::glass_row_shell;

/// One sub-shell row and its page.
pub(crate) struct SubShellProps {
    pub id: ElementId,
    /// The row's label.
    pub label: SharedString,
    /// A muted second line under the label.
    pub description: Option<SharedString>,
    /// A leading registry glyph.
    pub icon: Option<Icon>,
    /// A muted trailing summary (`opus · fable`).
    pub value: Option<SharedString>,
    /// The child page's title; defaults to `label`.
    pub title: Option<SharedString>,
    pub disabled: bool,
}

/// The page a row opens: the same shell — groups of rows.
pub(crate) struct SubShellPage {
    pub title: SharedString,
    pub content: AnyElement,
}

/// The card boundary a sub-shell page replaces. Contract stub: renders the
/// card only (EXP-1020 keeps the page stack on the host).
pub(crate) struct SubShellHost {
    pub card: Div,
}

impl SubShellHost {
    pub(crate) fn new(card: Div) -> Self {
        Self { card }
    }

    /// Whether a page is open right now (never, in the stub).
    pub(crate) fn is_open(&self) -> bool {
        false
    }

    /// Slides `page` in place of the card. Stub: drops it.
    pub(crate) fn open(&mut self, page: SubShellPage, _window: &mut Window, _cx: &mut App) {
        drop(page);
    }

    /// Returns to the card (or the enclosing page). Stub: nothing to close.
    pub(crate) fn back(&mut self, _window: &mut Window, _cx: &mut App) {}

    pub(crate) fn render(self, _window: &mut Window, _cx: &mut App) -> AnyElement {
        self.card.into_any_element()
    }
}

/// A row entry that slides its child page in place of the whole card.
/// Contract stub: the row (label, description, value, chevron) on the glass
/// row shell; opening does nothing until EXP-1020.
pub(crate) fn sub_shell_row(props: SubShellProps, _window: &mut Window, _cx: &mut App) -> AnyElement {
    let mut row = glass_row_shell().id(props.id);
    if let Some(icon) = props.icon {
        row = row.child(icon);
    }
    let mut text = div().flex().flex_col().flex_1().min_w_0().child(props.label);
    if let Some(description) = props.description {
        text = text.child(description);
    }
    row = row.child(text);
    if let Some(value) = props.value {
        row = row.child(value);
    }
    // EXP-870: the `ui-chevron-right` CONCEPT — the row's "there is a page
    // behind this" glyph; the page itself opens with `controls::back_glyph`.
    row.child(Icon::from(crate::icons::registry::UI_CHEVRON_RIGHT))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "EXP-1029 contract: EXP-1020 implements sub-shell navigation"]
    fn clicking_the_row_slides_the_child_page_in_place_of_the_whole_card() {
        unimplemented!("EXP-1020")
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1020 implements sub-shell navigation"]
    fn the_child_page_carries_a_back_button_on_top_that_returns_to_the_card() {
        unimplemented!("EXP-1020")
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1020 implements sub-shell navigation"]
    fn a_sub_shell_inside_the_child_page_slides_one_level_deeper() {
        unimplemented!("EXP-1020")
    }

    #[test]
    #[ignore = "EXP-1029 contract: EXP-1020 implements sub-shell navigation"]
    fn a_disabled_row_never_opens() {
        unimplemented!("EXP-1020")
    }
}
