//! The center panel (masterplan-v3 §4.2, reworked twice — EXP-288): a
//! TAB-BASED editor area whose tabs are DETAIL VIEWS ONLY (issue detail,
//! PR diff, support thread, action detail). High-level surfaces get no
//! tabs: Source Control's diff and the file viewer are the center content
//! their rail tool shows (driven by the sidebar's commit/file selection),
//! and Settings/Account are tab-less full-screen modes. Every tab REMEMBERS
//! the sidebar entry it was opened from ([`TabEntry::origin`]) — clicking a
//! tab re-selects that entry (and board) so the sidebar always shows the
//! list the tab came from.
//!
//! One panel: a compact chip strip over content swapped on the per-window
//! [`Navigation`] state. The heavyweight views (issue detail, file viewer,
//! …) stay single instances re-pointed on tab switch — tabs remember *what*
//! is open, not per-tab view state. Closing the active tab activates its
//! neighbor; closing the last shows the active tool's default center. A
//! team switch drops all tabs (they are team-scoped). Tabs that don't fit
//! the strip collapse into a "+N" overflow menu (EXP-288).

use std::collections::HashMap;

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, ClickEvent, Entity, FocusHandle,
    Focusable, FontWeight, InteractiveElement as _, IntoElement, MouseButton, ParentElement,
    Render, StatefulInteractiveElement as _, Styled, Subscription, Window, WindowId,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    dock::{Panel, PanelControl, PanelEvent},
    h_flex,
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    skeleton::Skeleton,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;


use crate::actions::{CreateTeam, JoinTeam, NewBoard};
use crate::icons::{registry, ExpIcon};
use crate::issue_detail::IssueDetailView;
use crate::navigation::{
    active_board_id, active_team_id, nav_for_window, resolved_screen, screen_title, set_screen,
    shapes_ready, Navigation, PendingOrigin, Screen, TabOrigin,
};
use crate::sidebar::{rail_shared_for_window, RailShared, ToolWindow};

/// Stable serialization name (§3.3: never change once shipped in a layout).
pub const PANEL_NAME: &str = "Screens";

/// Per-tab hover group (EXP-65): reveals the undock button. Reused per tab —
/// gpui resolves `group_hover` against the innermost enclosing group (the
/// same idiom as the issue list's `ROW_GROUP`).
const TAB_GROUP: &str = "center-tab";

/// EXP-277: per-window handle to the center [`ScreensPanel`] so the titlebar
/// ([`crate::app_title_bar::AppTitleBar`]) can host the tab strip. Mirrors
/// `sidebar::RailRegistry` / `navigation`'s per-window globals. Shell windows
/// only — undocked windows have no center panel and no strip.
#[derive(Default)]
struct ScreensRegistry {
    by_window: HashMap<WindowId, Entity<ScreensPanel>>,
}

impl gpui::Global for ScreensRegistry {}

/// This window's center screens panel, if one exists yet.
pub(crate) fn screens_for_window(window: &Window, cx: &App) -> Option<Entity<ScreensPanel>> {
    cx.try_global::<ScreensRegistry>()
        .and_then(|registry| registry.by_window.get(&window.window_handle().window_id()).cloned())
}

/// Drop a closed window's entry (called from the `Shell` release hook,
/// mirroring `sidebar::remove_window`).
pub(crate) fn remove_window(window_id: WindowId, cx: &mut App) {
    if let Some(registry) = cx.try_global::<ScreensRegistry>() {
        if registry.by_window.contains_key(&window_id) {
            cx.global_mut::<ScreensRegistry>().by_window.remove(&window_id);
        }
    }
}

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
        Screen::SupportThread { thread_id } => {
            let view = cx.new(|cx| crate::support_thread::SupportThreadView::new(window, cx));
            let thread_id = thread_id.clone();
            view.update(cx, |thread, cx| thread.set_thread(thread_id, window, cx));
            view.into()
        }
        Screen::PrDiff { issue_id } => {
            let view = cx.new(|cx| crate::pr_diff::PrDiffView::new(window, cx));
            let issue_id = issue_id.clone();
            view.update(cx, |diff, cx| diff.set_issue(issue_id, cx));
            view.into()
        }
        Screen::ActionDetail { action_id } => {
            let view = cx.new(|cx| crate::action_detail::ActionDetailView::new(window, cx));
            let action_id = action_id.clone();
            view.update(cx, |detail, cx| detail.set_action(action_id, cx));
            view.into()
        }
        // Never undockable — unreachable via the undock path, kept total for
        // the compiler.
        Screen::Settings => cx.new(|cx| crate::settings::SettingsView::new(window, cx)).into(),
        Screen::Account => cx.new(|cx| crate::settings::AccountView::new(window, cx)).into(),
    }
}

