//! #EXP-1611 — CI on the chat's PR (pr-checks.json CHECKS LIST,
//! `domain::pr_checks`): the glyphs and tints, the Checks submenu of the
//! chat header's `…` (one menu row per check, a pick opens its logs) and
//! the static panel the styleguide entry draws. #EXP-1710: the header's merge
//! STATUS pill (`work_header::merge_status_pill`) opens the same rows.

use gpui::{div, px, AnyElement, App, Hsla, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::{menu::PopupMenuItem, ActiveTheme as _, Icon};
use theme::tokens as t;

use domain::pr_checks::{CheckGlyph, CheckListRow, CheckTone};
use domain::rows::PullRequestCheckRow;
use sync::Store;

use crate::icons::{registry, ExpIcon};

/// The concept icon of a check glyph.
pub(crate) fn glyph_icon(glyph: CheckGlyph) -> ExpIcon {
    match glyph {
        CheckGlyph::Failed => registry::CI_FAILED,
        CheckGlyph::Running => registry::CI_RUNNING,
        CheckGlyph::Queued => registry::CI_QUEUED,
        CheckGlyph::Passed => registry::CI_PASSED,
        CheckGlyph::Skipped => registry::CI_SKIPPED,
    }
}

/// A tone's tint: the semantic red / green accents, else muted.
pub(crate) fn tone_color(tone: CheckTone, cx: &App) -> Hsla {
    match tone {
        CheckTone::Red => t::RED.to_hsla(),
        CheckTone::Green => t::GREEN.to_hsla(),
        CheckTone::Muted => cx.theme().muted_foreground,
    }
}

/// A row's lead: the tinted glyph, `ci-running` = THE spinner.
pub(crate) fn check_glyph(glyph: CheckGlyph, tone: CheckTone, size: f32, cx: &App) -> AnyElement {
    let color = tone_color(tone, cx);
    if glyph == CheckGlyph::Running {
        return crate::feedback::spinner_colored(size, color, cx);
    }
    Icon::new(glyph_icon(glyph))
        .size(px(size))
        .flex_shrink_0()
        .text_color(color)
        .into_any_element()
}

/// The synced check rows of `pull_request_id` (empty without a store).
///
/// #PERF-4: the chat header reads this every frame (its status pill and its
/// `…` menu, twice), and a scan of every synced check row (thousands on a
/// busy team) was the header's largest share of a frame. The rows are
/// indexed by pull request once per change of the collection.
pub(crate) fn checks_of(pull_request_id: &str, cx: &App) -> Vec<PullRequestCheckRow> {
    let Some(store) = Store::try_global(cx) else { return Vec::new() };
    let checks = &store.collections().pull_request_checks;
    let stamp = (checks.entity_id(), checks.read(cx).revision());
    CHECKS_BY_PULL.with(|memo| {
        let mut memo = memo.borrow_mut();
        if memo.0 != Some(stamp) {
            // The ids only: a row's clone (its output) costs more than the
            // whole index, and a busy CI rewrites the table all day.
            let mut index: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
            for row in checks.read(cx).iter() {
                index.entry(row.pull_request_id.clone()).or_default().push(row.id.clone());
            }
            *memo = (Some(stamp), index);
        }
        let rows = checks.read(cx);
        memo.1.get(pull_request_id).into_iter().flatten().filter_map(|id| rows.get(id).cloned()).collect()
    })
}

/// #PERF-4: how many issues `pull_request_id` closes (`pull_request_issues`),
/// indexed once per change of the collection like [`checks_of`].
pub(crate) fn linked_issue_count(pull_request_id: &str, cx: &App) -> usize {
    let Some(store) = Store::try_global(cx) else { return 0 };
    let links = &store.collections().pull_request_issues;
    let stamp = (links.entity_id(), links.read(cx).revision());
    LINKS_BY_PULL.with(|memo| {
        let mut memo = memo.borrow_mut();
        if memo.0 != Some(stamp) {
            let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for link in links.read(cx).iter() {
                *index.entry(link.pull_request_id.clone()).or_default() += 1;
            }
            *memo = (Some(stamp), index);
        }
        memo.1.get(pull_request_id).copied().unwrap_or(0)
    })
}

type Stamp = Option<(gpui::EntityId, u64)>;

thread_local! {
    static CHECKS_BY_PULL: std::cell::RefCell<(Stamp, std::collections::HashMap<String, Vec<String>>)> = Default::default();
    static LINKS_BY_PULL: std::cell::RefCell<(Stamp, std::collections::HashMap<String, usize>)> = Default::default();
}

/// One check row's body (the live menu's and the styleguide panel's):
/// glyph, name, the muted duration on the right. A row without a url is
/// disabled (muted, no pointer).
pub(crate) fn check_row_body(row: &CheckListRow, cx: &App) -> gpui::Div {
    use gpui::prelude::FluentBuilder as _;
    let disabled = row.url.is_none();
    let muted = cx.theme().muted_foreground;
    crate::controls::pointer_row_fill(
        gpui_component::h_flex()
            .flex_1()
            .min_w_0()
            .gap(px(t::menu::pointer::ITEM_GAP))
            .items_center()
            .child(check_glyph(row.glyph, row.tone, t::menu::pointer::ICON_SIZE, cx))
            .child(div().flex_1().min_w_0().truncate().child(row.name.clone()))
            .children(row.duration.clone().map(|duration| {
                div().flex_shrink_0().text_xs().text_color(muted).child(duration).into_any_element()
            })),
        disabled,
    )
    .when(disabled, |body| body.text_color(muted))
}

/// One check as THE menu item: a pick opens its logs in the browser.
pub(crate) fn check_menu_item(index: usize, row: CheckListRow) -> PopupMenuItem {
    let url = row.url.clone();
    let disabled = url.is_none();
    PopupMenuItem::element(move |_, cx| {
        use gpui::InteractiveElement as _;
        check_row_body(&row, cx).id(SharedString::from(format!(
            "{}-{index}",
            domain::pr_checks::TEST_ID_CHECK_ROW
        )))
    })
    .disabled(disabled)
    .on_click(move |_, window, cx| {
        // #EXP-1804: our own CI's Details open its run page in-app (THE
        // link opener); anything else goes to the browser as before.
        if let Some(url) = url.clone() {
            crate::links::open_link(&url, None, window, cx);
        }
    })
}

/// The `…` menu's leading Checks submenu over `rows` (#EXP-1611): the entry
/// = the summary glyph tinted by its tone, `Checks` with the summary as the
/// row's muted trailing value (#EXP-1620, THE Menu `value`); its rows =
/// `checks_list`. No rows = no entry (the menu is unchanged).
pub(crate) fn checks_submenu(
    menu: gpui_component::menu::PopupMenu,
    rows: &[PullRequestCheckRow],
    window: &mut Window,
    cx: &mut gpui::Context<gpui_component::menu::PopupMenu>,
) -> gpui_component::menu::PopupMenu {
    let Some(summary) = domain::pr_checks::checks_summary(rows) else {
        return menu;
    };
    let list = domain::pr_checks::checks_list(rows, crate::session_data::now_ms());
    let icon = Icon::new(glyph_icon(summary.glyph)).text_color(tone_color(summary.tone, cx));
    let label = domain::pr_checks::CHECKS_LABEL;
    menu.submenu_with_value(Some(icon), label, Some(summary.value.clone()), window, cx, move |mut sub, _, _| {
        sub = sub.min_w(px(t::menu::pointer::MIN_WIDTH + 60.));
        for (index, row) in list.iter().cloned().enumerate() {
            sub = sub.item(check_menu_item(index, row));
        }
        sub
    })
}

/// The open Checks submenu drawn as a STATIC menu panel (the styleguide
/// entry): the menu surface with the parent row and the flyout beside it.
pub(crate) fn checks_panel(rows: &[PullRequestCheckRow], now_ms: i64, cx: &App) -> gpui::Div {
    let surface = |cx: &App| crate::picker::search_list::surface(cx).p_1();
    let mut parent = surface(cx).w(px(240.));
    if let Some(summary) = domain::pr_checks::checks_summary(rows) {
        let theme = cx.theme();
        parent = parent.child(
            crate::controls::pointer_row_fill(
                gpui_component::h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(t::menu::pointer::ITEM_GAP))
                    .items_center()
                    .child(
                        Icon::new(glyph_icon(summary.glyph))
                            .size(px(t::menu::pointer::ICON_SIZE))
                            .flex_shrink_0()
                            .text_color(tone_color(summary.tone, cx)),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(domain::pr_checks::CHECKS_LABEL))
                    .child(crate::controls::menu_row_value(Some(summary.value.clone().into()), true, cx)),
                false,
            )
            .px_2()
            .rounded(theme.radius)
            .bg(theme.accent),
        );
    }
    let mut flyout = surface(cx).w(px(t::menu::pointer::MIN_WIDTH + 60.));
    for row in domain::pr_checks::checks_list(rows, now_ms) {
        flyout = flyout.child(check_row_body(&row, cx).px_2());
    }
    div().flex().flex_row().items_start().gap_1().child(parent).child(flyout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_is_its_concept_icon() {
        for glyph in [
            CheckGlyph::Failed,
            CheckGlyph::Running,
            CheckGlyph::Queued,
            CheckGlyph::Passed,
            CheckGlyph::Skipped,
        ] {
            use gpui_component::IconNamed as _;
            let concept = registry::concept_by_name(glyph.concept()).map(|icon| icon.path());
            assert_eq!(concept, Some(glyph_icon(glyph).path()), "{}", glyph.concept());
        }
    }
}

#[cfg(test)]
mod perf_index_tests {
    use sync::{ShapeRow as _, Store};

    fn seed_check(id: &str, pull: &str, cx: &mut gpui::TestAppContext) {
        let row: domain::rows::PullRequestCheckRow = serde_json::from_value(serde_json::json!({"id": id, "pull_request_id": pull, "name": id})).unwrap();
        cx.update(|cx| {
            let checks = Store::global(cx).collections().pull_request_checks.clone();
            checks.update(cx, |collection, cx| {
                collection.seed(row.key(), row);
                cx.notify();
            })
        });
    }

    fn seed_link(id: &str, pull: &str, issue: &str, cx: &mut gpui::TestAppContext) {
        let row: domain::rows::PullRequestIssueRow = serde_json::from_value(serde_json::json!({"id": id, "pull_request_id": pull, "issue_id": issue})).unwrap();
        cx.update(|cx| {
            let links = Store::global(cx).collections().pull_request_issues.clone();
            links.update(cx, |collection, cx| {
                collection.seed(row.key(), row);
                cx.notify();
            })
        });
    }

    /// #PERF-4: the per-PR indexes follow every write of their collection
    /// (the header reads them every frame; a stale index = a wrong pill).
    #[gpui::test]
    fn the_pr_indexes_follow_their_collections(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let store = Store::open(cx, None, None);
            cx.set_global(store);
        });
        let checks = |pull: &str, cx: &mut gpui::TestAppContext| cx.update(|cx| super::checks_of(pull, cx).len());
        let links = |pull: &str, cx: &mut gpui::TestAppContext| cx.update(|cx| super::linked_issue_count(pull, cx));
        assert_eq!(checks("p1", cx), 0);
        seed_check("c1", "p1", cx);
        assert_eq!(checks("p1", cx), 1);
        seed_check("c2", "p1", cx);
        seed_check("c3", "p2", cx);
        assert_eq!((checks("p1", cx), checks("p2", cx), checks("p3", cx)), (2, 1, 0));
        assert_eq!(links("p1", cx), 0);
        seed_link("l1", "p1", "i1", cx);
        seed_link("l2", "p1", "i2", cx);
        assert_eq!((links("p1", cx), links("p2", cx)), (2, 0));
        // A second store (another account) never reads the first's index.
        cx.update(|cx| {
            let store = Store::open(cx, None, None);
            cx.set_global(store);
        });
        assert_eq!((checks("p1", cx), links("p1", cx)), (0, 0));
    }
}
