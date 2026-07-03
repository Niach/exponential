//! The workspace sidebar (masterplan-v3 §4.2 "Workspace shell", §4.8 EXP-1).
//!
//! Lives in the `DockArea`'s **left dock** as a chrome-less `DockItem::Panel`
//! (no tab/title bar) and is **non-collapsible** (`SidebarCollapsible::None`,
//! EXP-1 #8). Contents top-to-bottom per §4.2:
//!
//! - workspace-picker `DropdownMenu` (EXP-1 #1 — shadcn dropdown, not a
//!   native menu),
//! - first-class nav rows **My Issues / Inbox / Search** (EXP-1 #3),
//! - a "Projects" group with the **`+` new-project button on the group
//!   header** (EXP-1 #2),
//! - footer: a **Send Feedback** direct item (EXP-1 #10) and the
//!   **account/settings dropdown** (EXP-1 #11 — Settings lives here, not only
//!   in a system menubar).
//!
//! Phase-1 scope: static/dummy content — the workspace name, project list,
//! and account identity are placeholders until the Phase-2 sync collections
//! and auth land. The footer also renders the live shared-`Store` window
//! count, which is the §3.10 multi-window shared-state proof (opening a
//! second window updates the first window's sidebar).

use gpui::{
    div, App, ClickEvent, ElementId, FocusHandle, Focusable, FontWeight, IntoElement,
    ParentElement, Render, SharedString, Styled, Window,
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
    v_flex, ActiveTheme as _, Collapsible, Icon, IconName, Sizable as _,
};
use sync::Store;

use crate::actions::{
    CreateWorkspace, NewProject, OpenInbox, OpenMyIssues, OpenSearch, OpenSettings, SelectWorkspace,
    SendFeedback, SignOut,
};

/// Stable serialization name (§3.3: "Once you have defined a panel name, this
/// must not be changed").
pub const PANEL_NAME: &str = "Sidebar";

/// The left-dock sidebar panel. Implements [`Panel`] because everything
/// dockable must (§3.3), but renders chrome-less via `DockItem::Panel`.
pub struct SidebarPanel {
    focus_handle: FocusHandle,
}

impl SidebarPanel {
    pub fn new(_window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // Re-render on shared-store changes (the window count in the footer —
        // the Phase-1 shared-state proof; Phase 3 observes real collections).
        let shared = Store::global(cx).state();
        cx.observe(&shared, |_, _, cx| cx.notify()).detach();

        Self {
            focus_handle: cx.focus_handle(),
        }
    }

    /// Workspace picker placeholder (EXP-1 #1): shadcn-style dropdown on the
    /// sidebar header. Real workspace list arrives with the Phase-2 store.
    fn render_header(&self, cx: &App) -> impl IntoElement {
        SidebarHeader::new()
            .child(
                h_flex()
                    .gap_2()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size_6()
                            .flex_shrink_0()
                            .rounded(cx.theme().radius)
                            .bg(cx.theme().sidebar_primary)
                            .text_color(cx.theme().sidebar_primary_foreground)
                            .child(Icon::new(IconName::GalleryVerticalEnd).xsmall()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child("Exponential"),
                    ),
            )
            .child(
                Icon::new(IconName::ChevronsUpDown)
                    .xsmall()
                    .text_color(cx.theme().sidebar_foreground.opacity(0.7)),
            )
            .dropdown_menu(|menu, _window, _cx| {
                menu.label("Workspaces")
                    .menu_with_check("Exponential", true, Box::new(SelectWorkspace))
                    .separator()
                    .menu_with_icon("Create workspace…", IconName::Plus, Box::new(CreateWorkspace))
            })
    }

    /// First-class nav rows (EXP-1 #3): My Issues, Inbox, Search.
    fn nav_menu() -> SidebarMenu {
        SidebarMenu::new()
            .child(
                SidebarMenuItem::new("My Issues")
                    .icon(IconName::CircleUser)
                    .on_click(dispatch(OpenMyIssues)),
            )
            .child(
                SidebarMenuItem::new("Inbox")
                    .icon(IconName::Inbox)
                    .on_click(dispatch(OpenInbox)),
            )
            .child(
                SidebarMenuItem::new("Search")
                    .icon(IconName::Search)
                    .on_click(dispatch(OpenSearch)),
            )
    }

    /// Projects group placeholder — the real project rows read the Phase-2
    /// `projects` collection.
    fn projects_menu() -> SidebarMenu {
        SidebarMenu::new().child(SidebarMenuItem::new("No projects yet").disable(true))
    }

    /// Footer: Send Feedback direct item (EXP-1 #10), the account/settings
    /// dropdown (EXP-1 #11), and the shared-store status line (§3.10 proof).
    fn render_footer(&self, cx: &App) -> impl IntoElement {
        let state = Store::global(cx).state();
        let windows_open = state.read(cx).windows_open;
        // The sidebar only renders when Synced (the workspace swaps to the
        // login surface otherwise), so show who is signed in.
        let who: SharedString = state
            .read(cx)
            .session
            .account_id()
            .and_then(|account_id| {
                cx.try_global::<crate::AuthContext>()
                    .and_then(|auth| auth.auth.account(account_id))
                    .map(|account| SharedString::from(account.email))
            })
            .unwrap_or_else(|| "Not signed in".into());
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
                                    .child("Guest"),
                            ),
                    )
                    .child(
                        Icon::new(IconName::ChevronsUpDown)
                            .xsmall()
                            .text_color(cx.theme().sidebar_foreground.opacity(0.7)),
                    )
                    .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, |menu, _window, _cx| {
                        menu.menu_with_icon("Settings", IconName::Settings, Box::new(OpenSettings))
                            .separator()
                            .menu("Sign out", Box::new(SignOut))
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
            .child(SidebarSection::Menu(Self::nav_menu()))
            .child(SidebarSection::Projects(Self::projects_menu()))
            .footer(self.render_footer(cx))
    }
}

// Compact-density note (§4.4): interactive chrome in the shell uses
// `.small()`/`.xsmall()` per the density decision; row heights and the
// `sm_button()`-style prelude helpers land with the Phase-3 screens.
