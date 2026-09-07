//! EXP-736 — the issue-detail RELATIONS block (web parity target:
//! `apps/web/src/components/issue-relations-card.tsx`).
//!
//! EXP-760 made it Linear-shaped: no card, no "Relations" title and no "Add
//! relation" chip of its own (that pick moved into the header's `…` menu and
//! the list row's context menu), and it sits BELOW the description instead of
//! above it. What is left is the group headings themselves — "Sub-issues",
//! "Blocked by", … from [`domain::relations::group_title`], the web
//! `RELATION_GROUP_TITLES` mirror — with the related issues under each, and a
//! `done/total` counter on Sub-issues. Nothing at all renders when the issue
//! has no relations.
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

use gpui::{
    div, px, AnyElement, App, ElementId, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::ButtonVariants as _,
    ElementExt as _,
    h_flex,
    menu::PopupMenuItem,
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
    /// EXP-760: the other issue's status resolves to the `completed`
    /// category — what the Sub-issues counter counts.
    completed: bool,
}

/// One heading group of the block ("Sub-issues", "Blocked by", …).
struct RelationGroup {
    /// Sort key: the forward pick's menu position, inverse side second.
    order: usize,
    /// Contract `issue_relation_type` + which side this group reads from —
    /// what [`domain::relations::group_title`] and the Sub-issues counter key
    /// on.
    kind: String,
    inverse: bool,
    icon: ExpIcon,
    entries: Vec<RelationEntry>,
}

/// The relations block, or `None` when the issue has none.
///
/// EXP-760: no card, no heading and no "Add relation" affordance — an issue
/// without relations renders NOTHING here (the add pick lives in the header's
/// `…` menu, so an empty read-only affordance would only be noise). The
/// caller places it under the description.
pub(crate) fn render_relations_section(issue: &Issue, cx: &App) -> Option<AnyElement> {
    let groups = relation_groups(issue, cx);
    if groups.is_empty() {
        return None;
    }
    let mut column = v_flex()
        .w_full()
        .gap_2()
        .px(px(DETAIL_GUTTER))
        .pb_2();
    for group in groups {
        column = column.child(render_group(group, cx));
    }
    Some(column.into_any_element())
}

/// EXP-760: the six picks as MENU ITEMS, for whichever menu hosts them — the
/// issue header's `…` and the list row's context menu both open the same
/// second-stage picker.
pub(crate) fn add_relation_submenu(
    mut menu: gpui_component::menu::PopupMenu,
    issue_id: &str,
) -> gpui_component::menu::PopupMenu {
    for pick in RELATION_PICKS {
        let issue_id = issue_id.to_string();
        menu = menu.item(
            PopupMenuItem::new(pick.label)
                .icon(Icon::new(pick_icon(&pick)))
                .on_click(move |_, window, cx| {
                    open_relation_target_picker(issue_id.clone(), pick, window, cx);
                }),
        );
    }
    menu
}

/// Stage two of a pick: choose the issue on the other side. "Duplicate of"
/// takes the duplicate picker instead — the duplicate relation is the mirror
/// of `issues.duplicate_of_id` and only `issues.update` writes both halves.
pub(crate) fn open_relation_target_picker(
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
    let muted = cx.theme().muted_foreground;
    let completed: Vec<bool> = group.entries.iter().map(|entry| entry.completed).collect();
    let progress = sub_issue_progress(&group.kind, group.inverse, &completed);
    let mut column = v_flex().w_full().gap_0p5().child(
        h_flex()
            .items_center()
            .gap_1p5()
            .child(Icon::new(group.icon).xsmall().text_color(muted))
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(SharedString::from(domain::relations::group_title(
                        &group.kind,
                        group.inverse,
                    ))),
            )
            // EXP-760: Sub-issues carry their own progress (web parity) —
            // the one group where the rows are work this issue owns.
            .children(progress.map(|(done, total)| {
                div()
                    .text_xs()
                    .text_color(muted.opacity(0.8))
                    .font_family(theme::terminal::FONT_FAMILY)
                    .child(SharedString::from(format!("{done}/{total}")))
            })),
    );
    for entry in group.entries {
        column = column.child(render_row(entry, cx));
    }
    column
}

/// EXP-760: the `done/total` counter, for the SUB-ISSUES group only. Every
/// other heading counts nothing — "2/5 Blocked by" would read as progress on
/// work this issue does not own.
fn sub_issue_progress(kind: &str, inverse: bool, completed: &[bool]) -> Option<(usize, usize)> {
    if kind != domain::contract::ISSUE_RELATION_TYPE_PARENT || inverse {
        return None;
    }
    Some((completed.iter().filter(|done| **done).count(), completed.len()))
}

