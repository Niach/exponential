//! The full-width top bar — sits ABOVE everything (rail included), the
//! JetBrains-new-UI header. Left: the **board picker** (the window's active
//! board; picking navigates to that board and re-scopes Files / Source
//! Control / run configs) followed by the current screen's breadcrumb trail.
//! Right: the run widget (config select + play/stop) and the trunk git
//! cluster (branch chip, sync status, Commit, Pull/Push).
//!
//! EXP-69: the picker doubles as the team switcher — it lists EVERY
//! board across ALL the user's teams (grouped under small team
//! headers once there is more than one), and picking a board from another
//! team switches team + board in one action. The old footer
//! account-menu team list is gone.
//!
//! The issue detail keeps its own richer breadcrumb row (board › identifier
//! › live title + Start coding) — the top bar shows no crumb for it.

use gpui::{
    div, px, AppContext as _, Entity, FontWeight, IntoElement, ParentElement, Render,
    SharedString, Styled, Subscription, WeakEntity, Window,
};
use gpui_component::{
    dock::DockArea, h_flex, menu::DropdownMenu as _, sidebar::SidebarHeader, skeleton::Skeleton,
    ActiveTheme as _, Icon, IconName, Sizable as _,
};
use sync::Store;

use crate::actions::{NewBoard, OpenBoard, SwitchTeam};
use crate::git_bar::GitBar;
use crate::navigation::{active_board_id, active_team_id, nav_for_window, Navigation};
use crate::properties_panel::parse_hex_color;
use crate::run_bar::RunBar;
use crate::sidebar::rail_shared_for_window;

/// Top-bar height (the terminal strip / rail metrics live in their modules).
pub(crate) const TOP_BAR_H: f32 = 38.;

/// One team's slice of the merged picker menu (EXP-69) — a captured
/// snapshot, cheap clones only (menus render lazily in the overlay).
struct PickerGroup {
    team_id: String,
    team_name: String,
    /// `(board_id, board_name, is_active)` rows, board sort order.
    boards: Vec<(String, String, bool)>,
}

/// The header view owned by the `Shell` shell.
pub struct TopBar {
    nav: Entity<Navigation>,
    /// The shared trunk git chrome (owned by the rail registry so the rail's
    /// conflict badge and this rendering read the same state).
    git_bar: Entity<GitBar>,
    /// The run widget (config select + play/stop). Self-scopes to the active
    /// board and hides without one.
    run_bar: Entity<RunBar>,
    _subscriptions: Vec<Subscription>,
}

