//! EXP-1248 — `pr-row`: THE pull-request row, the REAL
//! [`crate::pr_rows`] (web `@exp/ui` `pr-row.tsx`, fixture `list-item.json`):
//! a PR TREE nested with tree guides, a STACK on its rail down to the base
//! branch (the current member washed, the ghost "Merge through here" pinned
//! on one row the way a hover shows it, `stack` on the top row) and an
//! unlinked PR with its external-link glyph. One line per PR, no counts, no
//! inline Merge.

use std::rc::Rc;

use gpui::{div, px, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _};

use domain::list_item::PrNodeState;

use crate::icons::registry;
use crate::pr_rows::{pr_list, pr_row, stack_rail, PrListRow, PrRowSpec, StackRailMember, StackRailSpec};

pub(crate) const ID: &str = "pr-row";
pub(crate) const OWNER: &str = "EXP-1248";

fn tree_row(key: &str, identifier: &str, title: &str, depth: usize) -> PrListRow {
    PrListRow {
        key: key.to_string(),
        identifier: Some(SharedString::from(identifier.to_string())),
        title: SharedString::from(title.to_string()),
        depth,
        node: PrNodeState::Open,
        word: None,
        active: false,
        on_open: Some(Box::new(|_, _, _| {})),
        trailing: None,
    }
}

fn member(key: &str, identifier: &str, title: &str, current: bool) -> StackRailMember {
    StackRailMember {
        key: key.to_string(),
        identifier: SharedString::from(identifier.to_string()),
        title: SharedString::from(title.to_string()),
        current,
        on_open: Some(Box::new(|_, _, _| {})),
        trailing: None,
    }
}

pub(crate) fn render(_window: &mut Window, cx: &mut App) -> Div {
    let muted = cx.theme().muted_foreground;
    let tree = pr_list(
        "styleguide-pr-tree",
        vec![
            tree_row("e1250", "EXP-1250", "opening tabs closes all at some point", 0),
            tree_row("e1252", "EXP-1252", "tabs per team on desktop", 1),
            tree_row("e1253", "EXP-1253", "inbox stepping reuses one tab", 1),
        ],
        cx,
    );
    let stack = stack_rail(
        StackRailSpec {
            id_prefix: SharedString::from("styleguide-pr-stack"),
            members: vec![
                member("v100", "VAPP-100", "SwiftUI parity with the round-1/2 contract", false),
                member("v98", "VAPP-98", "Exponential UI renderer hardening round 1", false),
                member("v91", "VAPP-91", "Exponential UI host API ×4", true),
                member("v88", "VAPP-88", "Exponential UI SwiftUI renderer", false),
            ],
            base_branch: SharedString::from("master"),
            word: Some(SharedString::from(crate::reviews_view::STACK_WORD)),
            // The styleguide pins one row's hover, so the ghost shows.
            hovered: Some("v98".to_string()),
            on_hover: None,
            on_merge_through: Some(Rc::new(|_, _, _| {})),
        },
        cx,
    );
    let unlinked = pr_row(
        PrRowSpec {
            identifier: Some(SharedString::from("#1013")),
            trailing: Some(
                Icon::new(registry::UI_EXTERNAL_LINK)
                    .xsmall()
                    .text_color(muted)
                    .into_any_element(),
            ),
            on_click: Some(Box::new(|_, _, _| {})),
            ..PrRowSpec::open(
                "styleguide-pr-unlinked",
                "Dockerfiles: stub the exponential-ui workspace packages",
            )
        },
        cx,
    );
    div()
        .flex()
        .flex_col()
        .w(px(640.))
        .gap_4()
        .child(tree)
        .child(stack)
        .child(unlinked)
}
