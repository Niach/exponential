//! The workspace sidebar (masterplan-v3 §4.2 "Workspace shell", §4.8 EXP-1).
//!
//! Lives in the `DockArea`'s **left dock** as a chrome-less `DockItem::Panel`
//! (no tab/title bar) and is **non-collapsible** (`SidebarCollapsible::None`,
//! EXP-1 #8). Contents top-to-bottom per §4.2:
//!
//! - workspace-picker `DropdownMenu` (EXP-1 #1 — shadcn dropdown listing the
//!   synced workspaces, check on the active one),
//! - first-class nav rows **My Issues / Inbox / Search** (EXP-1 #3),
//! - a "Projects" group with the **`+` new-project button on the group
//!   header** (EXP-1 #2) and the real project rows off the synced `projects`
//!   collection (active row highlighted),
//! - footer: a **Send Feedback** direct item (EXP-1 #10) and the
//!   **account/settings dropdown** (EXP-1 #11 — Settings lives here, not only
//!   in a system menubar).
//!
//! Every affordance dispatches a typed action (§3.6); the navigation actions
//! are handled by `navigation::init`'s App-global listeners (menus render in
//! the Root overlay, outside this element tree). The footer also renders the
//! live shared-`Store` window count — the §3.10 multi-window proof.

use gpui::{
    div, App, ClickEvent, ElementId, FocusHandle, Focusable, FontWeight, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dock::{Panel, PanelControl, PanelEvent},
    h_flex,
    menu::DropdownMenu as _,
    sidebar::{
        Sidebar, SidebarCollapsible, SidebarFooter, SidebarHeader, SidebarItem, SidebarMenu,
        SidebarMenuItem,
    },
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Collapsible, Icon, IconName, Sizable as _,
};
use sync::Store;

use crate::actions::{
    CreateWorkspace, NewProject, OpenInbox, OpenMyIssues, OpenProject, OpenSearch, OpenSettings,
    SendFeedback, SignOut, SwitchWorkspace,
};
use crate::navigation::{active_workspace_id, nav_for_window, resolved_screen, Navigation, Screen};

/// Stable serialization name (§3.3: "Once you have defined a panel name, this
/// must not be changed").
pub const PANEL_NAME: &str = "Sidebar";

/// The left-dock sidebar panel. Implements [`Panel`] because everything
/// dockable must (§3.3), but renders chrome-less via `DockItem::Panel`.
pub struct SidebarPanel {
    focus_handle: FocusHandle,
    nav: gpui::Entity<Navigation>,
    _subscriptions: Vec<Subscription>,
}

