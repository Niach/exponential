//! The center panel (masterplan-v3 §4.2, reworked twice — EXP-288): a
//! TAB-BASED editor area whose tabs are DETAIL VIEWS ONLY (issue detail,
//! PR diff, support thread). High-level surfaces get no
//! tabs: Source Control's diff and the file viewer are the center content
//! their rail tool shows (driven by the sidebar's commit/file selection),
//! and Settings and the Actions page (EXP-480) are tab-less full-screen
//! modes. Every tab REMEMBERS
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

/// [`screens_for_window`] by id — for callers that hold a `WindowId` instead
/// of a live `&Window` (the issue header's switcher). `None` in windows
/// without a panel (undocked screens).
pub(crate) fn screens_for_window_id(
    window_id: WindowId,
    cx: &App,
) -> Option<Entity<ScreensPanel>> {
    cx.try_global::<ScreensRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
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
        // Never undockable — unreachable via the undock path, kept total for
        // the compiler.
        Screen::Actions => cx
            .new(|cx| crate::actions_view::ActionsView::new(window, cx))
            .into(),
        Screen::GettingStarted => cx
            .new(|cx| crate::getting_started::GettingStartedView::new(window, cx))
            .into(),
        Screen::Settings => cx.new(|cx| crate::settings::SettingsView::new(window, cx)).into(),
    }
}

/// One open tab: the detail screen it shows plus the sidebar entry it was
/// opened from (EXP-288 — activating the tab re-selects that entry).
#[derive(Clone)]
struct TabEntry {
    screen: Screen,
    origin: TabOrigin,
}

/// Shaped width of a single line, in pixels (EXP-326).
///
/// `WindowTextSystem::layout_line` is the same path the text elements
/// themselves take — including the per-frame layout cache — so measuring a
/// label the strip is about to render is a cache hit, not a second shaping.
/// The run carries no decorations: only the glyph advances matter here.
pub(crate) fn measure_text(window: &Window, text: &str, font: gpui::Font, size: gpui::Rems) -> f32 {
    if text.is_empty() {
        return 0.;
    }
    let run = gpui::TextRun {
        len: text.len(),
        font,
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let layout = window
        .text_system()
        .layout_line(text, size.to_pixels(window.rem_size()), &[run], None);
    f32::from(layout.width)
}

/// Gap between chips in the strip — the `gap_1()` on the strip's `h_flex`,
/// which like every gpui spacing helper resolves against the rem size.
/// Shared with the terminal dock's strip (EXP-497), which uses the same gap.
pub(crate) fn chip_gap(window: &Window) -> f32 {
    0.25 * f32::from(window.rem_size())
}

/// Width of the trailing "+N" button: an xsmall `Button` (`px_1` a side)
/// with a `text_xs` label. `hidden_max` is the largest count the label could
/// carry, so the reserve never comes out short.
pub(crate) fn overflow_button_width(window: &Window, hidden_max: usize) -> f32 {
    let label = format!("+{hidden_max}");
    0.5 * f32::from(window.rem_size())
        + measure_text(window, &label, window.text_style().font(), gpui::rems(0.75))
}

/// EXP-288: which tabs get a chip, in strip order — the rest collapse into
/// the trailing "+N" dropdown. Shared with the terminal dock's tab strip
/// (EXP-497), which partitions its chips the same way.
///
/// Chips are laid out in tab order until the next one would not fit; the
/// overflow button's own width is only reserved once something actually
/// overflows, so a set that fits exactly keeps every chip. The ACTIVE tab is
/// always among the visible ones — its width is committed up front and the
/// rest pack around it (display order only; `self.tabs` keeps its order).
///
/// EXP-326: this used to run on estimates that came out long in three
/// separate places, so the strip collapsed tabs while there was still empty
/// room to its right. `widths`, `gap` and `overflow_w` are measured against
/// the window now (see [`ScreensPanel::measure_chip_width`]) and `available`
/// is computed from the window chrome, so "fits" means fits.
pub(crate) fn partition_tabs(
    widths: &[f32],
    available: f32,
    gap: f32,
    overflow_w: f32,
    active_ix: Option<usize>,
) -> Vec<usize> {
    let count = widths.len();
    let available = available.max(0.);
    let total = widths.iter().sum::<f32>() + gap * count.saturating_sub(1) as f32;
    if total <= available {
        return (0..count).collect();
    }

    let budget = (available - overflow_w - gap).max(0.);
    // EXP-343: the ACTIVE chip's width is committed before any packing — it
    // is always visible, so the rest pack around it in tab order. The old
    // shape of this packed a prefix and then swapped its last chip for the
    // active one WITHOUT re-checking the budget, so a wide active tab
    // overflowed the strip — far enough to shove the Linux window controls
    // off the window edge and clip the "+N" button.
    let mut visible: Vec<usize> = Vec::new();
    let mut used = active_ix.map_or(0., |ix| widths[ix]);
    let mut chips = usize::from(active_ix.is_some());
    for (ix, width) in widths.iter().enumerate() {
        if Some(ix) == active_ix {
            visible.push(ix);
            continue;
        }
        let next = used + width + if chips > 0 { gap } else { 0. };
        if next > budget && chips > 0 {
            break;
        }
        visible.push(ix);
        used = next;
        chips += 1;
    }
    if let Some(active) = active_ix {
        if !visible.contains(&active) {
            visible.push(active);
        }
    }
    if visible.is_empty() && count > 0 {
        visible.push(0);
    }
    visible
}

/// Chip content for one tab (EXP-310): issue-backed tabs (issue detail + PR
/// diff) lead with the colored status icon and the identifier shortcode. A
/// blank issue title drops the title part — the shortcode already labels the
/// chip, where `screen_title`'s identifier fallback would render it twice.
/// Non-issue tabs (and issue rows not yet synced) keep the plain
/// `screen_title`.
struct ChipContent {
    lead: ChipLead,
    identifier: Option<gpui::SharedString>,
    title: Option<gpui::SharedString>,
}

/// A chip's leading glyph (EXP-426). Cloneable — the overflow dropdown
/// collects entries up front and builds `Icon`s (not `Clone` at the pinned
/// gpui-component rev) only at menu-build time.
#[derive(Clone)]
enum ChipLead {
    None,
    /// EXP-314: the issue's RESOLVED status (custom rows included).
    /// Resolution is per-issue, so it stays correct on this cross-team strip
    /// — only GROUPING is team-scoped.
    Status(domain::statuses::ResolvedStatus),
}

impl ChipLead {
    fn icon(&self, cx: &App) -> Option<gpui_component::Icon> {
        match self {
            ChipLead::None => None,
            ChipLead::Status(status) => Some(crate::icons::resolved_status_icon(status, cx)),
        }
    }
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
                lead: ChipLead::Status(resolved),
                identifier: Some(gpui::SharedString::from(issue.identifier.clone())),
                title,
            };
        }
    }
    ChipContent {
        lead: ChipLead::None,
        identifier: None,
        title: Some(screen_title(screen, cx)),
    }
}

