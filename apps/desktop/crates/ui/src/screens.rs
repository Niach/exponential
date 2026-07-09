//! The center panel (masterplan-v3 §4.2, reworked): a TAB-BASED editor area,
//! the desktop analog of an IDE's editor tabs. Every opened issue and file
//! gets its own tab (deduped — re-opening focuses); Source Control, Settings
//! and Account are singleton tabs. Issue LISTS are not tabs: they live in
//! the sidebar tool windows and their rows open tabs here.
//!
//! One panel: a compact `TabBar` strip over content swapped on the per-window
//! [`Navigation`] state. The heavyweight views (issue detail, file viewer,
//! …) stay single instances re-pointed on tab switch — tabs remember *what*
//! is open, not per-tab view state. Closing the active tab activates its
//! neighbor; closing the last shows the empty state. A workspace switch
//! drops all tabs (they are workspace-scoped).

use gpui::{
    div, App, AppContext as _, ClickEvent, Entity, FocusHandle, Focusable, FontWeight,
    IntoElement, ParentElement, Render, SharedString, Styled, Subscription, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dock::{Panel, PanelControl, PanelEvent},
    h_flex,
    skeleton::Skeleton,
    tab::{Tab, TabBar},
    v_flex, ActiveTheme as _, Icon, IconName, Sizable as _, Size,
};
use sync::Store;

use crate::actions::NewProject;
use crate::issue_detail::IssueDetailView;
use crate::navigation::{
    active_workspace_id, nav_for_window, resolved_screen, set_screen, shapes_ready, Navigation,
    Screen,
};

/// Stable serialization name (§3.3: never change once shipped in a layout).
pub const PANEL_NAME: &str = "Screens";

pub struct ScreensPanel {
    focus_handle: FocusHandle,
    nav: Entity<Navigation>,
    issue_detail: Entity<IssueDetailView>,
    settings: Entity<crate::settings::SettingsView>,
    account: Entity<crate::settings::AccountView>,
    source_control: Entity<crate::source_control::SourceControlView>,
    file_viewer: Entity<crate::file_viewer::FileViewerView>,
    /// Open tabs in strip order — every [`Screen`] value is one tab identity
    /// (several issues / files at once; SC/settings/account dedupe).
    tabs: Vec<Screen>,
    /// The workspace the tabs belong to — a switch drops them.
    tabs_workspace: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl ScreensPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // Full-page issue detail (§4.2): one instance, re-pointed on
        // navigation (its local edit state resets per issue, web parity).
        let issue_detail = cx.new(|cx| IssueDetailView::new(window, cx));
        let settings = cx.new(|cx| crate::settings::SettingsView::new(window, cx));
        let account = cx.new(|cx| crate::settings::AccountView::new(window, cx));
        let source_control = cx.new(|cx| crate::source_control::SourceControlView::new(window, cx));
        let file_viewer = cx.new(|cx| crate::file_viewer::FileViewerView::new(window, cx));
        let nav = nav_for_window(window, cx);