impl SidebarPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            // Session phase + window count (footer) — the shared state.
            cx.observe(&Store::global(cx).state(), |_, _, cx| cx.notify()),
            // Picker + project rows are live collection reads (§4.1).
            cx.observe(&collections.workspaces, |_, _, cx| cx.notify()),
            cx.observe(&collections.projects, |_, _, cx| cx.notify()),
            // The §4.8 solo-vs-team chrome rule reads memberships + users.
            cx.observe(&collections.workspace_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
            // Active-row highlight follows navigation.
            cx.observe(&nav, |_, _, cx| cx.notify()),
        ];

        Self {
            focus_handle: cx.focus_handle(),
            nav,
            _subscriptions: subscriptions,
        }
    }

    /// Workspace picker (EXP-1 #1): shadcn-style dropdown on the sidebar
    /// header, listing the synced workspaces with a check on the active one.
    /// States (the "polish" pass): skeleton while the workspaces shape has
    /// not seen its first `up-to-date` (§4.1 — never render a wrong header
    /// off an in-flight snapshot); the **solo-vs-team chrome rule** (§4.8):
    /// a solo user sees a static "Exponential" brand row, no switcher.
    fn render_header(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let store = Store::global(cx);
        if !store.collections().workspaces.read(cx).is_ready() {
            return h_flex()
                .gap_2()
                .p_2()
                .items_center()
                .child(Skeleton::new().size_6().flex_shrink_0())
                .child(Skeleton::new().h_4().w_32())
                .into_any_element();
        }

        let workspaces = store.collections().workspaces_sorted(cx);
        let active_id = active_workspace_id(&self.nav, cx);
        let show_chrome = active_id
            .as_deref()
            .map(|id| crate::settings::show_workspace_chrome(cx, id))
            .unwrap_or(true);
        let active = active_id
            .as_deref()
            .and_then(|id| workspaces.iter().find(|w| w.id == id));

        if !show_chrome {
            // Web solo branch: static brand row — no workspace concept, no
            // switcher (Settings + New workspace move to the footer menu).
            return h_flex()
                .gap_2()
                .p_2()
                .items_center()
                .child(brand_badge("E", cx))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child("Exponential"),
                )
                .into_any_element();
        }

        let active_name: SharedString = active
            .map(|w| SharedString::from(w.name.clone()))
            .unwrap_or_else(|| "Exponential".into());
        let is_public = active.and_then(|w| w.is_public).unwrap_or(false);

        // Captured snapshot for the menu builder (menus render lazily in the
        // overlay; they must not read `self`).
        let picker: Vec<(String, String, bool)> = workspaces
            .iter()
            .map(|w| {
                (
                    w.id.clone(),
                    w.name.clone(),
                    Some(w.id.as_str()) == active_id.as_deref(),
                )
            })
            .collect();

        let mut name_row = h_flex()
            .gap_2()
            .overflow_hidden()
            .child(brand_badge(
                &active_name.chars().next().unwrap_or('E').to_uppercase().to_string(),
                cx,
            ))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(active_name),
            );
        if is_public {
            // Web "Public" pill on the switcher trigger.
            name_row = name_row.child(
                div()
                    .flex_shrink_0()
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().accent)
                    .px_1p5()
                    .py_0p5()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("PUBLIC"),
            );
        }

        SidebarHeader::new()
            .child(name_row)
            .child(
                Icon::new(IconName::ChevronsUpDown)
                    .xsmall()
                    .text_color(cx.theme().sidebar_foreground.opacity(0.7)),
            )
            .dropdown_menu(move |menu, _window, _cx| {
                let mut menu = menu.label("Workspaces");
                for (id, name, active) in &picker {
                    menu = menu.menu_with_check(
                        SharedString::from(name.clone()),
                        *active,
                        Box::new(SwitchWorkspace {
                            workspace_id: id.clone(),
                        }),
                    );
                }
                menu.separator()
                    .menu_with_icon("Create workspace…", IconName::Plus, Box::new(CreateWorkspace))
            })
            .into_any_element()
    }

    /// First-class nav rows (EXP-1 #3): My Issues, Inbox, Search — active
    /// state mirrors the resolved screen.
    fn nav_menu(&self, cx: &App) -> SidebarMenu {
        let screen = resolved_screen(&self.nav, cx);
        SidebarMenu::new()
            .child(
                SidebarMenuItem::new("My Issues")
                    .icon(IconName::CircleUser)
                    .active(matches!(screen, Some(Screen::MyIssues)))
                    .on_click(dispatch(OpenMyIssues)),
            )
            .child(
                SidebarMenuItem::new("Inbox")
                    .icon(IconName::Inbox)
                    .active(matches!(screen, Some(Screen::Inbox)))
                    .on_click(dispatch(OpenInbox)),
            )
            .child(
                SidebarMenuItem::new("Search")
                    .icon(IconName::Search)
                    .on_click(dispatch(OpenSearch)),
            )
    }

    /// Project rows off the synced collection (sort-order, archived hidden —
    /// web sidebar parity), highlighted when their board is the screen.
    fn projects_menu(&self, cx: &App) -> SidebarMenu {
        let Some(workspace_id) = active_workspace_id(&self.nav, cx) else {
            return SidebarMenu::new()
                .child(SidebarMenuItem::new("No projects yet").disable(true));
        };
        let projects = Store::global(cx)
            .collections()
            .projects_in_workspace(&workspace_id, cx);
        if projects.is_empty() {
            return SidebarMenu::new()
                .child(SidebarMenuItem::new("No projects yet").disable(true));
        }

        let screen = resolved_screen(&self.nav, cx);
        let active_project = match &screen {
            Some(Screen::Board { project_id }) => Some(project_id.clone()),
            _ => None,
        };

        let mut menu = SidebarMenu::new();
        for project in projects {
            let action = OpenProject {
                project_id: project.id.clone(),
            };
            menu = menu.child(
                SidebarMenuItem::new(SharedString::from(project.name.clone()))
                    .active(active_project.as_deref() == Some(project.id.as_str()))
                    .on_click(dispatch(action)),
            );
        }
        menu
    }

    /// Footer: Send Feedback direct item (EXP-1 #10), the account/settings
    /// dropdown (EXP-1 #11), and the shared-store status line (§3.10 proof).
    /// In solo mode (§4.8) "New workspace" moves here (web parity — the
    /// switcher that normally carries it is hidden).
    fn render_footer(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let state = Store::global(cx).state();
        let windows_open = state.read(cx).windows_open;
        let solo = active_workspace_id(&self.nav, cx)
            .map(|id| !crate::settings::show_workspace_chrome(cx, &id))
            .unwrap_or(false);
        // The sidebar only renders when Synced (the workspace swaps to the
        // login surface otherwise), so show who is signed in.
        let account = crate::queries::active_account(cx);
        let who: SharedString = account
            .as_ref()
            .map(|account| SharedString::from(account.email.clone()))
            .unwrap_or_else(|| "Not signed in".into());
        let display_name: SharedString = account
            .as_ref()
            .and_then(|account| account.name.clone())
            .map(SharedString::from)
            .unwrap_or_else(|| who.clone());
        let status: SharedString = format!(
            "{who} · {windows_open} {}",
            if windows_open == 1 { "window" } else { "windows" }
        )
        .into();

        v_flex()
            .w_full()
            .gap_1()
            .child(
                Button::new("sidebar-send-feedback")
                    .icon(IconName::ThumbsUp)
                    .label("Send Feedback")
                    .ghost()
                    .small()
                    .w_full()
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(SendFeedback), cx)),
            )
            .child(
                SidebarFooter::new()
                    .child(
                        h_flex()
                            .gap_2()
                            .overflow_hidden()
                            .child(Icon::new(IconName::CircleUser).small())
                            .child(
                                div()
                                    .text_sm()
                                    .whitespace_nowrap()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child(display_name),
                            ),
                    )
                    .child(
                        Icon::new(IconName::ChevronsUpDown)
                            .xsmall()
                            .text_color(cx.theme().sidebar_foreground.opacity(0.7)),
                    )
                    .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, move |menu, _window, _cx| {
                        let mut menu = menu
                            .menu_with_icon("Settings", IconName::Settings, Box::new(OpenSettings))
                            .menu_with_icon(
                                "Account",
                                IconName::CircleUser,
                                Box::new(crate::actions::OpenAccount),
                            );
                        if solo {
                            // §4.8 solo rule: the switcher (and its "Create
                            // workspace…") is hidden — the affordance lives
                            // here instead, framed account-level (web parity).
                            menu = menu.menu_with_icon(
                                "New workspace",
                                IconName::Plus,
                                Box::new(CreateWorkspace),
                            );
                        }
                        menu.separator().menu("Sign out", Box::new(SignOut))
                    }),
            )
            .child(
                div()
                    .px_2()
                    .text_xs()
                    .text_color(cx.theme().sidebar_foreground.opacity(0.5))
                    .child(status),
            )
    }
}