pub struct ScreensPanel {
    focus_handle: FocusHandle,
    nav: Entity<Navigation>,
    issue_detail: Entity<IssueDetailView>,
    settings: Entity<crate::settings::SettingsView>,
    source_control: Entity<crate::source_control::SourceControlView>,
    file_viewer: Entity<crate::file_viewer::FileViewerView>,
    /// One shared support-thread view, re-pointed on tab switch (EXP-180 —
    /// same single-instance model as the issue detail).
    support_thread: Entity<crate::support_thread::SupportThreadView>,
    /// One shared PR diff view, re-pointed on tab switch (EXP-181 — the
    /// Reviews rows' target).
    pr_diff: Entity<crate::pr_diff::PrDiffView>,
    /// The Actions page (EXP-467 — the web agents page: machines + the
    /// action card grid; EXP-480: a tab-less full-page mode like Settings).
    actions: Entity<crate::actions_view::ActionsView>,
    /// The Getting-started checklist page (EXP-470 — the same tab-less
    /// full-page mode, behind a conditional rail entry).
    getting_started: Entity<crate::getting_started::GettingStartedView>,
    /// The window's shared rail state (EXP-288): the active tool drives the
    /// tab-less center default (SC diff / file viewer), and the file
    /// selection re-points the viewer.
    rail: Entity<RailShared>,
    /// Open tabs in strip order — detail screens only, deduped by `screen`
    /// (several issues at once; re-opening focuses + refreshes the origin).
    tabs: Vec<TabEntry>,
    /// The team the tabs belong to — a switch drops them.
    tabs_team: Option<String>,
    /// The screen shown at the last nav notify (EXP-369): the panes are
    /// long-lived, so a transition INTO one is the only "opened" signal a
    /// pane that fetches server-only data gets.
    active_screen: Option<Screen>,
    /// EXP-492/EXP-499: the panel's painted slot width, recorded each
    /// prepaint (the editor's EXP-421/436 recipe). The center content gets
    /// this as a DEFINITE pixel width, because between real layout frames
    /// gpui runs passes that resolve the panel subtree at fit-content —
    /// percent chains collapse (the Actions page's machines rows shrink-wrap
    /// and its card wrap-grid stops wrapping, running off the right edge).
    /// One frame stale during a live resize; 0.0 before the first paint
    /// falls back to stretch.
    slot_width: std::rc::Rc<std::cell::Cell<f32>>,
    _subscriptions: Vec<Subscription>,
}

