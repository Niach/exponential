//! EXP-736 — the issue-detail RELATIONS card (web parity target:
//! `apps/web/src/components/issue-relations-card.tsx`).
//!
//! It sits directly under the header's chip tray, above the description: a
//! glass card with the "Relations" heading, an "Add relation" chip and the
//! related issues grouped by their per-side label ("Parent of", "Blocked
//! by", …).
//!
//! Reads are pure derivations over the synced `issue_relations` +`issues`
//! collections (§4.1) — [`sync::Collections::relations_for_issue`] returns
//! every row touching this issue from EITHER side, and the side decides which
//! label the row wears (`domain::relations::label`). A row whose OTHER issue
//! has not synced (its board is trashed, or it belongs to a team this device
//! left) is HIDDEN rather than rendered as a dangling id — the shape scopes
//! rows by the SOURCE issue's board, so the pairing is not guaranteed.
//!
//! Writes go through `relations.create` / `relations.delete`, except the
//! "Duplicate of" pick: duplicates are DUAL-WRITTEN with `issues.duplicate_of_id`
//! server-side, so that pick opens the existing duplicate picker
//! ([`crate::issue_detail::open_duplicate_picker`]) and the mirror row
//! follows.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    div, px, AnyElement, App, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::ButtonVariants as _,
    h_flex,
    menu::{DropdownMenu as _, PopupMenuItem},
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use domain::relations::{RelationPick, RELATION_PICKS};
use domain::rows::Issue;

use crate::icons::{registry, ExpIcon};
use crate::issue_detail::{open_duplicate_picker, open_issue_picker, DETAIL_GUTTER};
use crate::navigation::{navigate, Screen};
use crate::queries;

/// The row's hover group (web `group/relation-row`) — reveals the remove
/// button. Reused per row: gpui resolves `group_hover` against the innermost
/// enclosing group with the name.
const ROW_GROUP: &str = "relation-row";

/// One rendered relation: the row's id (for the delete) and the issue on the
/// other side.
struct RelationEntry {
    id: String,
    other: Issue,
}

/// One label group of the card ("Blocked by", "Sub-issue of", …).
struct RelationGroup {
    /// Sort key: the forward pick's menu position, inverse side second.
    order: usize,
    label: String,
    icon: ExpIcon,
    entries: Vec<RelationEntry>,
}

/// The card, or `None` when there is nothing to show: no rows AND no
/// signed-in account to add one with (a read-only viewer gets no empty
/// affordance, web parity).
pub(crate) fn render_relations_card(issue: &Issue, cx: &App) -> Option<AnyElement> {
    let groups = relation_groups(issue, cx);
    let can_write = queries::active_account(cx).is_some();
    if groups.is_empty() && !can_write {
        return None;
    }

    let mut card = crate::surface::glass_card()
        .w_full()
        .gap_1p5()
        .px_3()
        .py_2p5()
        .child(header_row(issue, can_write, cx));
    for group in groups {
        card = card.child(render_group(group, cx));
    }
    Some(
        div()
            .w_full()
            .px(px(DETAIL_GUTTER))
            .pb_2()
            .child(card)
            .into_any_element(),
    )
}

/// Heading + the "Add relation" chip (the chip is the only write affordance;
/// removes ride the rows' hover buttons).
fn header_row(issue: &Issue, can_write: bool, cx: &App) -> impl IntoElement {
    let issue_id = issue.id.clone();
    h_flex()
        .w_full()
        .items_center()
        .gap_1p5()
        .child(
            Icon::new(registry::RELATION_SECTION)
                .xsmall()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(cx.theme().foreground.opacity(0.7))
                .child("Relations"),
        )
        .child(div().flex_1().min_w_0())
        .when(can_write, move |row| {
            row.child(add_relation_chip(issue_id, cx))
        })
}

/// The "Add relation" chip: a popup menu of the six picks, each opening the
/// shared issue picker for its `(type, inverse)` pair.
fn add_relation_chip(issue_id: String, cx: &App) -> impl IntoElement {
    crate::pickers::chip_button("relations-add", cx)
        .icon(Icon::new(registry::UI_ADD).xsmall())
        .child(crate::pickers::chip_label("Add relation", false, cx))
        .dropdown_menu(move |mut menu, _window, _cx| {
            for pick in RELATION_PICKS {
                let issue_id = issue_id.clone();
                menu = menu.item(
                    PopupMenuItem::new(pick.label)
                        .icon(Icon::new(pick_icon(&pick)))
                        .on_click(move |_, window, cx| {
                            open_relation_target_picker(issue_id.clone(), pick, window, cx);
                        }),
                );
            }
            menu
        })
}

