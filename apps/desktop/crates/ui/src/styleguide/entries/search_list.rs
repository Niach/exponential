//! SLOP-28 (EXP-1259 D4): `search-list` (General components): THE search list,
//! the IDE half of the entry the web page draws — the REAL pieces
//! (`crate::picker::search_list`) twice: the `#` typeahead's popover (its
//! `popoverWidth`, the field prefilled with the token after the `#`, more
//! than eight hits so the rows scroll under the bottom fade), the issue
//! pick (the picks with their check) and the "+" menu's Message an agent ›
//! list (an agent's icon and its name, nothing else). The live popover hangs off the caret
//! (`typeahead_popover`); the specimens draw its surface in place.

use gpui::{
    div, px, AnyElement, App, Div, IntoElement as _, ParentElement as _,
    ScrollHandle, SharedString, Styled as _, Window,
};
use gpui_component::{input::InputState, ActiveTheme as _, Icon, Sizable as _};

use crate::icons::{registry, ExpIcon};
use crate::picker::search_list::{
    action_row_body, issue_row_body, row, row_check, surface, SearchList, ISSUE_SEARCH_PLACEHOLDER,
    MENU_LIST_WIDTH, POPOVER_WIDTH,
};

pub(crate) const ID: &str = "search-list";
pub(crate) const OWNER: &str = "SLOP-28";

/// The demo issues: (identifier, title, status glyph).
const ISSUES: [(&str, &str, ExpIcon); 10] = [
    ("EXP-1259", "UX batch 2: home, tabs, search, getting started", registry::STATUS_IN_PROGRESS),
    ("EXP-1255", "Tab close on hover, a New session tab", registry::STATUS_IN_REVIEW),
    ("EXP-1253", "The Agent page is home", registry::STATUS_IN_PROGRESS),
    ("EXP-1249", "One \"+\" menu in the composer", registry::STATUS_BACKLOG),
    ("EXP-1243", "Motion pass over every component", registry::STATUS_BACKLOG),
    ("EXP-1241", "A comment call renders the real comment", registry::STATUS_IN_REVIEW),
    ("EXP-1240", "One Work card for workflows and task lists", registry::STATUS_IN_PROGRESS),
    ("SLOP-28", "Side panel search for # references", registry::STATUS_BACKLOG),
    ("EXP-1270", "Getting started as one row of cards", registry::STATUS_BACKLOG),
    ("EXP-1271", "Remote starts never take the window", registry::STATUS_BACKLOG),
];

fn issue_rows(prefix: &str, count: usize, picked: &[usize], cx: &App) -> Vec<AnyElement> {
    ISSUES
        .iter()
        .take(count)
        .enumerate()
        .map(|(index, (identifier, title, glyph))| {
            let trailing = picked.contains(&index).then(|| row_check(cx));
            row(SharedString::from(format!("{prefix}-{index}")), index == 0, cx)
                .child(issue_row_body(
                    Some(
                        Icon::new(glyph.clone())
                            .xsmall()
                            .text_color(cx.theme().muted_foreground)
                            .into_any_element(),
                    ),
                    *identifier,
                    *title,
                    trailing,
                    cx,
                ))
                .into_any_element()
        })
        .collect()
}

fn specimen(
    key: &'static str,
    prefill: &'static str,
    width: f32,
    count: usize,
    picked: &[usize],
    caption: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let input = window.use_keyed_state(SharedString::from(format!("styleguide-{key}-input")), cx, |window, cx| {
        InputState::new(window, cx)
            .placeholder(ISSUE_SEARCH_PLACEHOLDER)
            .default_value(prefill)
    });
    let scroll = window
        .use_keyed_state(SharedString::from(format!("styleguide-{key}-scroll")), cx, |_, _| ScrollHandle::new())
        .read(cx)
        .clone();
    let rows = issue_rows(key, count, picked, cx);
    let list = SearchList::new(SharedString::from(format!("styleguide-{key}-rows")), &input, &scroll)
        .rows(rows, count)
        .render(cx);
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(surface(cx).w(px(width)).child(list))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(caption),
        )
}

/// Message an agent ›: the agent's icon and its name, nothing else.
fn agent_specimen(window: &mut Window, cx: &mut App) -> Div {
    let input = window.use_keyed_state("styleguide-search-list-agents-input", cx, |window, cx| {
        InputState::new(window, cx).placeholder(super::menu_submenu::SEARCH_AGENTS)
    });
    let scroll = window
        .use_keyed_state("styleguide-search-list-agents-scroll", cx, |_, _| ScrollHandle::new())
        .read(cx)
        .clone();
    let rows: Vec<AnyElement> = super::menu_submenu::AGENTS
        .iter()
        .enumerate()
        .map(|(index, (name, icon))| {
            row(SharedString::from(format!("styleguide-search-list-agent-{index}")), index == 0, cx)
                .child(action_row_body(crate::icons::action_icon(Some(icon)), *name, cx))
                .into_any_element()
        })
        .collect();
    let count = rows.len();
    let list = SearchList::new("styleguide-search-list-agents-rows", &input, &scroll)
        .rows(rows, count)
        .count(None)
        .render(cx);
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(surface(cx).w(px(MENU_LIST_WIDTH)).child(list))
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Message an agent ›: the agent's icon and its name, nothing else."),
        )
}

pub(crate) fn render(window: &mut Window, cx: &mut App) -> Div {
    let agents = agent_specimen(window, cx);
    div()
        .flex()
        .flex_col()
        .gap_6()
        .pt_2()
        .child(specimen(
            "search-list-typeahead",
            "sid",
            POPOVER_WIDTH,
            ISSUES.len(),
            &[],
            "The # typeahead: its own field, prefilled with the token",
            window,
            cx,
        ))
        .child(specimen(
            "search-list-picker",
            "",
            480.,
            4,
            &[0, 2],
            "An issue pick: the picks pinned first, checked",
            window,
            cx,
        ))
        .child(agents)
}
