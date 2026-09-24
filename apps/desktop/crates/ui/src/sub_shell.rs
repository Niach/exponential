//! EXP-1029 contract, implemented by EXP-1020 — sub-shell navigation for the
//! settings shell (`surface::glass_group` rows, EXP-994). The device settings
//! dialog and the Agents pane use it for "Workflow settings".
//!
//! A sub-shell is a ROW ENTRY inside a card. Opening it slides a child page
//! in place of the WHOLE card — not a nested card, not a dialog — with a
//! back button on top (`controls::back_glyph`); the child page is the same
//! shell (its own groups of rows), so a sub-shell may hold another
//! sub-shell. [`SubShellHost`] is the card boundary the page replaces: it
//! renders its card at rest and, once a row inside opened, that row's page.
//!
//! gpui retains no element state between frames, so the page stack lives on
//! the OWNING view as a [`SubShellNav`]: the row's click pushes a title onto
//! it, the host reads it back and renders the matching page, and the back
//! glyph pops one level. That is also what makes the behaviour testable
//! without a window — [`SubShellNav`] is a plain struct.
//!
//! Siblings (same names): web `packages/ui/src/sub-shell.tsx`, iOS
//! `ExpUI/Sources/SubShell.swift`, Android `ui/components/SubShell.kt`.
#![allow(dead_code)]

use gpui::{
    div, prelude::FluentBuilder as _, px, AnyElement, App, ClickEvent, Div, ElementId,
    InteractiveElement as _, IntoElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon};
use theme::tokens as t;

use crate::surface::{glass_row_shell, ROW_DESCRIPTION_MAX_W};

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

impl SubShellProps {
    pub(crate) fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            icon: None,
            value: None,
            title: None,
            disabled: false,
        }
    }

    pub(crate) fn value(mut self, value: impl Into<SharedString>) -> Self {
        self.value = Some(value.into());
        self
    }

    pub(crate) fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub(crate) fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub(crate) fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// What the page the row opens is titled.
    pub(crate) fn page_title(&self) -> SharedString {
        self.title.clone().unwrap_or_else(|| self.label.clone())
    }

    /// Whether clicking this row opens its page. A disabled row takes NO
    /// pointer — [`sub_shell_row`] gates the click handler on this — so it
    /// can never reach the owning view's [`SubShellNav`].
    pub(crate) fn opens(&self) -> bool {
        !self.disabled
    }
}

/// The page a row opens: the same shell — groups of rows.
pub(crate) struct SubShellPage {
    pub title: SharedString,
    pub content: AnyElement,
}

impl SubShellPage {
    pub(crate) fn new(title: impl Into<SharedString>, content: AnyElement) -> Self {
        Self {
            title: title.into(),
            content,
        }
    }
}

/// The page stack of ONE shell, held by the view that owns the card.
///
/// A title is pushed per open page, so the host knows both HOW DEEP it is
/// and what the current page is called. Nothing renders while the stack is
/// empty beyond the card itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SubShellNav {
    stack: Vec<SharedString>,
}

impl SubShellNav {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Whether a page is open right now.
    pub(crate) fn is_open(&self) -> bool {
        !self.stack.is_empty()
    }

    /// How many pages deep the shell is (0 = the card).
    pub(crate) fn depth(&self) -> usize {
        self.stack.len()
    }

    /// The open page's title, if any.
    pub(crate) fn current(&self) -> Option<&SharedString> {
        self.stack.last()
    }

    /// Slides one page deeper. A disabled row never calls this.
    pub(crate) fn open(&mut self, title: impl Into<SharedString>) {
        self.stack.push(title.into());
    }

    /// Returns ONE level: to the enclosing page, or to the card.
    pub(crate) fn back(&mut self) {
        self.stack.pop();
    }

    /// Back to the card, whatever the depth — what closing the dialog does.
    pub(crate) fn close(&mut self) {
        self.stack.clear();
    }

    /// Whether the page at `depth` (1 = the card's own page) is the one on
    /// screen. A page renders its rows only while it is the deepest: a
    /// sub-shell inside it hides them, its back header included.
    pub(crate) fn showing(&self, depth: usize) -> bool {
        self.depth() == depth
    }
}

/// The card boundary a sub-shell page replaces: the card at rest, the open
/// page instead of it.
pub(crate) struct SubShellHost {
    card: Div,
    page: Option<SubShellPage>,
    #[allow(clippy::type_complexity)]
    on_back: Option<Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>>,
}

impl SubShellHost {
    pub(crate) fn new(card: Div) -> Self {
        Self {
            card,
            page: None,
            on_back: None,
        }
    }

    /// Slides `page` in place of the card. `on_back` pops the owning view's
    /// [`SubShellNav`] one level.
    pub(crate) fn open(
        mut self,
        page: SubShellPage,
        on_back: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.page = Some(page);
        self.on_back = Some(Box::new(on_back));
        self
    }

    /// Whether a page is open right now.
    pub(crate) fn is_open(&self) -> bool {
        self.page.is_some()
    }

