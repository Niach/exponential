//! The center screens panel (masterplan-v3 §4.2) — the desktop analog of the
//! web's routed center content: sidebar selection swaps this panel between
//! the project board, issue detail, My Issues, Inbox, Settings and Account.
//!
//! One panel, content swapped on the per-window [`Navigation`] state — the
//! §3.3 dock layout stays stable (one center `TabPanel` child) while the
//! screen inside changes, exactly like the web's `<Outlet/>`.
//!
//! Phase-3 state: every §4.2 surface renders real — board, full issue detail
//! (markdown editor + timeline + properties), My Issues, Inbox, Settings and
//! Account. Default-screen resolution honors §4.1 `is_ready` — an unsynced
//! workspace shows a skeleton, never a wrong default or a false empty state.

use gpui::{
    div, App, AppContext as _, Entity, FocusHandle, Focusable, IntoElement, ParentElement, Render,
    Styled, Subscription, Window,
};
use gpui_component::{
    dock::{Panel, PanelControl, PanelEvent},
    skeleton::Skeleton,
    v_flex, ActiveTheme as _,
};
use sync::Store;

use crate::board::BoardView;
use crate::issue_detail::IssueDetailView;
use crate::issue_list::IssueQuery;
use crate::navigation::{nav_for_window, resolved_screen, Navigation, Screen};

/// Stable serialization name (§3.3: never change once shipped in a layout).
pub const PANEL_NAME: &str = "Screens";

pub struct ScreensPanel {
    focus_handle: FocusHandle,
    nav: Entity<Navigation>,
    board: Entity<BoardView>,
    issue_detail: Entity<IssueDetailView>,
    my_issues: Entity<crate::my_issues::MyIssuesView>,
    inbox: Entity<crate::inbox::InboxView>,
    settings: Entity<crate::settings::SettingsView>,
    account: Entity<crate::settings::AccountView>,
    _subscriptions: Vec<Subscription>,
}

impl ScreensPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let board = cx.new(|cx| BoardView::new(window, cx));
        // Full-page issue detail (§4.2): one instance, re-pointed on
        // navigation (its local edit state resets per issue, web parity).
        let issue_detail = cx.new(|cx| IssueDetailView::new(window, cx));
        // My Issues + Inbox (§4.2): self-contained views that track the
        // active workspace/account themselves.
        let my_issues = cx.new(|cx| crate::my_issues::MyIssuesView::new(window, cx));
        let inbox = cx.new(|cx| crate::inbox::InboxView::new(window, cx));
        // Settings + Account (§4.2): constructed with the panel (cheap —
        // their server-only reads fetch lazily on first render).
        let settings = cx.new(|cx| crate::settings::SettingsView::new(window, cx));
        let account = cx.new(|cx| crate::settings::AccountView::new(window, cx));

        let mut subscriptions = Vec::new();
        // Navigation changes swap the center content (and retarget the
        // board list / detail view — needs `window` for the detail's input
        // state resets, hence `observe_in`).
        subscriptions.push(cx.observe_in(&nav, window, |this, _, window, cx| {
            this.sync_screen_targets(window, cx);
            cx.notify();
        }));
        // Default-screen resolution depends on workspaces + projects; the
        // session switch on the shared state re-resolves after (re-)login.
        let collections = Store::global(cx).collections().clone();
        subscriptions.push(cx.observe_in(
            &collections.workspaces,
            window,
            |this, _, window, cx| {
                this.sync_screen_targets(window, cx);
                cx.notify();
            },
        ));
        subscriptions.push(cx.observe_in(
            &collections.projects,
            window,
            |this, _, window, cx| {
                this.sync_screen_targets(window, cx);
                cx.notify();
            },
        ));
        subscriptions.push(cx.observe_in(
            &Store::global(cx).state(),
            window,
            |this, _, window, cx| {
                this.sync_screen_targets(window, cx);
                cx.notify();
            },
        ));

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            nav,
            board,
            issue_detail,
            my_issues,
            inbox,
            settings,
            account,
            _subscriptions: subscriptions,
        };
        this.sync_screen_targets(window, cx);
        this
    }

    /// Keep the board's issue list and the detail view pointed at the
    /// resolved screen's scope. Runs in observers (never mid-render) so
    /// entity updates are clean. (My Issues owns its own list and tracks its
    /// scope itself.)
    fn sync_screen_targets(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match resolved_screen(&self.nav, cx) {
            Some(Screen::Board { project_id }) => {
                self.board.update(cx, |board, cx| {
                    board.set_query(IssueQuery::Project { project_id }, cx)
                });
            }
            Some(Screen::IssueDetail { issue_id }) => {
                self.issue_detail
                    .update(cx, |detail, cx| detail.set_issue(issue_id, window, cx));
            }
            _ => {} // other screens do not use these; keep them parked
        }
    }

    // -- screen bodies -------------------------------------------------------

    /// The §4.2 board (filter bar + pills + virtualized list). Web parity:
    /// the bar IS the top of the page — no extra screen header (the web
    /// route's `h1` lives inside `IssueFilterBar`).
    fn render_board(&self, _project_id: &str, _cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        self.board.clone().into_any_element()
    }

    /// §4.1: while `resolved_screen` is `None` the workspace/projects shapes
    /// have not caught up — skeleton, never a default screen guess.
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
        let content = match screen {
            None => self.render_syncing(cx),
            Some(Screen::Board { project_id }) => self.render_board(&project_id, cx),
            Some(Screen::MyIssues) => self.my_issues.clone().into_any_element(),
            Some(Screen::IssueDetail { .. }) => self.issue_detail.clone().into_any_element(),
            Some(Screen::Inbox) => self.inbox.clone().into_any_element(),
            Some(Screen::Settings) => self.settings.clone().into_any_element(),
            Some(Screen::Account) => self.account.clone().into_any_element(),
        };

        div()
            .size_full()
            .bg(cx.theme().colors.list)
            .child(content)
    }
}