/// Stage two of a pick: choose the issue on the other side. "Duplicate of"
/// takes the duplicate picker instead — the duplicate relation is the mirror
/// of `issues.duplicate_of_id` and only `issues.update` writes both halves.
fn open_relation_target_picker(
    issue_id: String,
    pick: RelationPick,
    window: &mut Window,
    cx: &mut App,
) {
    if pick.kind == domain::contract::ISSUE_RELATION_TYPE_DUPLICATE && !pick.inverse {
        open_duplicate_picker(issue_id, window, cx);
        return;
    }
    let source_id = issue_id.clone();
    open_issue_picker(
        issue_id,
        format!("{} …", pick.label),
        "Search issues…",
        std::rc::Rc::new(move |related_issue_id: String, _window: &mut Window, cx: &mut App| {
            spawn_relation_create(cx, source_id.clone(), related_issue_id, pick);
        }),
        window,
        cx,
    );
}

fn spawn_relation_create(
    cx: &mut App,
    issue_id: String,
    related_issue_id: String,
    pick: RelationPick,
) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] relations.create skipped: no signed-in account");
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::relations::relations_create(
                &trpc,
                &issue_id,
                &related_issue_id,
                pick.kind,
                pick.inverse,
            ) {
                log::warn!("[ui] relations.create({}) failed: {err}", pick.kind);
            }
        })
        .detach();
}

fn spawn_relation_delete(cx: &mut App, relation_id: String) {
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] relations.delete skipped: no signed-in account");
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::relations::relations_delete(&trpc, &relation_id) {
                log::warn!("[ui] relations.delete({relation_id}) failed: {err}");
            }
        })
        .detach();
}

fn render_group(group: RelationGroup, cx: &App) -> impl IntoElement {
    let mut column = v_flex().w_full().gap_0p5().child(
        h_flex()
            .items_center()
            .gap_1p5()
            .child(
                Icon::new(group.icon)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(group.label)),
            ),
    );
    for entry in group.entries {
        column = column.child(render_row(entry, cx));
    }
    column
}

