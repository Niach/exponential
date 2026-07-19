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
//! neighbor; closing the last shows the empty state. A team switch
//! drops all tabs (they are team-scoped).

use gpui::{
    div, prelude::FluentBuilder as _, App, AppContext as _, ClickEvent, Entity, FocusHandle,
    Focusable, FontWeight, InteractiveElement as _, IntoElement, ParentElement, Render, Styled,
    Subscription, Window,
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

use crate::actions::NewBoard;
use crate::icons::ExpIcon;
use crate::issue_detail::IssueDetailView;
use crate::navigation::{
    active_team_id, nav_for_window, resolved_screen, screen_title, set_screen, shapes_ready,
    Navigation, Screen,
};

/// Stable serialization name (§3.3: never change once shipped in a layout).
pub const PANEL_NAME: &str = "Screens";

/// Per-tab hover group (EXP-65): reveals the undock button. Reused per tab —
/// gpui resolves `group_hover` against the innermost enclosing group (the
/// same idiom as the issue list's `ROW_GROUP`).
const TAB_GROUP: &str = "center-tab";

/// Build a FRESH content view for `screen` (EXP-65 undocked windows). The
/// panel's own shared single-instance views (re-pointed on tab switch) must
/// never be moved to another window; a fresh construction also binds the
/// view to the new window's per-window registries (rail, nav, resolver).
pub(crate) fn build_screen_content(
    screen: &Screen,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyView {
    match screen {
        Screen::IssueDetail { issue_id } => {
            let view = cx.new(|cx| IssueDetailView::new(window, cx));
            let issue_id = issue_id.clone();
            view.update(cx, |detail, cx| detail.set_issue(issue_id, window, cx));
            view.into()
        }
        Screen::FileViewer { path } => {
            let view = cx.new(|cx| crate::file_viewer::FileViewerView::new(window, cx));
            let path = path.clone();
            view.update(cx, |viewer, cx| viewer.set_path(path, cx));
            view.into()
        }
        Screen::SourceControl => cx
            .new(|cx| crate::source_control::SourceControlView::new(window, cx))
            .into(),
        Screen::SupportThread { thread_id } => {
            let view = cx.new(|cx| crate::support_thread::SupportThreadView::new(window, cx));
            let thread_id = thread_id.clone();
            view.update(cx, |thread, cx| thread.set_thread(thread_id, window, cx));
            view.into()
        }
        Screen::Settings => cx.new(|cx| crate::settings::SettingsView::new(window, cx)).into(),
        Screen::Account => cx.new(|cx| crate::settings::AccountView::new(window, cx)).into(),
    }
}

pub struct ScreensPanel {
    focus_handle: FocusHandle,
    nav: Entity<Navigation>,
    issue_detail: Entity<IssueDetailView>,
    settings: Entity<crate::settings::SettingsView>,
    account: Entity<crate::settings::AccountView>,
    source_control: Entity<crate::source_control::SourceControlView>,
    file_viewer: Entity<crate::file_viewer::FileViewerView>,
    /// One shared support-thread view, re-pointed on tab switch (EXP-180 —
    /// same single-instance model as the issue detail).
    support_thread: Entity<crate::support_thread::SupportThreadView>,
    /// Open tabs in strip order — every [`Screen`] value is one tab identity
    /// (several issues / files at once; SC/settings/account dedupe).
    tabs: Vec<Screen>,
    /// The team the tabs belong to — a switch drops them.
    tabs_team: Option<String>,
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
        let support_thread =
            cx.new(|cx| crate::support_thread::SupportThreadView::new(window, cx));
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
            &collections.teams,
            window,
            |this, _, window, cx| {
                this.sync_tabs(window, cx);
                cx.notify();
            },
        ));
        // Tab titles join issue identifiers live.
        subscriptions.push(cx.observe(&collections.issues, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&collections.boards, |_, _, cx| cx.notify()));
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
            support_thread,
            tabs: Vec::new(),
            tabs_team: None,
            _subscriptions: subscriptions,
        };
        this.sync_tabs(window, cx);
        this
    }

    /// Reconcile tabs with the navigation state: drop tabs on a team
    /// switch, open (or keep) a tab for the active screen, and re-point the
    /// shared views at it. Runs in observers (never mid-render).
    fn sync_tabs(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        // EXP-48 prev/next: an in-place `replace_screen` marks the screen it
        // displaced — consume the marker so that tab's identity swaps instead
        // of a new tab opening per step.
        let replaced = crate::navigation::take_replaced_screen(&self.nav, cx);
        let team = active_team_id(&self.nav, cx);
        if team != self.tabs_team {
            // Dropping the tabs tears the issue detail down without a blur —
            // flush a pending description edit first (EXP-68).
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
            self.tabs_team = team;
            self.tabs.clear();
        }
        let Some(screen) = resolved_screen(&self.nav, cx) else {
            return;
        };
        if !self.tabs.contains(&screen) {
            let replaced_ix = replaced
                .and_then(|old| self.tabs.iter().position(|tab| *tab == old));
            match replaced_ix {
                Some(ix) => self.tabs[ix] = screen.clone(),
                None => self.tabs.push(screen.clone()),
            }
        }
        match screen {
            Screen::IssueDetail { issue_id } => {
                self.issue_detail.update(cx, |detail, cx| {
                    detail.set_issue(issue_id, window, cx);
                });
            }
            Screen::FileViewer { path } => {
                self.file_viewer
                    .update(cx, |viewer, cx| viewer.set_path(path, cx));
            }
            Screen::SupportThread { thread_id } => {
                // Re-pointing also restarts the 15s poll on tab reactivation.
                self.support_thread
                    .update(cx, |thread, cx| thread.set_thread(thread_id, window, cx));
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
        // Closing (or undocking) the active issue tab unmounts the detail's
        // description editor without a blur — flush the pending edit so it
        // is written before teardown (EXP-68).
        if matches!(self.tabs[ix], Screen::IssueDetail { .. }) {
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
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

    /// Undock the tab at `ix` into its own native window (EXP-65): open (or
    /// focus) the undocked window, then close the tab here — the screen now
    /// lives in that window until reattached.
    fn undock_tab(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(screen) = self.tabs.get(ix).cloned() else {
            return;
        };
        crate::undock::open_undocked_screen(screen, window.window_handle(), cx);
        self.close_tab(ix, window, cx);
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
                Tab::new().group(TAB_GROUP).label(screen_title(screen, cx)).suffix(
                    h_flex()
                        .pr_1()
                        .gap_0p5()
                        // Hover-revealed undock (EXP-65): `invisible` keeps
                        // the layout slot so tabs don't jitter on hover.
                        .when(screen.undockable(), |this| {
                            this.child(
                                div()
                                    .invisible()
                                    .group_hover(TAB_GROUP, |style| style.visible())
                                    .child(
                                        Button::new(("undock-center-tab", ix))
                                            .ghost()
                                            .xsmall()
                                            .icon(ExpIcon::ExternalLink)
                                            .tooltip("Open in new window")
                                            .on_click(cx.listener(
                                                move |this, _: &ClickEvent, window, cx| {
                                                    cx.stop_propagation();
                                                    this.undock_tab(ix, window, cx);
                                                },
                                            )),
                                    ),
                            )
                        })
                        .child(
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

    /// §4.1: while the team/boards shapes have not caught up, render a
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

    /// Nothing open: point at the sidebar (or at board creation when the
    /// team has none).
    fn render_empty(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        let active_team = active_team_id(&self.nav, cx);
        let has_boards = active_team
            .as_deref()
            .map(|id| {
                !Store::global(cx)
                    .collections()
                    .boards_in_team(id, cx)
                    .is_empty()
            })
            .unwrap_or(false);
        if has_boards {
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
                    .child("No boards yet"),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Create a board to start tracking issues. Connect a repository to code on it."),
            );
        // No team resolves (e.g. the last one was just deleted — the
        // EXP-43 self-heal is creating a fresh personal one): the create
        // action would silently no-op, so don't offer a dead button.
        if active_team.is_some() {
            column = column.child(
                Button::new("screens-new-board")
                    .primary()
                    .small()
                    .label("New board…")
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(NewBoard), cx);
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
        "Team"
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
        // board scope resolves, `EXP_DEV_CREATE_DIALOG=1` opens the
        // create-issue dialog exactly once so gate screenshots can capture it
        // without synthetic input. Unset in normal runs.
        if std::env::var("EXP_DEV_CREATE_DIALOG").as_deref() == Ok("1") {
            if let Some(board_id) = crate::navigation::active_board_id(&self.nav, cx) {
                use std::sync::atomic::{AtomicBool, Ordering};
                static FIRED: AtomicBool = AtomicBool::new(false);
                if !FIRED.swap(true, Ordering::SeqCst) {
                    cx.spawn_in(_window, async move |_this, cx| {
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(1500))
                            .await;
                        let opened = cx.update(|window, cx| {
                            crate::create_issue_dialog::open(window, cx, board_id);
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
            Some(Screen::SupportThread { .. }) => {
                self.support_thread.clone().into_any_element()
            }
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