/// One open tab: the detail screen it shows plus the sidebar entry it was
/// opened from (EXP-288 — activating the tab re-selects that entry).
#[derive(Clone)]
struct TabEntry {
    screen: Screen,
    origin: TabOrigin,
}

/// Chip content for one tab (EXP-310): issue-backed tabs (issue detail + PR
/// diff) lead with the colored status icon and the identifier shortcode. A
/// blank issue title drops the title part — the shortcode already labels the
/// chip, where `screen_title`'s identifier fallback would render it twice.
/// Non-issue tabs (and issue rows not yet synced) keep the plain
/// `screen_title`.
struct ChipContent {
    /// EXP-314: the issue's RESOLVED status (custom rows included).
    /// Resolution is per-issue, so it stays correct on this cross-team strip
    /// — only GROUPING is team-scoped.
    status: Option<domain::statuses::ResolvedStatus>,
    identifier: Option<gpui::SharedString>,
    title: Option<gpui::SharedString>,
}

fn chip_content(screen: &Screen, cx: &App) -> ChipContent {
    if let Screen::IssueDetail { issue_id } | Screen::PrDiff { issue_id } = screen {
        let store = Store::global(cx);
        let issues = store.collections().issues.read(cx);
        if let Some(issue) = issues.get(issue_id) {
            let title = issue.title.trim();
            // "· Diff" keeps the diff tab distinguishable from the same
            // issue's detail tab (mirrors `screen_title`).
            let title = match screen {
                Screen::PrDiff { .. } if title.is_empty() => Some("Diff".into()),
                Screen::PrDiff { .. } => {
                    Some(gpui::SharedString::from(format!("{title} · Diff")))
                }
                _ if title.is_empty() => None,
                _ => Some(gpui::SharedString::from(title.to_string())),
            };
            let resolved = crate::queries::resolve_issue_status(cx, issue);
            return ChipContent {
                status: Some(resolved),
                identifier: Some(gpui::SharedString::from(issue.identifier.clone())),
                title,
            };
        }
    }
    ChipContent {
        status: None,
        identifier: None,
        title: Some(screen_title(screen, cx)),
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
    /// One shared PR diff view, re-pointed on tab switch (EXP-181 — the
    /// Reviews rows' target).
    pr_diff: Entity<crate::pr_diff::PrDiffView>,
    /// One shared action detail view, re-pointed on tab switch (EXP-277 —
    /// the Actions tool-window rows' target).
    action_detail: Entity<crate::action_detail::ActionDetailView>,
    /// The window's shared rail state (EXP-288): the active tool drives the
    /// tab-less center default (SC diff / file viewer), and the file
    /// selection re-points the viewer.
    rail: Entity<RailShared>,
    /// Open tabs in strip order — detail screens only, deduped by `screen`
    /// (several issues at once; re-opening focuses + refreshes the origin).
    tabs: Vec<TabEntry>,
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
        let pr_diff = cx.new(|cx| crate::pr_diff::PrDiffView::new(window, cx));
        let action_detail = cx.new(|cx| crate::action_detail::ActionDetailView::new(window, cx));
        let nav = nav_for_window(window, cx);
        let rail = rail_shared_for_window(window, cx);

        let mut subscriptions = Vec::new();
        // Navigation changes open/focus tabs and retarget the shared views
        // (needs `window` for the detail's input resets, hence `observe_in`).
        subscriptions.push(cx.observe_in(&nav, window, |this, _, window, cx| {
            this.sync_tabs(window, cx);
            cx.notify();
        }));
        // EXP-288: the rail drives the tab-less center — tool switches swap
        // the default content, and the Files selection re-points the viewer.
        // Loop-safe: this only updates the leaf viewer entity.
        subscriptions.push(cx.observe(&rail, |this, _, cx| {
            this.sync_file_viewer(cx);
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

        // EXP-277: publish this window's panel so the titlebar can render the
        // tab strip (insert overwrites — a rebuilt center wins).
        let panel_entity = cx.entity();
        cx.default_global::<ScreensRegistry>()
            .by_window
            .insert(window.window_handle().window_id(), panel_entity);

        let mut this = Self {
            focus_handle: cx.focus_handle(),
            nav,
            issue_detail,
            settings,
            account,
            source_control,
            file_viewer,
            support_thread,
            pr_diff,
            action_detail,
            rail,
            tabs: Vec::new(),
            tabs_team: None,
            _subscriptions: subscriptions,
        };
        this.sync_tabs(window, cx);
        this.sync_file_viewer(cx);
        this
    }

    /// Re-point the file viewer at the rail's file selection (EXP-288 —
    /// files are not tabs; the viewer is the Files tool's center content).
    fn sync_file_viewer(&mut self, cx: &mut gpui::Context<Self>) {
        let selected = self
            .rail
            .read(cx)
            .selected_file()
            .map(str::to_string);
        self.file_viewer.update(cx, |viewer, cx| match selected {
            Some(path) => viewer.set_path(path, cx),
            None => viewer.clear(cx),
        });
    }

    /// Reconcile tabs with the navigation state: drop tabs on a team
    /// switch, open (or keep) a tab for the active DETAIL screen, and
    /// re-point the shared views at it. Runs in observers (never
    /// mid-render). MUST never call `activate_tool`/`select_*` — the rail
    /// observer + this nav observer would feed back.
    fn sync_tabs(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        // EXP-48 prev/next: an in-place `replace_screen` marks the screen it
        // displaced — consume the marker so that tab's identity swaps instead
        // of a new tab opening per step. EXP-288: the origin marker rides
        // every REAL navigation (tab clicks never set it).
        let replaced = crate::navigation::take_replaced_screen(&self.nav, cx);
        let pending_origin = crate::navigation::take_pending_origin(&self.nav, cx);
        let team = active_team_id(&self.nav, cx);
        if team != self.tabs_team {
            // Dropping the tabs tears the issue / action detail down without a
            // blur — flush a pending description or prompt edit first (EXP-68).
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
            self.action_detail
                .update(cx, |detail, cx| detail.flush_body(cx));
            self.tabs_team = team;
            self.tabs.clear();
            // The sidebar selections are team-scoped too (trunk-relative
            // paths / commit hashes of the OLD team's clone).
            self.rail.update(cx, |rail, cx| {
                rail.clear_selected_file(cx);
                rail.clear_sc_selected_commit(cx);
            });
        }
        let Some(screen) = resolved_screen(&self.nav, cx) else {
            return;
        };
        // EXP-288: only detail views are tabs — Settings/Account render
        // tab-less (no chip, nothing highlighted).
        if !screen.is_detail() {
            return;
        }
        // Resolve the origin: an explicit one wins; Capture reads the rail
        // tool + active board at consume time (the row click that navigated
        // ran with its tool already active). `None` (go-back / tab
        // reactivation of a closed tab) falls back to Capture too.
        let captured = {
            let tool = self.rail.read(cx).tool();
            TabOrigin {
                board_id: (tool == ToolWindow::BoardIssues)
                    .then(|| active_board_id(&self.nav, cx))
                    .flatten(),
                tool,
            }
        };
        match self.tabs.iter().position(|tab| tab.screen == screen) {
            Some(ix) => {
                // Dedupe keeps ONE tab; a real re-navigation refreshes its
                // origin (LATEST origin wins), a plain activation keeps it.
                if let Some(pending) = pending_origin {
                    self.tabs[ix].origin = match pending {
                        PendingOrigin::Explicit(origin) => origin,
                        PendingOrigin::Capture => captured,
                    };
                }
            }
            None => {
                let origin = match pending_origin {
                    Some(PendingOrigin::Explicit(origin)) => origin,
                    _ => captured,
                };
                let entry = TabEntry {
                    screen: screen.clone(),
                    origin,
                };
                // EXP-48: a replace_screen swap keeps the displaced tab's
                // slot (and, deliberately, position) instead of appending.
                let replaced_ix = replaced
                    .and_then(|old| self.tabs.iter().position(|tab| tab.screen == old));
                match replaced_ix {
                    Some(ix) => {
                        // Identity swaps in place; the origin is preserved
                        // (prev/next walks the SAME list the tab came from).
                        self.tabs[ix].screen = entry.screen;
                    }
                    None => self.tabs.push(entry),
                }
            }
        }
        match screen {
            Screen::IssueDetail { issue_id } => {
                self.issue_detail.update(cx, |detail, cx| {
                    detail.set_issue(issue_id, window, cx);
                });
            }
            Screen::SupportThread { thread_id } => {
                // Re-pointing also restarts the 15s poll on tab reactivation.
                self.support_thread
                    .update(cx, |thread, cx| thread.set_thread(thread_id, window, cx));
            }
            Screen::PrDiff { issue_id } => {
                self.pr_diff
                    .update(cx, |diff, cx| diff.set_issue(issue_id, cx));
            }
            Screen::ActionDetail { action_id } => {
                self.action_detail
                    .update(cx, |detail, cx| detail.set_action(action_id, cx));
            }
            Screen::Settings | Screen::Account => unreachable!("filtered by is_detail"),
        }
    }

    /// Activate the tab at `ix`: re-select its origin sidebar entry (and
    /// board), then show its screen (EXP-288). Order matters — tool, board,
    /// then screen — so observers reading tool/board during the nav notify
    /// see final state. Deliberately NOT `navigate`: activation must never
    /// rewrite the tab's remembered origin.
    fn activate_tab(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(entry) = self.tabs.get(ix).cloned() else {
            return;
        };
        crate::sidebar::select_tool_for_tab(window, cx, entry.origin.tool);
        if entry.origin.tool == ToolWindow::BoardIssues {
            if let Some(board_id) = entry.origin.board_id {
                // Degrades safely if the board has since been trashed —
                // `active_board_id` existence-checks at query time.
                crate::navigation::set_active_board(window, cx, board_id);
            }
        }
        set_screen(window, cx, Some(entry.screen));
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
        // is written before teardown (EXP-68). EXP-298: the action detail's
        // prompt editor is the same story.
        if matches!(self.tabs[ix].screen, Screen::IssueDetail { .. }) {
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
        }
        if matches!(self.tabs[ix].screen, Screen::ActionDetail { .. }) {
            self.action_detail
                .update(cx, |detail, cx| detail.flush_body(cx));
        }
        let closed = self.tabs.remove(ix);
        let active = resolved_screen(&self.nav, cx);
        if active.as_ref() == Some(&closed.screen) {
            let next = self
                .tabs
                .get(ix)
                .or_else(|| self.tabs.last())
                .map(|tab| tab.screen.clone());
            set_screen(window, cx, next);
        }
        cx.notify();
    }

    /// Close every tab except `ix` (EXP-235 context menu). The kept tab
    /// becomes active — the active tab may be among the closed ones.
    fn close_other_tabs(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if ix >= self.tabs.len() || self.tabs.len() <= 1 {
            return;
        }
        let keep = self.tabs[ix].screen.clone();
        // Same EXP-68 flush as `close_tab`: a closing issue tab may hold a
        // pending description edit, an action tab a pending prompt edit.
        if self
            .tabs
            .iter()
            .any(|tab| tab.screen != keep && matches!(tab.screen, Screen::IssueDetail { .. }))
        {
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
        }
        if self
            .tabs
            .iter()
            .any(|tab| tab.screen != keep && matches!(tab.screen, Screen::ActionDetail { .. }))
        {
            self.action_detail
                .update(cx, |detail, cx| detail.flush_body(cx));
        }
        self.tabs.retain(|tab| tab.screen == keep);
        set_screen(window, cx, Some(keep));
        cx.notify();
    }

    /// Close every tab (EXP-235 context menu) and clear the center.
    fn close_all_tabs(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        if self
            .tabs
            .iter()
            .any(|tab| matches!(tab.screen, Screen::IssueDetail { .. }))
        {
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
        }
        if self
            .tabs
            .iter()
            .any(|tab| matches!(tab.screen, Screen::ActionDetail { .. }))
        {
            self.action_detail
                .update(cx, |detail, cx| detail.flush_body(cx));
        }
        self.tabs.clear();
        set_screen(window, cx, None);
        cx.notify();
    }

    /// Undock the tab at `ix` into its own native window (EXP-65): open (or
    /// focus) the undocked window, then close the tab here — the screen now
    /// lives in that window until reattached.
    fn undock_tab(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(screen) = self.tabs.get(ix).map(|tab| tab.screen.clone()) else {
            return;
        };
        crate::undock::open_undocked_screen(screen, window.window_handle(), cx);
        self.close_tab(ix, window, cx);
    }

    /// Estimated chip width for the overflow computation (gpui has no
    /// pre-layout measurement here): a rough glyph-width times the label
    /// length, clamped to the chip's real max, plus the fixed chrome
    /// (padding + close button + reserved undock slot + gaps).
    fn estimate_chip_width(&self, entry: &TabEntry, cx: &App) -> f32 {
        let content = chip_content(&entry.screen, cx);
        let title_w = content
            .title
            .as_ref()
            .map(|title| (title.chars().count() as f32 * 7.5).clamp(40., 180.))
            .unwrap_or(0.);
        // EXP-310 issue chrome: xsmall status icon and the mono text_xs
        // shortcode, each plus the chip gap.
        let icon_w = if content.status.is_some() { 18. } else { 0. };
        let identifier_w = content
            .identifier
            .as_ref()
            .map(|identifier| identifier.chars().count() as f32 * 6.2 + 4.)
            .unwrap_or(0.);
        title_w + icon_w + identifier_w + 68.
    }

    /// EXP-277: the hand-rolled rounded tab strip. Hosted INSIDE the titlebar
    /// (via [`screens_for_window`] from `AppTitleBar`) when the window paints
    /// its own chrome; falls back to the legacy in-panel position under Linux
    /// server-side decorations (where the titlebar is hidden). Chips are plain
    /// stateful divs, so the EXP-235 context-menu overlay hack is gone — the
    /// menu attaches directly.
    ///
    /// EXP-288: tabs that don't fit `available` collapse into a trailing
    /// "+N" dropdown of the hidden tabs (no more cut-off horizontal scroll);
    /// the ACTIVE tab is always kept visible (it displaces the last fitting
    /// chip). `available` is the caller's width estimate — heuristic, with
    /// `max_w_full` as the safety net.
    pub(crate) fn render_tab_strip(
        &mut self,
        available: gpui::Pixels,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        if self.tabs.is_empty() {
            return gpui::Empty.into_any_element();
        }
        let active_ix = resolved_screen(&self.nav, cx)
            .and_then(|screen| self.tabs.iter().position(|tab| tab.screen == screen));
        let panel = cx.entity().downgrade();
        let tab_count = self.tabs.len();

        // --- overflow partition (estimate-based) --------------------------
        const CHIP_GAP: f32 = 4.; // .gap_1()
        const OVERFLOW_BUTTON_W: f32 = 48.;
        let widths: Vec<f32> = self
            .tabs
            .iter()
            .map(|entry| self.estimate_chip_width(entry, cx))
            .collect();
        let total: f32 =
            widths.iter().sum::<f32>() + CHIP_GAP * (tab_count.saturating_sub(1)) as f32;
        let available_f = f32::from(available).max(0.);
        let mut visible: Vec<usize>;
        if total <= available_f {
            visible = (0..tab_count).collect();
        } else {
            let budget = (available_f - OVERFLOW_BUTTON_W - CHIP_GAP).max(0.);
            visible = Vec::new();
            let mut used = 0.;
            for (ix, width) in widths.iter().enumerate() {
                let next = used + width + if visible.is_empty() { 0. } else { CHIP_GAP };
                if next > budget && !visible.is_empty() {
                    break;
                }
                visible.push(ix);
                used = next;
            }
            if visible.is_empty() {
                visible.push(0);
            }
            // The active tab must always be visible: swap it into the last
            // visible slot (display only — `self.tabs` keeps its order).
            if let Some(active) = active_ix {
                if !visible.contains(&active) {
                    *visible.last_mut().expect("non-empty") = active;
                }
            }
        }
        let hidden: Vec<usize> = (0..tab_count).filter(|ix| !visible.contains(ix)).collect();
        let chips: Vec<(usize, Screen)> = visible
            .iter()
            .map(|&ix| (ix, self.tabs[ix].screen.clone()))
            .collect();

        let mut strip = h_flex()
            .id("center-tab-strip")
            .max_w_full()
            .gap_1()
            .items_center()
            .children(chips.into_iter().map(|(ix, screen)| {
                let screen = &screen;
                let content = chip_content(screen, cx);
                crate::surface::tab_chip(Some(ix) == active_ix, cx)
                    .id(("center-tab", ix))
                    .group(TAB_GROUP)
                    // Tab activation re-selects the tab's origin sidebar
                    // entry, then shows the screen (EXP-288) — never a
                    // back-stack push, never an origin rewrite.
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.activate_tab(ix, window, cx);
                    }))
                    // Middle-click closes (EXP-235).
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.close_tab(ix, window, cx);
                        }),
                    )
                    // Right-click context menu (EXP-235). Hosted in the
                    // titlebar this used to lose to the Linux WM window menu;
                    // the strip's `app_title_bar::interactive` wrapper now
                    // swallows the press that popped it (EXP-294).
                    .context_menu({
                        let panel = panel.clone();
                        move |menu, _window, _cx| {
                            let close = panel.clone();
                            let close_others = panel.clone();
                            let close_all = panel.clone();
                            menu.item(PopupMenuItem::new("Close").on_click(
                                move |_, window, cx| {
                                    let _ = close.update(cx, |this, cx| {
                                        this.close_tab(ix, window, cx);
                                    });
                                },
                            ))
                            .item(
                                PopupMenuItem::new("Close others")
                                    .disabled(tab_count <= 1)
                                    .on_click(move |_, window, cx| {
                                        let _ = close_others.update(cx, |this, cx| {
                                            this.close_other_tabs(ix, window, cx);
                                        });
                                    }),
                            )
                            .item(PopupMenuItem::new("Close all").on_click(
                                move |_, window, cx| {
                                    let _ = close_all.update(cx, |this, cx| {
                                        this.close_all_tabs(window, cx);
                                    });
                                },
                            ))
                        }
                    })
                    // EXP-310: status icon + identifier shortcode ahead of
                    // the title (issue-backed tabs only), mirroring the issue
                    // list row's glyph/mono-identifier treatment.
                    .when_some(content.status, |chip, status| {
                        chip.child(crate::icons::resolved_status_icon(&status, cx).xsmall())
                    })
                    .when_some(content.identifier, |chip, identifier| {
                        chip.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .font_family(theme::terminal::FONT_FAMILY)
                                .whitespace_nowrap()
                                .child(identifier),
                        )
                    })
                    .when_some(content.title, |chip, title| {
                        chip.child(div().max_w(px(180.)).truncate().child(title))
                    })
                    .child(
                        h_flex()
                            .gap_0p5()
                            // Hover-revealed undock (EXP-65): `invisible`
                            // keeps the layout slot so tabs don't jitter.
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
                                    .icon(registry::UI_CLOSE)
                                    .on_click(cx.listener(
                                        move |this, _: &ClickEvent, window, cx| {
                                            cx.stop_propagation();
                                            this.close_tab(ix, window, cx);
                                        },
                                    )),
                            ),
                    )
            }));

        // EXP-288: the hidden tabs collapse into a "+N" dropdown; clicking
        // one activates it (origin re-selection included via activate_tab).
        if !hidden.is_empty() {
            // Keyed by SCREEN, not by index: the menu's closures run at click
            // time, and a tab closed while the dropdown is open (middle-click
            // on a visible chip, a team switch) shifts every index after it,
            // which would activate the wrong tab. Tabs are deduped by screen,
            // so it is a stable identity.
            let hidden_entries: Vec<(
                Screen,
                Option<domain::statuses::ResolvedStatus>,
                gpui::SharedString,
            )> = hidden
                .iter()
                .map(|&ix| {
                    let screen = self.tabs[ix].screen.clone();
                    // EXP-310: the menu rows carry the same status icon +
                    // shortcode as the chips (composed into the label —
                    // menu items are plain icon + text).
                    let content = chip_content(&screen, cx);
                    let label = match (&content.identifier, &content.title) {
                        (Some(identifier), Some(title)) => {
                            gpui::SharedString::from(format!("{identifier} {title}"))
                        }
                        (Some(identifier), None) => identifier.clone(),
                        _ => content
                            .title
                            .unwrap_or_else(|| screen_title(&screen, cx)),
                    };
                    (screen, content.status, label)
                })
                .collect();
            let panel = panel.clone();
            strip = strip.child(
                Button::new("center-tab-overflow")
                    .ghost()
                    .xsmall()
                    .label(format!("+{}", hidden_entries.len()))
                    .tooltip("More tabs")
                    .dropdown_menu(move |mut menu, _window, cx| {
                        menu = menu.scrollable(true).max_h(px(320.));
                        for (screen, status, title) in &hidden_entries {
                            let panel = panel.clone();
                            let screen = screen.clone();
                            let mut item = PopupMenuItem::new(title.clone());
                            if let Some(status) = status {
                                item = item.icon(crate::icons::resolved_status_icon(status, cx));
                            }
                            menu = menu.item(item.on_click(
                                move |_, window, cx| {
                                    let _ = panel.update(cx, |this, cx| {
                                        let Some(ix) = this
                                            .tabs
                                            .iter()
                                            .position(|tab| tab.screen == screen)
                                        else {
                                            return;
                                        };
                                        this.activate_tab(ix, window, cx);
                                    });
                                },
                            ));
                        }
                        menu
                    }),
            );
        }
        strip.into_any_element()
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
    /// team has none, or team creation when the account has none).
    fn render_empty(&self, cx: &mut gpui::Context<Self>) -> gpui::AnyElement {
        // EXP-188 zero-team state (signup no longer auto-creates a personal
        // team, and the last team is deletable): offer create-or-join. Only
        // a READY-and-empty teams shape counts — empty-because-loading must
        // never show this (§4.1), though `shapes_ready` already gates us.
        {
            let teams = Store::global(cx).collections().teams.read(cx);
            if teams.is_ready() && teams.is_empty() {
                return v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .child(
                        Icon::new(registry::UI_TEAM)
                            .size_6()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child("No team yet"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Create a team to get started, or join one with an invite link."),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("screens-create-team")
                                    .primary()
                                    .small()
                                    .label("Create team…")
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(CreateTeam), cx);
                                    }),
                            )
                            .child(
                                Button::new("screens-join-team")
                                    .small()
                                    .label("Join team…")
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(JoinTeam), cx);
                                    }),
                            ),
                    )
                    .into_any_element();
            }
        }
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
                    Icon::new(registry::NAV_INBOX)
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
                Icon::new(registry::NAV_BOARDS)
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
        // No team resolves (e.g. mid team-switch churn): the create
        // action would silently no-op, so don't offer a dead button. (The
        // fully-teamless account is handled by the zero-team branch above.)
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
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
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
                    cx.spawn_in(window, async move |_this, cx| {
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
            Some(Screen::IssueDetail { .. }) => self.issue_detail.clone().into_any_element(),
            Some(Screen::Settings) => self.settings.clone().into_any_element(),
            Some(Screen::Account) => self.account.clone().into_any_element(),
            Some(Screen::SupportThread { .. }) => {
                self.support_thread.clone().into_any_element()
            }
            Some(Screen::PrDiff { .. }) => self.pr_diff.clone().into_any_element(),
            Some(Screen::ActionDetail { .. }) => {
                self.action_detail.clone().into_any_element()
            }
            // EXP-288: no tab selected — the active TOOL owns the center.
            // Source Control shows its diff (following the History
            // selection), Files the read-only viewer (its Idle phase covers
            // "no file selected"); everything else keeps the empty state.
            None => match self.rail.read(cx).tool() {
                ToolWindow::SourceControl => self.source_control.clone().into_any_element(),
                ToolWindow::Files => self.file_viewer.clone().into_any_element(),
                _ if !shapes_ready(cx) => self.render_syncing(cx),
                _ => self.render_empty(cx),
            },
        };

        // EXP-277: the tab strip lives in the titlebar (AppTitleBar) whenever
        // the window paints its own chrome; under Linux server-side
        // decorations the titlebar is hidden, so the strip renders here in
        // its legacy in-panel position (with its own EXP-288 divider — the
        // titlebar carries it otherwise).
        let fallback_strip = (!self.tabs.is_empty()
            && !crate::app_title_bar::client_chrome(window))
        .then(|| {
            // Conservative width budget: the strip shares the row with
            // nothing, but the panel itself sits right of the rail + tool
            // column (both unknown here) — assume the default split.
            let available = (window.viewport_size().width - px(620.)).max(px(160.));
            div()
                .w_full()
                .px_2()
                .pt_1()
                .pb_1()
                .border_b_1()
                .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
                .child(self.render_tab_strip(available, cx))
        });

        div().size_full().bg(cx.theme().colors.list).child(
            v_flex()
                .size_full()
                .children(fallback_strip)
                .child(div().flex_1().min_h_0().child(content)),
        )
    }
}