fn render_row(entry: RelationEntry, cx: &App) -> impl IntoElement {
    let status = queries::resolve_issue_status(cx, &entry.other);
    let issue_id = entry.other.id.clone();
    let relation_id = entry.id.clone();
    // EXP-760: the row opens the shared issue hover preview — the same card
    // the `#IDENT` pills in prose show. The row's painted rectangle is the
    // anchor, captured at prepaint (the `Popup` recipe) because a hover
    // listener is handed the pointer, not the element.
    let preview_key = format!("relation-row-{}", entry.id);
    let preview_issue = entry.other.id.clone();
    let anchor: std::rc::Rc<std::cell::Cell<gpui::Bounds<gpui::Pixels>>> =
        std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::default()));
    let anchor_write = anchor.clone();
    h_flex()
        .id(ElementId::from(SharedString::from(format!(
            "relation-row-{}",
            entry.id
        ))))
        .on_prepaint(move |bounds, _window, _cx| anchor_write.set(bounds))
        .on_hover(move |hovered, window, cx| {
            let host = crate::issue_preview::host_for_window(window, cx);
            if *hovered {
                let issue_id = preview_issue.clone();
                let bounds = anchor.get();
                host.update(cx, |host, cx| {
                    host.request(preview_key.clone(), issue_id, bounds, cx)
                });
            } else {
                host.update(cx, |host, cx| host.release(preview_key.clone(), cx));
            }
        })
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
        let completed = queries::resolve_issue_status(cx, &other).category
            == domain::statuses::IssueStatusCategory::Completed;
        let entry = RelationEntry {
            id: row.id.clone(),
            other,
            completed,
        };
        match groups.iter_mut().find(|group| group.order == order) {
            Some(group) => group.entries.push(entry),
            None => groups.push(RelationGroup {
                order,
                kind: kind.clone(),
                inverse,
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

/// Group order, mirrored from the web `RELATION_GROUP_ORDER` (EXP-760):
/// Sub-issues · Parent · Blocked by · Blocks · Duplicate of · Duplicated by ·
/// Related. What blocks THIS issue reads before what it blocks.
fn group_order(kind: &str, inverse: bool) -> usize {
    match (kind, inverse) {
        ("parent", false) => 0,
        ("parent", true) => 1,
        ("blocks", true) => 2,
        ("blocks", false) => 3,
        ("duplicate", false) => 4,
        ("duplicate", true) => 5,
        _ => 6,
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
    fn groups_follow_the_web_group_order() {
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
                ("blocks", true),
                ("blocks", false),
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

    /// EXP-760: the block's headings come from the SHARED table
    /// (`domain::relations::group_title`, the web `RELATION_GROUP_TITLES`
    /// mirror) — never from the per-side row labels, which read as sentences
    /// ("sub-issue of") rather than section names.
    #[test]
    fn group_titles_reach_the_section() {
        // Every group the block can build resolves a heading, in the order
        // `group_order` lays them out.
        let mut keys: Vec<(&str, bool)> = vec![
            ("parent", false),
            ("parent", true),
            ("blocks", false),
            ("blocks", true),
            ("duplicate", false),
            ("duplicate", true),
            ("related", false),
        ];
        keys.sort_by_key(|(kind, inverse)| group_order(kind, *inverse));
        let titles: Vec<&str> = keys
            .iter()
            .map(|(kind, inverse)| domain::relations::group_title(kind, *inverse))
            .collect();
        assert_eq!(
            titles,
            vec![
                "Sub-issues",
                "Parent",
                "Blocked by",
                "Blocks",
                "Duplicate of",
                "Duplicated by",
                "Related",
            ]
        );
    }

    /// EXP-760: only Sub-issues counts, and it counts COMPLETED rows.
    #[test]
    fn sub_issue_progress_counts_completed_only() {
        assert_eq!(
            sub_issue_progress("parent", false, &[true, false, true]),
            Some((2, 3))
        );
        // An empty Sub-issues group cannot exist (the block hides empty
        // groups), but the counter must not divide by anything.
        assert_eq!(sub_issue_progress("parent", false, &[]), Some((0, 0)));
        // "Parent" is the OTHER side — the parent is not this issue's work.
        assert_eq!(sub_issue_progress("parent", true, &[true]), None);
        assert_eq!(sub_issue_progress("blocks", false, &[true]), None);
        assert_eq!(sub_issue_progress("blocks", true, &[true]), None);
        assert_eq!(sub_issue_progress("duplicate", false, &[true]), None);
        assert_eq!(sub_issue_progress("related", false, &[true]), None);
    }
}