impl TopBar {
    pub fn new(
        dock_area: WeakEntity<DockArea>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let git_bar = shared.read(cx).git_bar().clone();
        let run_bar = cx.new(|cx| RunBar::new(dock_area, window, cx));
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&git_bar, |_, _, cx| cx.notify()),
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            cx.observe(&Store::global(cx).state(), |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            git_bar,
            run_bar,
            _subscriptions: subscriptions,
        }
    }

    /// The board picker (EXP-69 merged with the team switcher): the
    /// active board as the label; the dropdown lists ALL boards across
    /// ALL teams, grouped under small team headers (flat with a
    /// plain "Boards" label while there is only one team), plus
    /// "New board…". Picking a board from another team switches
    /// team + board in one action (the `OpenBoard` handler
    /// re-scopes); a board-less team gets a "Switch to team"
    /// entry so it stays reachable.
    fn render_board_picker(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let store = Store::global(cx);
        if !store.collections().boards.read(cx).is_ready() {
            return h_flex()
                .gap_2()
                .items_center()
                .child(Skeleton::new().size_4().flex_shrink_0())
                .child(Skeleton::new().h_4().w_32())
                .into_any_element();
        }

        let Some(team_id) = active_team_id(&self.nav, cx) else {
            return div().into_any_element();
        };
        let boards = store
            .collections()
            .boards_in_team(&team_id, cx);
        let active_id = active_board_id(&self.nav, cx);
        let active = active_id
            .as_deref()
            .and_then(|id| boards.iter().find(|p| p.id == id));

        let dot_color = active
            .and_then(|p| p.color.as_deref())
            .and_then(parse_hex_color)
            .unwrap_or(cx.theme().muted_foreground);
        let label: SharedString = active
            .map(|p| SharedString::from(p.name.clone()))
            .unwrap_or_else(|| "Select board".into());
        // The active board's stored icon (falling back to the
        // attribute-derived glyph) drives the leading glyph, color-tinted.
        // Without an active board fall back to the neutral color dot.
        let type_glyph = active.map(crate::icons::board_icon);

        // Captured snapshot for the menu builder (menus render lazily in the
        // overlay; they must not read `self`): one group per team
        // (name-sorted, web picker order), each with its boards as
        // `(id, name, is_active)` rows.
        let groups: Vec<PickerGroup> = store
            .collections()
            .teams_sorted(cx)
            .into_iter()
            .map(|team| {
                let boards: Vec<(String, String, bool)> = store
                    .collections()
                    .boards_in_team(&team.id, cx)
                    .iter()
                    .map(|p| {
                        (
                            p.id.clone(),
                            p.name.clone(),
                            Some(p.id.as_str()) == active_id.as_deref(),
                        )
                    })
                    .collect();
                PickerGroup {
                    team_id: team.id,
                    team_name: team.name,
                    boards,
                }
            })
            .collect();

        let leading = match type_glyph {
            Some(icon) => icon
                .xsmall()
                .text_color(dot_color)
                .flex_shrink_0()
                .into_any_element(),
            None => div()
                .size_3()
                .flex_shrink_0()
                .rounded_full()
                .bg(dot_color)
                .into_any_element(),
        };

        let trigger_inner = h_flex()
            .gap_2()
            .items_center()
            .max_w(px(240.))
            .overflow_hidden()
            .child(leading)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(label),
            );

        div()
            .flex_shrink_0()
            .child(
                SidebarHeader::new()
                    .px_2()
                    .py_1()
                    .child(trigger_inner)
                    .child(
                        Icon::new(IconName::ChevronsUpDown)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .dropdown_menu(move |menu, _window, _cx| {
                        // Board lists grow with the account's teams —
                        // cap + scroll (EXP-46a). Flat items only (no
                        // submenus): team grouping is small label
                        // headers, shown once there is more than one
                        // team.
                        let mut menu = menu.scrollable(true).max_h(px(320.));
                        let show_headers = groups.len() > 1;
                        if !show_headers {
                            menu = menu.label("Boards");
                        }
                        for group in &groups {
                            if show_headers {
                                menu =
                                    menu.label(SharedString::from(group.team_name.clone()));
                            }
                            if group.boards.is_empty() && show_headers {
                                // Keep board-less teams reachable now
                                // that the footer team switcher is gone.
                                menu = menu.menu(
                                    "Switch to team",
                                    Box::new(SwitchTeam {
                                        team_id: group.team_id.clone(),
                                    }),
                                );
                                continue;
                            }
                            for (id, name, active) in &group.boards {
                                menu = menu.menu_with_check(
                                    SharedString::from(name.clone()),
                                    *active,
                                    Box::new(OpenBoard {
                                        board_id: id.clone(),
                                    }),
                                );
                            }
                        }
                        menu.separator()
                            .menu_with_icon("New board…", IconName::Plus, Box::new(NewBoard))
                    }),
            )
            .into_any_element()
    }

}

impl Render for TopBar {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        h_flex()
            .w_full()
            .h(px(TOP_BAR_H))
            .flex_shrink_0()
            .items_center()
            .px_2()
            .gap_1p5()
            .bg(cx.theme().tokens.sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .border_b_1()
            .border_color(cx.theme().sidebar_border)
            .child(self.render_board_picker(cx))
            .child(div().flex_1().min_w_0())
            .child(self.run_bar.clone())
            .child(
                div()
                    .w(px(1.))
                    .h_4()
                    .mx_1()
                    .bg(cx.theme().sidebar_border),
            )
            .child(self.git_bar.clone())
    }
}
