//! The full-width top bar — sits ABOVE everything (rail included), the
//! JetBrains-new-UI header. Left: the **project picker** (the window's active
//! project; picking navigates to that board and re-scopes Files / Source
//! Control / run configs) followed by the current screen's breadcrumb trail.
//! Right: the run widget (config select + play/stop) and the trunk git
//! cluster (branch chip, sync status, Commit, Pull/Push).
//!
//! The issue detail keeps its own richer breadcrumb row (project › identifier
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

use crate::actions::{NewProject, OpenProject};
use crate::git_bar::GitBar;
use crate::navigation::{active_project_id, active_workspace_id, nav_for_window, Navigation};
use crate::properties_panel::parse_hex_color;
use crate::run_bar::RunBar;
use crate::sidebar::rail_shared_for_window;

/// Top-bar height (the terminal strip / rail metrics live in their modules).
pub(crate) const TOP_BAR_H: f32 = 38.;

/// The header view owned by the `Workspace` shell.
pub struct TopBar {
    nav: Entity<Navigation>,
    /// The shared trunk git chrome (owned by the rail registry so the rail's
    /// conflict badge and this rendering read the same state).
    git_bar: Entity<GitBar>,
    /// The run widget (config select + play/stop). Self-scopes to the active
    /// project and hides without one.
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
            cx.observe(&collections.workspaces, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |_, _, cx| cx.notify()),
            cx.observe(&Store::global(cx).state(), |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            git_bar,
            run_bar,
            _subscriptions: subscriptions,
        }
    }

    /// The project picker: the active project as the label, the workspace's
    /// projects (check on the active one) + "New project…" in the dropdown.
    fn render_project_picker(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let store = Store::global(cx);
        if !store.collections().projects.read(cx).is_ready() {
            return h_flex()
                .gap_2()
                .items_center()
                .child(Skeleton::new().size_4().flex_shrink_0())
                .child(Skeleton::new().h_4().w_32())
                .into_any_element();
        }

        let Some(workspace_id) = active_workspace_id(&self.nav, cx) else {
            return div().into_any_element();
        };
        let projects = store
            .collections()
            .projects_in_workspace(&workspace_id, cx);
        let active_id = active_project_id(&self.nav, cx);
        let active = active_id
            .as_deref()
            .and_then(|id| projects.iter().find(|p| p.id == id));

        let dot_color = active
            .and_then(|p| p.color.as_deref())
            .and_then(parse_hex_color)
            .unwrap_or(cx.theme().muted_foreground);
        let label: SharedString = active
            .map(|p| SharedString::from(p.name.clone()))
            .unwrap_or_else(|| "Select project".into());
        // The active project's type drives the leading glyph (code / kanban /
        // megaphone, color-tinted) and the feedback globe marker. Without an
        // active project fall back to the neutral color dot.
        let type_glyph = active.map(crate::icons::project_type_icon);
        let is_feedback = active.map(|p| p.is_feedback()).unwrap_or(false);

        // Captured snapshot for the menu builder (menus render lazily in the
        // overlay; they must not read `self`).
        let picker: Vec<(String, String, bool)> = projects
            .iter()
            .map(|p| {
                (
                    p.id.clone(),
                    p.name.clone(),
                    Some(p.id.as_str()) == active_id.as_deref(),
                )
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

        let mut trigger_inner = h_flex()
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
        if is_feedback {
            trigger_inner = trigger_inner.child(
                crate::icons::public_board_icon()
                    .xsmall()
                    .flex_shrink_0()
                    .text_color(cx.theme().muted_foreground),
            );
        }

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
                        // Project lists grow with the workspace — cap + scroll
                        // (EXP-46a). Flat items only (no submenus).
                        let mut menu = menu.scrollable(true).max_h(px(320.)).label("Projects");
                        for (id, name, active) in &picker {
                            menu = menu.menu_with_check(
                                SharedString::from(name.clone()),
                                *active,
                                Box::new(OpenProject {
                                    project_id: id.clone(),
                                }),
                            );
                        }
                        menu.separator()
                            .menu_with_icon("New project…", IconName::Plus, Box::new(NewProject))
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
            .child(self.render_project_picker(cx))
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