fn render_row(entry: RelationEntry, cx: &App) -> impl IntoElement {
    let status = queries::resolve_issue_status(cx, &entry.other);
    let issue_id = entry.other.id.clone();
    let relation_id = entry.id.clone();
    h_flex()
        .id(ElementId::from(SharedString::from(format!(
            "relation-row-{}",
            entry.id
        ))))
        .group(ROW_GROUP)
        .w_full()
        .items_center()
        .gap_2()
        .px_1p5()
        .py_1()
        .rounded(cx.theme().radius)
        .cursor_pointer()
        .hover(|style| style.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
        .on_click(move |_, window, cx| {
            navigate(
                window,
                cx,
                Screen::IssueDetail {
                    issue_id: issue_id.clone(),
                },
            );
        })
        .child(
            crate::icons::resolved_status_icon(&status, cx)
                .xsmall()
                .flex_shrink_0(),
        )
        .child(
            div()
                .flex_shrink_0()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .font_family(theme::terminal::FONT_FAMILY)
                .child(SharedString::from(entry.other.identifier.clone())),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_sm()
                .whitespace_nowrap()
                .overflow_hidden()
                .text_ellipsis()
                .child(SharedString::from(entry.other.title.clone())),
        )
        .child(
            div()
                .invisible()
                .group_hover(ROW_GROUP, |style| style.visible())
                .flex_shrink_0()
                .child(
                    gpui_component::button::Button::new(ElementId::from(SharedString::from(
                        format!("relation-remove-{}", entry.id),
                    )))
                    .ghost()
                    .cursor_pointer()
                    .xsmall()
                    .icon(Icon::new(registry::UI_CLOSE))
                    .tooltip("Remove relation")
                    .on_click(move |_, _window, cx| {
                        cx.stop_propagation();
                        spawn_relation_delete(cx, relation_id.clone());
                    }),
                ),
        )
}

/// The card's rows, grouped by per-side label in menu order.
fn relation_groups(issue: &Issue, cx: &App) -> Vec<RelationGroup> {
    let collections = Store::global(cx).collections();
    let issues = collections.issues.read(cx);
    let mut groups: Vec<RelationGroup> = Vec::new();

    for row in collections.relations_for_issue(&issue.id, cx) {
        let Some(kind) = row.kind.clone() else {
            continue;
        };
        // Which side of the row is this issue on? The other side is the
        // issue we render — and a row can name this issue twice only if the
        // server's CHECK were gone, in which case the forward reading wins.
        let (other_id, mut inverse) = if row.issue_id == issue.id {
            (row.related_issue_id.clone(), false)
        } else {
            (row.issue_id.clone(), true)
        };
        // `related` is symmetric — both sides read "related to", and its
        // rows must land in ONE group whichever way they were stored.
        if kind == domain::contract::ISSUE_RELATION_TYPE_RELATED {
            inverse = false;
        }
        let Some(other) = issues.get(&other_id).cloned() else {
            continue;
        };
        let order = group_order(&kind, inverse);
        let entry = RelationEntry {
            id: row.id.clone(),
            other,
        };
        match groups.iter_mut().find(|group| group.order == order) {
            Some(group) => group.entries.push(entry),
            None => groups.push(RelationGroup {
                order,
                label: capitalize(domain::relations::label(&kind, inverse)),
                icon: relation_icon(&kind, inverse),
                entries: vec![entry],
            }),
        }
    }

    groups.sort_by_key(|group| group.order);
    for group in &mut groups {
        group
            .entries
            .sort_by(|a, b| sync::cmp_identifiers(&a.other.identifier, &b.other.identifier));
    }
    groups
}

/// Group order: the FORWARD pick's menu position, the inverse side right
/// after it — so the card reads Parent of · Sub-issue of · Blocking · Blocked
/// by · Duplicate of · Duplicated by · Related to, matching the add menu even
/// for the sides that have no pick of their own (`duplicated by`).
fn group_order(kind: &str, inverse: bool) -> usize {
    let forward = RELATION_PICKS
        .iter()
        .position(|pick| pick.kind == kind && !pick.inverse)
        .unwrap_or(RELATION_PICKS.len());
    forward * 2 + usize::from(inverse)
}

/// Sentence-case a per-side label for a group heading ("blocked by" →
/// "Blocked by").
fn capitalize(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

/// A pick's icon concept → its glyph.
fn pick_icon(pick: &RelationPick) -> ExpIcon {
    icon_for_concept(pick.icon)
}

/// One side of a relation → its glyph.
fn relation_icon(kind: &str, inverse: bool) -> ExpIcon {
    icon_for_concept(domain::relations::icon_name(kind, inverse))
}

/// The `packages/icons` CONCEPT name → the generated registry glyph (§Shared
/// Contracts: multi-client surfaces name a concept, never a raw glyph).
fn icon_for_concept(concept: &str) -> ExpIcon {
    match concept {
        "relation-parent" => registry::RELATION_PARENT,
        "relation-sub-issue" => registry::RELATION_SUB_ISSUE,
        "relation-blocks" => registry::RELATION_BLOCKS,
        "relation-blocked-by" => registry::RELATION_BLOCKED_BY,
        "relation-duplicate" => registry::RELATION_DUPLICATE,
        "relation-related" => registry::RELATION_RELATED,
        _ => registry::RELATION_SECTION,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_component::IconNamed as _;

    #[test]
    fn groups_follow_the_add_menu_order() {
        let order = |kind: &str, inverse: bool| group_order(kind, inverse);
        let mut keys = vec![
            ("related", false),
            ("duplicate", true),
            ("blocks", true),
            ("parent", false),
            ("blocks", false),
            ("duplicate", false),
            ("parent", true),
        ];
        keys.sort_by_key(|(kind, inverse)| order(kind, *inverse));
        assert_eq!(
            keys,
            vec![
                ("parent", false),
                ("parent", true),
                ("blocks", false),
                ("blocks", true),
                ("duplicate", false),
                ("duplicate", true),
                ("related", false),
            ]
        );
    }

    #[test]
    fn every_pick_and_side_resolves_a_real_glyph() {
        // `ExpIcon` carries no PartialEq (the macro derives Clone +
        // IntoElement only), so glyphs compare by their SVG path like the
        // issue-files table does.
        for pick in RELATION_PICKS {
            assert_ne!(
                pick_icon(&pick).path(),
                registry::RELATION_SECTION.path(),
                "pick {} fell back to the section glyph",
                pick.label
            );
        }
        // The pick-less inverse sides still get their forward glyph.
        assert_eq!(
            relation_icon("duplicate", true).path(),
            registry::RELATION_DUPLICATE.path()
        );
        assert_eq!(
            relation_icon("related", true).path(),
            registry::RELATION_RELATED.path()
        );
    }

    #[test]
    fn group_headings_are_sentence_case() {
        assert_eq!(capitalize(domain::relations::label("blocks", true)), "Blocked by");
        assert_eq!(capitalize(domain::relations::label("parent", true)), "Sub-issue of");
        assert_eq!(capitalize(domain::relations::label("related", false)), "Related to");
        assert_eq!(capitalize(""), "");
    }
}