impl ScreensPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        // Full-page issue detail (§4.2): one instance, re-pointed on
        // navigation (its local edit state resets per issue, web parity).
        let issue_detail = cx.new(|cx| IssueDetailView::new(window, cx));
        let settings = cx.new(|cx| crate::settings::SettingsView::new(window, cx));
        let source_control = cx.new(|cx| crate::source_control::SourceControlView::new(window, cx));
        let file_viewer = cx.new(|cx| crate::file_viewer::FileViewerView::new(window, cx));
        let support_thread =
            cx.new(|cx| crate::support_thread::SupportThreadView::new(window, cx));
        let pr_diff = cx.new(|cx| crate::pr_diff::PrDiffView::new(window, cx));
        let actions = cx.new(|cx| crate::actions_view::ActionsView::new(window, cx));
        let getting_started =
            cx.new(|cx| crate::getting_started::GettingStartedView::new(window, cx));
        let nav = nav_for_window(window, cx);
        let rail = rail_shared_for_window(window, cx);

        let mut subscriptions = Vec::new();
        // Navigation changes open/focus tabs and retarget the shared views
        // (needs `window` for the detail's input resets, hence `observe_in`).
        subscriptions.push(cx.observe_in(&nav, window, |this, _, window, cx| {
            this.sync_tabs(window, cx);
            this.sync_active_screen(cx);
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
        // Tab titles join issue identifiers live; a deleted issue's tabs
        // close instead of lingering as "not found" (EXP-493).
        subscriptions.push(cx.observe_in(
            &collections.issues,
            window,
            |this, _, window, cx| {
                this.prune_missing_issue_tabs(window, cx);
                cx.notify();
            },
        ));
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
            source_control,
            file_viewer,
            support_thread,
            pr_diff,
            actions,
            getting_started,
            rail,
            tabs: Vec::new(),
            tabs_team: None,
            active_screen: None,
            slot_width: std::rc::Rc::new(std::cell::Cell::new(0.0)),
            _subscriptions: subscriptions,
        };
        this.sync_tabs(window, cx);
        this.sync_active_screen(cx);
        this.sync_file_viewer(cx);
        this
    }

    /// EXP-369 (re-homed by EXP-238): the settings Personal panes hold
    /// server-only reads (email prefs, timezone, API keys) that would show
    /// the first visit's snapshot forever. Every transition INTO the
    /// settings screen marks them stale; each pane refetches on its next
    /// render.
    fn sync_active_screen(&mut self, cx: &mut gpui::Context<Self>) {
        let screen = resolved_screen(&self.nav, cx);
        if screen == self.active_screen {
            return;
        }
        let entered_settings = matches!(screen, Some(Screen::Settings));
        self.active_screen = screen;
        if entered_settings {
            self.settings
                .update(cx, |settings, cx| settings.mark_personal_stale(cx));
        }
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
            // Dropping the tabs tears the issue detail down without a blur —
            // flush a pending description edit first (EXP-68).
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
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
        // EXP-288: only detail views are tabs — Settings renders
        // tab-less (no chip, nothing highlighted).
        if !screen.is_detail() {
            return;
        }
        // Resolve the origin: an explicit one wins; Capture reads the rail
        // tool + active board at consume time (the row click that navigated
        // ran with its tool already active). `None` (go-back / tab
        // reactivation of a closed tab) falls back to Capture too.
        let captured = {
            let rail = self.rail.read(cx);
            let tool = rail.tool();
            TabOrigin {
                board_id: (tool == ToolWindow::BoardIssues)
                    .then(|| active_board_id(&self.nav, cx))
                    .flatten(),
                inbox_tab: (tool == ToolWindow::Inbox).then(|| rail.inbox_tab()),
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
            Screen::Actions | Screen::GettingStarted | Screen::Settings => {
                unreachable!("filtered by is_detail")
            }
        }
        // A back-navigation can re-open a tab for an issue deleted while its
        // screen sat on the stack — prune it right away (EXP-493).
        self.prune_missing_issue_tabs(window, cx);
    }

    /// EXP-493: close the tabs of issues that no longer exist — the user
    /// deleting the open issue, a teammate deleting it mid-view, a board
    /// trashing, or a lost membership all remove the row from the synced
    /// collection, and the tab would otherwise linger as a generic "Issue"
    /// chip over "Issue not found in this team". Only a READY collection may
    /// close anything (§4.1 — absence in an unsynced snapshot is "still
    /// syncing", never "deleted"); this mirrors the detail view's own
    /// not-found condition. Covers issue detail AND PR diff tabs.
    fn prune_missing_issue_tabs(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let missing: Vec<usize> = {
            let issues = Store::global(cx).collections().issues.read(cx);
            if !issues.is_ready() {
                return;
            }
            self.tabs
                .iter()
                .enumerate()
                .filter_map(|(ix, tab)| match &tab.screen {
                    Screen::IssueDetail { issue_id } | Screen::PrDiff { issue_id } => {
                        issues.get(issue_id).is_none().then_some(ix)
                    }
                    _ => None,
                })
                .collect()
        };
        // Highest index first — `close_tab` removes by index, and closing an
        // active tab re-activates a neighbor safely mid-loop.
        for ix in missing.into_iter().rev() {
            self.close_tab(ix, window, cx);
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
        if entry.origin.tool == ToolWindow::Inbox {
            if let Some(tab) = entry.origin.inbox_tab {
                // EXP-426: restore the Inbox tab the detail came from (the
                // tab-only setter — `activate_tool` would close this tab).
                crate::sidebar::select_inbox_tab_for_tab(window, cx, tab);
            }
        }
        set_screen(window, cx, Some(entry.screen));
    }

    /// The ACTIVE tab's remembered origin (EXP-426): the issue header's
    /// prev/next switcher steps the list the tab was opened from, not
    /// whatever the rail happens to show now. `None` when the active screen
    /// has no tab (or no screen is active).
    pub(crate) fn active_tab_origin(&self, cx: &App) -> Option<TabOrigin> {
        let active = resolved_screen(&self.nav, cx)?;
        self.tabs
            .iter()
            .find(|tab| tab.screen == active)
            .map(|tab| tab.origin.clone())
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
        if matches!(self.tabs[ix].screen, Screen::IssueDetail { .. }) {
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
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
        // pending description edit.
        if self
            .tabs
            .iter()
            .any(|tab| tab.screen != keep && matches!(tab.screen, Screen::IssueDetail { .. }))
        {
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
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

    /// Chip width for the overflow computation.
    ///
    /// EXP-326: both halves of this used to be guesses that ran long, which
    /// is why the strip collapsed tabs into "+N" with visible room still to
    /// its right. The labels are SHAPED with the window's own text system
    /// now, and the chrome is expressed in the units gpui actually lays it
    /// out in: every spacing helper (`px_2`, `gap_1`, `size_3`, `size_5`,
    /// `gap_0p5`) resolves against the REM size, and this app runs a 13px rem
    /// — reading them as their 16px-rem pixel values inflated every chip by
    /// ~23%. The one genuine pixel constant is the title's `max_w`.
    fn measure_chip_width(&self, entry: &TabEntry, window: &Window, cx: &App) -> f32 {
        /// `tab_chip`'s `px_2`, both sides.
        const CHIP_PADDING_REMS: f32 = 0.5 * 2.;
        /// `Icon::xsmall()` — `size_3` (status AND action leads render
        /// xsmall, so one constant covers both — EXP-426).
        const LEAD_ICON_REMS: f32 = 0.75;
        /// An icon-only xsmall `Button` — `size_5`.
        const XSMALL_BUTTON_REMS: f32 = 1.25;
        /// The trailing button cluster's own `gap_0p5`.
        const CLUSTER_GAP_REMS: f32 = 0.125;
        /// `.max_w(px(180.)).truncate()` on the title child — a real pixel
        /// value, so it does NOT scale with the rem.
        const TITLE_MAX_W: f32 = 180.;

        let rem = f32::from(window.rem_size());
        let content = chip_content(&entry.screen, cx);
        let base_font = window.text_style().font();
        let mut children: Vec<f32> = Vec::with_capacity(4);
        if !matches!(content.lead, ChipLead::None) {
            children.push(LEAD_ICON_REMS * rem);
        }
        if let Some(identifier) = content.identifier.as_ref() {
            // EXP-310: the shortcode renders `text_xs` in the terminal mono
            // family, not the bar's proportional font.
            let mut font = base_font.clone();
            font.family = theme::terminal::FONT_FAMILY.into();
            children.push(measure_text(window, identifier, font, gpui::rems(0.75)));
        }
        if let Some(title) = content.title.as_ref() {
            let width = measure_text(window, title, base_font, gpui::rems(0.875));
            children.push(width.min(TITLE_MAX_W));
        }
        // The undock slot is `invisible`, not absent, so it keeps its box.
        children.push(if entry.screen.undockable() {
            (XSMALL_BUTTON_REMS * 2. + CLUSTER_GAP_REMS) * rem
        } else {
            XSMALL_BUTTON_REMS * rem
        });

        let gaps = chip_gap(window) * children.len().saturating_sub(1) as f32;
        CHIP_PADDING_REMS * rem + gaps + children.into_iter().sum::<f32>()
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
    /// chip). `available` is the caller's width for the strip, with
    /// `max_w_full` as the safety net.
    pub(crate) fn render_tab_strip(
        &mut self,
        available: gpui::Pixels,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        if self.tabs.is_empty() {
            return gpui::Empty.into_any_element();
        }
        let active_ix = resolved_screen(&self.nav, cx)
            .and_then(|screen| self.tabs.iter().position(|tab| tab.screen == screen));
        let panel = cx.entity().downgrade();
        let tab_count = self.tabs.len();

        let widths: Vec<f32> = self
            .tabs
            .iter()
            .map(|entry| self.measure_chip_width(entry, window, cx))
            .collect();
        let visible = partition_tabs(
            &widths,
            f32::from(available),
            chip_gap(window),
            overflow_button_width(window, tab_count.saturating_sub(1)),
            active_ix,
        );
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
                    // EXP-310: lead glyph (status icon / action glyph) +
                    // identifier shortcode ahead of the title, mirroring the
                    // issue list row's glyph/mono-identifier treatment.
                    .when_some(content.lead.icon(cx), |chip, icon| {
                        chip.child(icon.xsmall())
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
            let hidden_entries: Vec<(Screen, ChipLead, gpui::SharedString)> = hidden
                .iter()
                .map(|&ix| {
                    let screen = self.tabs[ix].screen.clone();
                    // EXP-310: the menu rows carry the same lead glyph +
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
                    (screen, content.lead, label)
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
                        for (screen, lead, title) in &hidden_entries {
                            let panel = panel.clone();
                            let screen = screen.clone();
                            let mut item = PopupMenuItem::new(title.clone());
                            if let Some(icon) = lead.icon(cx) {
                                item = item.icon(icon);
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
        // EXP-470: a just-created/joined team exists only as an optimistic
        // seed until the Electric echo confirms it — its real boards (join)
        // haven't synced yet, so "No boards yet" would be a wrong empty
        // state. Show a "setting up" surface instead.
        if let Some(team_id) = active_team.as_deref() {
            let teams = Store::global(cx).collections().teams.read(cx);
            if teams.is_seeded(team_id) {
                let name = teams
                    .get(team_id)
                    .map(|team| team.name.clone())
                    .unwrap_or_else(|| "your team".to_string());
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
                            .child(gpui::SharedString::from(format!("Setting up {name}…"))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Syncing the team's data."),
                    )
                    .into_any_element();
            }
        }
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
                        .child("Pick an issue from the sidebar. It opens as a tab here."),
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
            Some(Screen::SupportThread { .. }) => {
                self.support_thread.clone().into_any_element()
            }
            Some(Screen::PrDiff { .. }) => self.pr_diff.clone().into_any_element(),
            Some(Screen::Actions) => self.actions.clone().into_any_element(),
            Some(Screen::GettingStarted) => self.getting_started.clone().into_any_element(),
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
                .child(self.render_tab_strip(available, window, cx))
        });

        pinned_panel_root(
            cx.theme().colors.list,
            fallback_strip,
            self.slot_width.clone(),
            content,
        )
    }
}

/// EXP-492/EXP-499: the panel root — background, the optional fallback tab
/// strip, then the center content pinned to the panel's recorded painted
/// width. Between real layout frames, gpui resolves this subtree under
/// fit-content constraints (diagnosed in EXP-492 with a per-frame bounds
/// logger: whole-column collapses to ~173px); a percent-width (`w_full`)
/// chain below then latches its collapsed result — on the Actions page the
/// machines rows shrink-wrapped and the card wrap-grid resolved its
/// unwrapped max-content line, running off the right edge (EXP-499). A
/// DEFINITE pixel width on the content slot makes those passes resolve
/// every percent chain below correctly. One frame stale during a live
/// resize, like the editor's own EXP-421/436 slot recording; before the
/// first paint (0.0) the content stretches as before.
fn pinned_panel_root(
    background: gpui::Hsla,
    fallback_strip: Option<gpui::Div>,
    slot_width: std::rc::Rc<std::cell::Cell<f32>>,
    content: gpui::AnyElement,
) -> gpui::Div {
    let recorded = slot_width.get();
    div()
        .size_full()
        .bg(background)
        .on_children_prepainted(move |bounds, _, _| {
            if let Some(first) = bounds.first() {
                slot_width.set(f32::from(first.size.width));
            }
        })
        .child(
            v_flex()
                .size_full()
                .children(fallback_strip)
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .when(recorded > 1.0, |this| this.w(px(recorded)))
                        .child(content),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::partition_tabs;

    /// The strip's `gap_1` and the "+N" button at the app's 13px rem.
    const GAP: f32 = 3.25;
    const OVERFLOW: f32 = 22.;

    fn partition(widths: &[f32], available: f32, active: Option<usize>) -> Vec<usize> {
        partition_tabs(widths, available, GAP, OVERFLOW, active)
    }

    /// Widths that add up to exactly the available space keep every chip —
    /// the overflow button's width is only spent once something overflows.
    #[test]
    fn an_exact_fit_keeps_every_tab() {
        let widths = [100., 100., 100.];
        assert_eq!(partition(&widths, 300. + GAP * 2., Some(0)), vec![0, 1, 2]);
    }

    /// EXP-326: the strip runs to the right edge — a tab is only dropped when
    /// it genuinely does not fit, not one chip early.
    #[test]
    fn the_last_fitting_tab_is_kept() {
        let widths = [100., 100., 100.];
        // Room for two chips plus the "+N" button, one pixel short of three.
        let available = 200. + GAP + OVERFLOW + GAP;
        assert_eq!(partition(&widths, available, Some(0)), vec![0, 1]);
        // One pixel less and only the first chip survives the budget.
        assert_eq!(partition(&widths, available - 1., Some(0)), vec![0]);
    }

    /// The active tab is never hidden: it displaces the last chip that fit.
    #[test]
    fn the_active_tab_displaces_the_last_visible_chip() {
        let widths = [100., 100., 100.];
        let available = 200. + GAP + OVERFLOW + GAP;
        assert_eq!(partition(&widths, available, Some(2)), vec![0, 2]);
    }

    /// EXP-343: a WIDER active chip shrinks the visible prefix instead of
    /// overflowing. The old displacement swapped the active chip in for the
    /// last fitting one without re-checking the budget, so the strip ran
    /// past its width — on Linux far enough to push the window controls off
    /// the window edge and clip the "+N" button.
    #[test]
    fn a_wide_active_tab_shrinks_the_prefix_instead_of_overflowing() {
        let widths = [100., 100., 250.];
        // Room for the two 100s plus "+N" — but not for 250 + 100.
        let available = 200. + GAP + OVERFLOW + GAP;
        assert_eq!(partition(&widths, available, Some(2)), vec![2]);
        // With room for 250 + 100 + "+N", the first chip stays.
        assert_eq!(partition(&widths, available + 150., Some(2)), vec![0, 2]);
    }

    /// A single chip wider than the whole strip still renders (clipped by
    /// `max_w_full`) — collapsing everything into "+N" would leave the strip
    /// showing nothing at all.
    #[test]
    fn one_tab_always_survives() {
        assert_eq!(partition(&[500.], 40., Some(0)), vec![0]);
        assert_eq!(partition(&[500., 500.], 0., None), vec![0]);
    }

    #[test]
    fn no_tabs_partition_to_nothing() {
        assert!(partition(&[], 400., None).is_empty());
    }

    /// EXP-499/EXP-508 regression (the EXP-492 fit-content collapse, on the
    /// Actions page): an Actions-shaped center — full-width section band,
    /// machine rows, a wrapping card grid, all with real text inside the
    /// page's capped `mx_auto` column and scroll pane — must resolve its
    /// column to exactly `min(1024, panel)` at EVERY panel width, the
    /// machines section and its band must span that column, and the grid
    /// must never run past the window's right edge. Two gates: a width sweep
    /// under the production `h_resizable` split (settled frames stay
    /// healthy), then the stray fit-content pass modeled directly — the one
    /// place the un-pinned tree demonstrably breaks (clean `cx.draw` frames
    /// alone never reproduce the live app's between-frame passes).
    ///
    /// The sweep runs to 3000px because the EXP-508 failure only starts at
    /// ~1940px (the EXP-499 sweep stopped at 1700 and missed it): a `w_full`
    /// PERCENT child of the centered column resolves against the UNCLAMPED
    /// ancestor available width, so once the panel out-widens the grid's
    /// unwrapped line the wrap grid stops wrapping (and the machines section
    /// shrink-wraps) — the EXP-436 block-hop leak. The probe mirrors the
    /// fixed page: the column's direct children carry NO `w_full` and are
    /// sized by flex-col stretch; percent widths below those stretch-sized
    /// parents (the band, the machine rows) resolve fine and stay covered.
    #[gpui::test]
    async fn actions_shaped_center_spans_the_panel_at_every_width(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::{
            div, point, px, size, AppContext as _, InteractiveElement as _, IntoElement,
            ParentElement as _, Render, ScrollHandle, SharedString, Styled as _, Window,
        };
        use gpui_component::{h_flex, v_flex};

        use super::pinned_panel_root;

        struct PanelProbe {
            scroll: ScrollHandle,
            slot_width: std::rc::Rc<std::cell::Cell<f32>>,
        }
        impl Render for PanelProbe {
            fn render(
                &mut self,
                _window: &mut Window,
                _cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                // The Actions page's real shapes (actions_view.rs /
                // machines.rs), with production-like text.
                let card = |name: &'static str, blurb: &'static str| {
                    v_flex()
                        .flex_basis(px(260.))
                        .flex_grow(1.)
                        .min_w(px(240.))
                        .max_w(px(420.))
                        .gap_2()
                        .p_3()
                        .child(div().text_sm().child(SharedString::from(name)))
                        .child(
                            div()
                                .w_full()
                                .min_w_0()
                                .text_xs()
                                .line_clamp(2)
                                .child(SharedString::from(blurb)),
                        )
                };
                let machine_row = |label: &'static str| {
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_2()
                        .px_3()
                        .py_2()
                        .child(
                            div()
                                .text_sm()
                                .whitespace_nowrap()
                                .child(SharedString::from(label)),
                        )
                        .child(h_flex().flex_1())
                        .child(div().text_xs().child(SharedString::from("Online")))
                };
                let band = h_flex()
                    .w_full()
                    .min_w_0()
                    .debug_selector(|| "probe-band".into())
                    .items_center()
                    .gap_1p5()
                    .px_3()
                    .py_1p5()
                    .child(div().text_sm().child(SharedString::from("My machines")))
                    .child(div().flex_1())
                    .child(div().text_xs().child(SharedString::from("Add server")));
                let grid = h_flex()
                    .min_w_0()
                    .flex_wrap()
                    .items_stretch()
                    .gap_3()
                    .debug_selector(|| "probe-grid".into())
                    .child(card(
                        "Fix merge conflicts",
                        "Pick a conflicted pull request and let your agent rebase, \
                         resolve, and merge it",
                    ))
                    .child(card(
                        "Triage code review findings",
                        "Re-check the latest code review board against today's code, \
                         cancel stale findings, and file one grouped issue",
                    ))
                    .child(card(
                        "Release staging",
                        "Copy the prod DB over staging, refresh staging web + steer \
                         relay, and push the staging iOS/Android build",
                    ))
                    .child(card(
                        "Release prod",
                        "Review the unreleased commit wave, then ship production — \
                         prod web, relays, marketing, desktop",
                    ))
                    .child(card(
                        "Release all",
                        "Run the full release train in one go: the staging refresh \
                         first, then the prod train — one combined wave, one report",
                    ))
                    .child(card(
                        "Extended code review",
                        "Run a deep multi-pass code review over the whole codebase \
                         and file every confirmed finding as an issue",
                    ));
                let column = v_flex()
                    .w_full()
                    .min_w_0()
                    .px_4()
                    .py_4()
                    .gap_4()
                    .debug_selector(|| "probe-column".into())
                    .child(
                        v_flex()
                            .min_w_0()
                            .debug_selector(|| "probe-machines".into())
                            .child(band)
                            .child(machine_row("mint · Danny Strähhuber  v0.14.8"))
                            .child(machine_row("macbook · Danny Strähhuber  v0.14.8")),
                    )
                    .child(v_flex().min_w_0().gap_2().child(grid));
                let content = v_flex()
                    .size_full()
                    .min_h_0()
                    .min_w_0()
                    .child(crate::scroll_pane::v_scroll_pane(
                        "probe-scroll",
                        &self.scroll,
                        div()
                            .w_full()
                            .min_w_0()
                            .child(column.max_w(px(1024.)).mx_auto()),
                    ))
                    .into_any_element();
                div()
                    .size_full()
                    .debug_selector(|| "probe-slot".into())
                    .child(pinned_panel_root(
                        gpui::hsla(0., 0., 0., 1.),
                        None,
                        self.slot_width.clone(),
                        content,
                    ))
            }
        }

        // The production nesting (shell.rs `CenterPanel`): an `h_resizable`
        // split whose right panel holds the screens panel. The resizable
        // panels are flex items with `flex_basis` fed BACK from prepaint
        // bounds via `ResizableState` — at widths where the bases mismatch
        // the container, taffy's flex resolution measures the panel CONTENT
        // under fit-content constraints (the EXP-492 diagnosis).
        struct Split {
            probe: gpui::Entity<PanelProbe>,
            state: gpui::Entity<gpui_component::resizable::ResizableState>,
        }
        impl Render for Split {
            fn render(
                &mut self,
                _window: &mut Window,
                _cx: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                use gpui_component::resizable::{h_resizable, resizable_panel};
                div().size_full().child(
                    h_resizable("center-split")
                        .with_state(&self.state)
                        .child(
                            resizable_panel()
                                .size(px(crate::sidebar::DEFAULT_DOCK_WIDTH))
                                .size_range(px(crate::sidebar::MIN_DOCK_WIDTH)..px(880.))
                                .child(div().size_full()),
                        )
                        .child(resizable_panel().child(self.probe.clone())),
                )
            }
        }

        let cx = cx.add_empty_window();
        // The resizable components read the gpui-component Theme global.
        cx.update(|_, cx| gpui_component::init(cx));
        let split = cx.update(|_, cx| {
            let probe = cx.new(|_| PanelProbe {
                scroll: ScrollHandle::new(),
                slot_width: std::rc::Rc::new(std::cell::Cell::new(0.0)),
            });
            let state = cx.new(|_| gpui_component::resizable::ResizableState::default());
            cx.new(|_| Split { probe, state })
        });
        let mut failures: Vec<(f32, f32, f32)> = Vec::new();
        // Every integer window width — the collapse bands are a few px wide,
        // so a coarse step would miss them. Each width draws TWICE: the
        // resizable state and the recorded slot width are both fed from
        // prepaint bounds into the NEXT frame, so the second frame is the
        // settled one users actually see.
        for width in (700..=3000).map(|w| w as f32) {
            for _ in 0..2 {
                cx.draw(point(px(0.), px(0.)), size(px(width), px(600.)), |_, _| {
                    div().size_full().child(split.clone())
                });
            }
            let panel = cx
                .debug_bounds("probe-slot")
                .unwrap_or_else(|| panic!("slot bounds missing at width {width}"));
            let panel_width = f32::from(panel.size.width);
            assert!(
                panel_width > 100.0,
                "panel itself collapsed at width {width}: {panel_width}"
            );
            let column = cx
                .debug_bounds("probe-column")
                .unwrap_or_else(|| panic!("column bounds missing at width {width}"));
            let expected = panel_width.min(1024.);
            let actual = f32::from(column.size.width);
            if (actual - expected).abs() > 1.5 {
                failures.push((width, actual, expected));
            }
            // The literal EXP-499 symptom: the card grid running off the
            // window's right edge.
            let grid = cx
                .debug_bounds("probe-grid")
                .unwrap_or_else(|| panic!("grid bounds missing at width {width}"));
            let right = f32::from(grid.origin.x) + f32::from(grid.size.width);
            if right > width + 1.5 {
                failures.push((width, right, width));
            }
            // The screenshot's second symptom: the machines section (and its
            // band, a `w_full` percent child of the stretch-sized section)
            // shrink-wrapping instead of spanning the column.
            let content = expected - 32.;
            for selector in ["probe-machines", "probe-band"] {
                let bounds = cx
                    .debug_bounds(selector)
                    .unwrap_or_else(|| panic!("{selector} bounds missing at width {width}"));
                let actual = f32::from(bounds.size.width);
                if (actual - content).abs() > 1.5 {
                    failures.push((width, actual, content));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "actions column broke at {} widths (width, actual, expected): {:?}",
            failures.len(),
            &failures[..failures.len().min(20)]
        );

        // The EXP-492 stray pass itself, modeled directly: between real
        // layout frames gpui can resolve the panel subtree under FIT-CONTENT
        // constraints (a flex parent that neither stretches nor sizes its
        // child), and the app can idle on that frame — the EXP-499
        // screenshot. Un-pinned, the percent chains collapse: the machines
        // band shrink-wraps and the card grid resolves its unwrapped
        // max-content line. The recorded-width pin must make even this pass
        // resolve the page like the real panel slot.
        let recorded = f32::from(
            cx.debug_bounds("probe-slot")
                .expect("slot bounds after the sweep")
                .size
                .width,
        );
        let probe = split.read_with(cx, |split, _| split.probe.clone());
        cx.draw(point(px(0.), px(0.)), size(px(1700.), px(600.)), |_, _| {
            div()
                .size_full()
                .flex()
                .flex_row()
                .items_start()
                .child(probe.clone())
        });
        let column = cx
            .debug_bounds("probe-column")
            .expect("column bounds in the fit-content pass");
        let expected = recorded.min(1024.);
        let actual = f32::from(column.size.width);
        assert!(
            (actual - expected).abs() <= 1.5,
            "fit-content pass collapsed the column: {actual} != {expected} \
             (recorded panel {recorded})"
        );
        let grid = cx
            .debug_bounds("probe-grid")
            .expect("grid bounds in the fit-content pass");
        let grid_right = f32::from(grid.origin.x) + f32::from(grid.size.width);
        assert!(
            grid_right <= recorded + 1.5,
            "fit-content pass ran the card grid past the panel edge: \
             {grid_right} > {recorded}"
        );
        for selector in ["probe-machines", "probe-band"] {
            let bounds = cx
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("{selector} bounds missing in the fit-content pass"));
            let actual = f32::from(bounds.size.width);
            let content = expected - 32.;
            assert!(
                (actual - content).abs() <= 1.5,
                "fit-content pass shrink-wrapped {selector}: {actual} != {content}"
            );
        }
    }
}