/// Click-handler that dispatches a typed action on the window (§3.6 actions
/// story — chrome never does inline view surgery).
fn dispatch<A: gpui::Action + Clone>(
    action: A,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    move |_, window, cx| window.dispatch_action(Box::new(action.clone()), cx)
}

/// The square initial badge of the web switcher trigger ("E" brand square in
/// solo mode, the workspace initial with chrome).
fn brand_badge(initial: &str, cx: &App) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .justify_center()
        .size_6()
        .flex_shrink_0()
        .rounded(cx.theme().radius)
        .bg(cx.theme().sidebar_primary)
        .text_color(cx.theme().sidebar_primary_foreground)
        .text_xs()
        .font_weight(FontWeight::BOLD)
        .child(SharedString::from(initial.to_string()))
        .into_any_element()
}

/// The sidebar's content sections. `Sidebar<E>` is generic over ONE
/// `SidebarItem` type, and the stock `SidebarGroup` header is label-only —
/// the "Projects" header needs the EXP-1 #2 `+` button, so this enum renders
/// both plain menus and the custom-headed Projects group.
#[derive(Clone)]
enum SidebarSection {
    /// Plain menu block (the nav rows).
    Menu(SidebarMenu),
    /// "Projects" group: custom header row with the `+` button (EXP-1 #2).
    Projects(SidebarMenu),
}

impl Collapsible for SidebarSection {
    fn is_collapsed(&self) -> bool {
        false
    }

    /// The sidebar is `SidebarCollapsible::None` (EXP-1 #8) — sections never
    /// collapse; the flag is intentionally dropped.
    fn collapsed(self, _collapsed: bool) -> Self {
        self
    }
}

impl SidebarItem for SidebarSection {
    fn render(
        self,
        id: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let id = id.into();
        match self {
            Self::Menu(menu) => menu.render(id, window, cx).into_any_element(),
            Self::Projects(menu) => v_flex()
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .justify_between()
                        .items_center()
                        .px_2()
                        .h_8()
                        .rounded(cx.theme().radius)
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().sidebar_foreground.opacity(0.7))
                                .child("Projects"),
                        )
                        .child(
                            // EXP-1 #2: the new-project `+` lives ON the group
                            // header, not next to the workspace picker.
                            Button::new("sidebar-new-project")
                                .icon(IconName::Plus)
                                .ghost()
                                .xsmall()
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(NewProject), cx)
                                }),
                        ),
                )
                .child(menu.render(id, window, cx).into_any_element())
                .into_any_element(),
        }
    }
}

impl Panel for SidebarPanel {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        "Workspace"
    }

    /// Fixed chrome: the sidebar can never be closed (EXP-1 #8).
    fn closable(&self, _cx: &App) -> bool {
        false
    }

    fn zoomable(&self, _cx: &App) -> Option<PanelControl> {
        None
    }
}

impl gpui::EventEmitter<PanelEvent> for SidebarPanel {}

impl Focusable for SidebarPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SidebarPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        Sidebar::new("workspace-sidebar")
            // EXP-1 #8: NOT collapsible — the enum value exists for this.
            .collapsible(SidebarCollapsible::None)
            // Fill the dock (the Dock owns the width; its resize handle works,
            // collapse does not).
            .w_full()
            .header(self.render_header(cx))
            .child(SidebarSection::Menu(self.nav_menu(cx)))
            .child(SidebarSection::Projects(self.projects_menu(cx)))
            .footer(self.render_footer(cx))
    }
}

// Compact-density note (§4.4): interactive chrome in the shell uses
// `.small()`/`.xsmall()` per the density decision; the issue-list rows carry
// their own 28px compact height (issue_list.rs).