    pub(crate) fn render(self, _window: &mut Window, cx: &mut App) -> AnyElement {
        let Some(page) = self.page else {
            return self.card.into_any_element();
        };
        let on_back = self.on_back;
        let foreground = cx.theme().foreground;
        v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .child(
                h_flex()
                    .id("sub-shell-back")
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .text_sm()
                    .text_color(foreground.opacity(0.7))
                    .hover(|style| style.text_color(foreground))
                    .when_some(on_back, |row, on_back| {
                        row.on_click(move |event, window, cx| on_back(event, window, cx))
                    })
                    .child(crate::controls::back_glyph())
                    .child(div().min_w_0().truncate().child(page.title)),
            )
            .child(page.content)
            .into_any_element()
    }
}

/// A row entry that slides its child page in place of the whole card.
///
/// `on_open` pushes the page title onto the owning view's [`SubShellNav`];
/// a disabled row never fires it and never takes the pointer.
pub(crate) fn sub_shell_row(
    props: SubShellProps,
    on_open: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    let foreground = cx.theme().foreground;
    let opens = props.opens();
    let disabled = props.disabled;
    let mut text = v_flex()
        .min_w_0()
        .gap_0p5()
        .text_sm()
        .text_color(foreground)
        .child(div().child(props.label));
    if let Some(description) = props.description {
        text = text.child(
            div()
                .max_w(px(ROW_DESCRIPTION_MAX_W))
                .text_xs()
                .text_color(foreground.opacity(0.5))
                .child(description),
        );
    }
    let row = glass_row_shell()
        .id(props.id)
        .when(opens, |row| {
            row.cursor_pointer()
                .hover(|style| style.bg(t::glass::FILL_ACTIVE.to_hsla()))
                .on_click(move |event, window, cx| on_open(event, window, cx))
        })
        .when(disabled, |row| row.opacity(0.5))
        .child(text)
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .items_center()
                .justify_end()
                .gap_2()
                .text_sm()
                .text_color(foreground.opacity(0.7))
                .children(props.value.map(|value| div().min_w_0().truncate().child(value)))
                // EXP-870: the `ui-chevron-right` CONCEPT — the row's "there
                // is a page behind this" glyph; the page opens with
                // `controls::back_glyph`.
                .child(Icon::from(crate::icons::registry::UI_CHEVRON_RIGHT)),
        );
    // `glass_group_rows` stacks plain `Div`s; the interactive row is the
    // whole shell, so the wrapper carries nothing but the divider.
    div().w_full().min_w_0().child(row)
}

#[cfg(test)]
mod tests {
    use super::{SubShellNav, SubShellProps};

    #[test]
    fn clicking_the_row_slides_the_child_page_in_place_of_the_whole_card() {
        let mut nav = SubShellNav::new();
        assert!(!nav.is_open());
        assert!(nav.showing(0), "the card is what shows at rest");
        nav.open("Workflow settings");
        assert!(nav.is_open());
        assert_eq!(nav.current().map(|title| title.as_ref()), Some("Workflow settings"));
        // The card is no longer what shows: the page replaced the WHOLE card.
        assert!(!nav.showing(0));
        assert!(nav.showing(1));
    }

    #[test]
    fn the_child_page_carries_a_back_button_on_top_that_returns_to_the_card() {
        let mut nav = SubShellNav::new();
        nav.open("Workflow settings");
        nav.back();
        assert!(!nav.is_open());
        assert_eq!(nav.current(), None);
        assert!(nav.showing(0));
    }

    #[test]
    fn a_sub_shell_inside_the_child_page_slides_one_level_deeper() {
        let mut nav = SubShellNav::new();
        nav.open("Workflow settings");
        nav.open("Advanced");
        assert_eq!(nav.depth(), 2);
        assert_eq!(nav.current().map(|title| title.as_ref()), Some("Advanced"));
        // The deeper page hides its parent's rows AND its parent's header.
        assert!(!nav.showing(1));
        assert!(nav.showing(2));
        // Back returns exactly ONE level.
        nav.back();
        assert_eq!(nav.depth(), 1);
        assert_eq!(nav.current().map(|title| title.as_ref()), Some("Workflow settings"));
        nav.back();
        assert!(!nav.is_open());
    }

    #[test]
    fn a_disabled_row_never_opens() {
        // `opens()` IS the production predicate `sub_shell_row` gates its
        // click handler on, so this pins the real guard rather than the
        // test's own `if`.
        let enabled = SubShellProps::new("row", "Workflow settings");
        assert!(enabled.opens());
        let disabled = SubShellProps::new("row", "Workflow settings").disabled(true);
        assert!(!disabled.opens());

        // What the row does with it: no pointer, so the stack never moves.
        let mut nav = SubShellNav::new();
        for props in [&disabled, &enabled] {
            if props.opens() {
                nav.open(props.page_title());
            }
        }
        assert_eq!(nav.depth(), 1, "only the enabled row opened");
        assert_eq!(
            nav.current().map(|title| title.as_ref()),
            Some("Workflow settings")
        );
    }

    #[test]
    fn closing_the_dialog_returns_to_the_card_whatever_the_depth() {
        let mut nav = SubShellNav::new();
        nav.open("Workflow settings");
        nav.open("Advanced");
        nav.close();
        assert!(!nav.is_open());
        assert_eq!(nav.depth(), 0);
    }
}