        let mut subscriptions = Vec::new();
        // Navigation changes open/focus tabs and retarget the shared views
        // (needs `window` for the detail's input resets, hence `observe_in`).
        subscriptions.push(cx.observe_in(&nav, window, |this, _, window, cx| {
            this.sync_tabs(window, cx);
            cx.notify();
        }));
        let collections = Store::global(cx).collections().clone();
        subscriptions.push(cx.observe_in(
            &collections.workspaces,
            window,
            |this, _, window, cx| {
                this.sync_tabs(window, cx);
                cx.notify();
            },
        ));
        // Tab titles join issue identifiers live.
        subscriptions.push(cx.observe(&collections.issues, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.projects, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe_in(
            &Store::global(cx).state(),
            window,
            |this, _, window, cx| {
                this.sync_tabs(window, cx);
                cx.notify();
            },
        ));

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            nav,
            issue_detail,
            settings,
            account,
            source_control,
            file_viewer,
            tabs: Vec::new(),
            tabs_workspace: None,
            _subscriptions: subscriptions,
        };
        this.sync_tabs(window, cx);
        this
    }

    /// Reconcile tabs with the navigation state: drop tabs on a workspace
    /// switch, open (or keep) a tab for the active screen, and re-point the
    /// shared views at it. Runs in observers (never mid-render).
    fn sync_tabs(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let workspace = active_workspace_id(&self.nav, cx);
        if workspace != self.tabs_workspace {
            self.tabs_workspace = workspace;
            self.tabs.clear();
        }
        let Some(screen) = resolved_screen(&self.nav, cx) else {
            return;
        };
        if !self.tabs.contains(&screen) {
            self.tabs.push(screen.clone());
        }
        match screen {
            Screen::IssueDetail { issue_id } => {
                self.issue_detail
                    .update(cx, |detail, cx| detail.set_issue(issue_id, window, cx));
            }
            Screen::FileViewer { path } => {
                self.file_viewer
                    .update(cx, |viewer, cx| viewer.set_path(path, cx));
            }
            _ => {}
        }
    }

    /// Close the tab at `ix`. Closing the active tab activates its right
    /// neighbor (else the new last); closing the last clears the center.
    /// Direct tab management never touches the back stack.
    fn close_tab(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if ix >= self.tabs.len() {
            return;
        }
        let closed = self.tabs.remove(ix);
        let active = resolved_screen(&self.nav, cx);
        if active.as_ref() == Some(&closed) {
            let next = self
                .tabs
                .get(ix)
                .or_else(|| self.tabs.last())
                .cloned();
            set_screen(window, cx, next);
        }
        cx.notify();
    }

    fn tab_title(&self, screen: &Screen, cx: &App) -> SharedString {
        match screen {
            Screen::IssueDetail { issue_id } => Store::global(cx)
                .collections()
                .issues
                .read(cx)
                .get(issue_id)
                .map(|issue| SharedString::from(issue.identifier.clone()))
                .unwrap_or_else(|| "Issue".into()),
            Screen::FileViewer { path } => {
                SharedString::from(path.rsplit('/').next().unwrap_or(path).to_string())
            }
            Screen::SourceControl => "Source Control".into(),
            Screen::Settings => "Settings".into(),
            Screen::Account => "Account".into(),
        }
    }

    fn render_tab_bar(&self, active_ix: usize, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        TabBar::new("center-tabs")
            .with_size(Size::Small)
            .selected_index(active_ix)
            .on_click(cx.listener(|this, ix: &usize, window, cx| {
                if let Some(screen) = this.tabs.get(*ix).cloned() {
                    // Direct tab activation — no back-stack push.
                    set_screen(window, cx, Some(screen));
                }
            }))
            .children(self.tabs.iter().enumerate().map(|(ix, screen)| {
                Tab::new().label(self.tab_title(screen, cx)).suffix(
                    h_flex().pr_1().child(
                        Button::new(("close-center-tab", ix))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                cx.stop_propagation();
                                this.close_tab(ix, window, cx);
                            })),
                    ),
                )
            }))
    }

    /// §4.1: while the workspace/projects shapes have not caught up, render a
    /// skeleton — never a wrong empty state.
    fn render_syncing(&self, _cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        v_flex()
            .size_full()
            .p_4()
            .gap_2()
            .child(Skeleton::new().h_4().w_48())
            .child(Skeleton::new().h_4().w_64())
            .child(Skeleton::new().h_4().w_56())
            .into_any_element()
    }

    /// Nothing open: point at the sidebar (or at project creation when the
    /// workspace has none — projects may be dev/task/feedback boards, v7).
    fn render_empty(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let active_workspace = active_workspace_id(&self.nav, cx);
        let has_projects = active_workspace
            .as_deref()
            .map(|id| {
                !Store::global(cx)
                    .collections()
                    .projects_in_workspace(id, cx)
                    .is_empty()
            })
            .unwrap_or(false);
        if has_projects {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .child(
                    Icon::new(IconName::Inbox)
                        .size_6()
                        .text_color(cx.theme().muted_foreground),
                )
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .child("Nothing open"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Pick an issue from the sidebar — it opens as a tab here."),
                )
                .into_any_element();
        }
        let mut column = v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child(
                Icon::new(IconName::Folder)
                    .size_6()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child("No projects yet"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Create a project to start tracking issues — a dev board with a repository, or a repo-less task or feedback board."),
            );
        // No workspace resolves (e.g. the last one was just deleted — the
        // EXP-43 self-heal is creating a fresh personal one): the create
        // action would silently no-op, so don't offer a dead button.
        if active_workspace.is_some() {
            column = column.child(
                Button::new("screens-new-project")
                    .primary()
                    .small()
                    .label("New project…")
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(NewProject), cx);
                    }),
            );
        }
        column.into_any_element()
    }
}

impl Panel for ScreensPanel {
    fn panel_name(&self) -> &'static str {
        PANEL_NAME
    }

    fn title(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        "Workspace"
    }

    /// The screens ARE the center — closing them would leave an empty center
    /// baked into the persisted layout.
    fn closable(&self, _cx: &App) -> bool {
        false
    }

    fn zoomable(&self, _cx: &App) -> Option<PanelControl> {
        None
    }
}

impl gpui::EventEmitter<PanelEvent> for ScreensPanel {}

impl Focusable for ScreensPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ScreensPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let screen = resolved_screen(&self.nav, cx);

        // DEV-ONLY (§11.4 headless verification, EXP_DEV_* family): once a
        // project scope resolves, `EXP_DEV_CREATE_DIALOG=1` opens the
        // create-issue dialog exactly once so gate screenshots can capture it
        // without synthetic input. Unset in normal runs.
        if std::env::var("EXP_DEV_CREATE_DIALOG").as_deref() == Ok("1") {
            if let Some(project_id) = crate::navigation::active_project_id(&self.nav, cx) {
                use std::sync::atomic::{AtomicBool, Ordering};
                static FIRED: AtomicBool = AtomicBool::new(false);
                if !FIRED.swap(true, Ordering::SeqCst) {
                    cx.spawn_in(_window, async move |_this, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(1500))
                            .await;
                        let opened = cx.update(|window, cx| {
                            crate::create_issue_dialog::open(window, cx, project_id);
                        });
                        eprintln!("[exp-desktop] dev: EXP_DEV_CREATE_DIALOG fired ({opened:?})");
                    })
                    .detach();
                }
            }
        }

        let content = match &screen {
            None if !shapes_ready(cx) => self.render_syncing(cx),
            None => self.render_empty(cx),
            Some(Screen::IssueDetail { .. }) => self.issue_detail.clone().into_any_element(),
            Some(Screen::Settings) => self.settings.clone().into_any_element(),
            Some(Screen::Account) => self.account.clone().into_any_element(),
            Some(Screen::SourceControl) => self.source_control.clone().into_any_element(),
            Some(Screen::FileViewer { .. }) => self.file_viewer.clone().into_any_element(),
        };

        let active_ix = screen
            .as_ref()
            .and_then(|screen| self.tabs.iter().position(|tab| tab == screen))
            .unwrap_or(0);
        let tab_bar = (!self.tabs.is_empty()).then(|| self.render_tab_bar(active_ix, cx));

        div().size_full().bg(cx.theme().colors.list).child(
            v_flex()
                .size_full()
                .children(tab_bar)
                .child(div().flex_1().min_h_0().child(content)),
        )
    }
}
