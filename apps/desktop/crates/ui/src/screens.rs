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
use crate::controls::WebControl as _;
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
/// of a live `&Window` (the session bar's repaint poke). `None` in windows
/// without a panel (undocked screens).
pub(crate) fn screens_for_window_id(
    window_id: WindowId,
    cx: &App,
) -> Option<Entity<ScreensPanel>> {
    cx.try_global::<ScreensRegistry>()
        .and_then(|registry| registry.by_window.get(&window_id).cloned())
}

/// EXP-746: every window's open screen for `session_id` (usually zero or
/// one). The engine's exit edge is app-global — it knows the row, not the
/// window that opened it — so it marks whatever is up.
pub(crate) fn session_views(
    session_id: &str,
    cx: &App,
) -> Vec<Entity<crate::session_screen::SessionScreenView>> {
    let Some(registry) = cx.try_global::<ScreensRegistry>() else {
        return Vec::new();
    };
    registry
        .by_window
        .values()
        .flat_map(|panel| {
            let panel = panel.read(cx);
            panel.session_view(session_id)
        })
        .collect()
}

/// EXP-818: put the rail back the way `screen`'s tab remembers it — the
/// go-back / go-forward twin of [`ScreensPanel::activate_tab`]'s origin
/// replay. A detail restores its tab's origin; the Chat page selects the
/// Sessions tool (it is the Agent page's center); a rail full page and
/// Settings have no list column to restore.
pub(crate) fn restore_origin_for_screen(window: &Window, cx: &mut App, screen: &Screen) {
    let origin = match screen {
        Screen::Chat => Some(TabOrigin {
            tool: ToolWindow::Sessions,
            board_id: None,
            inbox_tab: None,
        }),
        screen if screen.is_detail() => screens_for_window(window, cx)
            .and_then(|panel| panel.read(cx).origin_of(screen)),
        _ => None,
    };
    if let Some(origin) = origin {
        crate::sidebar::apply_origin(window, cx, &origin);
    }
}

/// EXP-746: hand the open tab of `resumed_from` to `session_id` in THIS
/// window's panel (D5's resume swap), reporting whether a tab changed hands.
///
/// The open-time half of the rule `sync_session_tabs` applies to synced rows:
/// a local resume opens its screen before the new row's echo can arrive, so
/// without this the run it continues would keep a second tab beside it. Only
/// this window's panel is consulted — the tab being taken over is the one the
/// user is looking at; a copy undocked into another window is that window's
/// own sync to swap.
pub(crate) fn take_over_session_tab(
    resumed_from: &str,
    session_id: &str,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let Some(panel) = screens_for_window(window, cx) else {
        return false;
    };
    panel.update(cx, |panel, cx| {
        panel.take_over_session_tab(resumed_from, session_id, window, cx)
    })
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
        // EXP-746: a fresh view is always the REMOTE one — a second view over
        // a live local run would take a second handle on one engine. Never
        // undockable today, so this only keeps the match total.
        Screen::Session { session_id } => cx
            .new(|cx| {
                crate::session_screen::SessionScreenView::remote(session_id.clone(), window, cx)
            })
            .into(),
        // EXP-769: a terminal pops out through `undock::open_undocked_terminal_tab`
        // (the manager keeps the tab; the window renders the same view) —
        // never through here. Kept total for the compiler.
        Screen::Terminal { .. } => cx.new(|_| NeverUndocked).into(),
        // Never undockable — unreachable via the undock path, kept total for
        // the compiler.
        Screen::Devices => cx
            .new(|cx| crate::devices_view::DevicesView::new(window, cx))
            .into(),
        Screen::Actions => cx
            .new(|cx| crate::actions_view::ActionsView::new(window, cx))
            .into(),
        Screen::Automations => cx
            .new(|cx| crate::automations_view::AutomationsView::new(window, cx))
            .into(),
        Screen::Chat => cx
            .new(|cx| crate::chat_screen::ChatScreenView::new(window, cx))
            .into(),
        Screen::Reviews => cx
            .new(|cx| crate::reviews_view::ReviewsView::new(window, cx))
            .into(),
        Screen::GettingStarted { .. } => cx
            .new(|cx| crate::getting_started::GettingStartedView::new(window, cx))
            .into(),
        Screen::Settings => cx.new(|cx| crate::settings::SettingsView::new(window, cx)).into(),
    }
}

/// The stand-in view for a screen kind that is never undocked
/// (`Screen::undockable` is false for it) — `build_screen_content` stays a
/// total match without inventing a real view for a path nothing takes.
struct NeverUndocked;

impl Render for NeverUndocked {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
    }
}

/// One open tab: the detail screen it shows plus the sidebar entry it was
/// opened from (EXP-288 — activating the tab re-selects that entry).
#[derive(Clone)]
struct TabEntry {
    screen: Screen,
    origin: TabOrigin,
}

/// EXP-769/EXP-791: one entry of the bottom session bar, in bar order — an
/// open [`Screen::Terminal`] tab (`ix` into `ScreensPanel::tabs`). The bar
/// used to be the web `AgentDock`'s tab list (sessions AND terminals, plus
/// the caller's tab-less live runs); EXP-791 moved every session to the
/// rail's Sessions section, so the bar is the terminal strip and nothing
/// else — and takes no height at all without one.
struct DockEntry {
    ix: usize,
    screen: Screen,
}

/// EXP-769: what a session-bar chip's × does (web `DockTab` semantics).
#[derive(Clone)]
enum DockClose {
    /// A terminal: close it (the child is killed) — the retired dock's cmd-w.
    CloseTerminal(terminal::TabId),
    /// A live run of the caller's: kill it, after the confirm.
    Kill(crate::session_bar::KillTarget),
    /// An ended run's transcript tab: just close the tab.
    CloseTab(Screen),
}

impl DockClose {
    fn label(&self) -> &'static str {
        match self {
            DockClose::CloseTerminal(_) => "Close terminal",
            DockClose::Kill(_) => "Kill session",
            DockClose::CloseTab(_) => "Close",
        }
    }
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

/// Gap BETWEEN chips in the strip — the `gap_1()` on the strip's `h_flex`,
/// which like every gpui spacing helper resolves against the rem size.
/// Shared with the terminal dock's strip (EXP-497), which uses the same gap.
pub(crate) fn chip_gap(window: &Window) -> f32 {
    0.25 * f32::from(window.rem_size())
}

/// Gap between a chip's own CHILDREN — `surface::rich_tab`'s `gap_1p5`.
/// EXP-698: this used to be measured with [`chip_gap`], which is the gap
/// between chips, not inside one; the two are different helpers on different
/// elements (`gap_1` on the strip, `gap_1p5` on the chip) and reading one for
/// the other under-measured every chip by a third of a gap per child.
pub(crate) fn rich_tab_child_gap(window: &Window) -> f32 {
    0.375 * f32::from(window.rem_size())
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
///
/// EXP-769: the session bar's chips are built from the same struct — a
/// session chip adds the web `RichTab`'s trailing ` · machine` caption and a
/// terminal chip its exit-code badge.
struct ChipContent {
    lead: ChipLead,
    identifier: Option<gpui::SharedString>,
    title: Option<gpui::SharedString>,
    /// EXP-769: a trailing muted caption (` · machine`) — session chips only.
    caption: Option<gpui::SharedString>,
    /// EXP-769: a tinted exit-code badge — terminal chips whose child exited.
    badge: Option<(gpui::SharedString, gpui::Hsla)>,
    /// EXP-769: a paused host's chip dims whole (EXP-696, web `paused`).
    paused: bool,
}

impl ChipContent {
    fn plain(title: gpui::SharedString) -> Self {
        Self {
            lead: ChipLead::None,
            identifier: None,
            title: Some(title),
            caption: None,
            badge: None,
            paused: false,
        }
    }
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
    /// EXP-746: a liveness tone dot — the session chip (web `tabStatus`).
    Dot(gpui::Hsla),
    /// EXP-769: the `session-shell` glyph — a plain terminal chip (EXP-723: a
    /// chip carrying only a title read as a nameless tab next to the issue
    /// chips' status glyphs).
    Shell,
}

impl ChipLead {
    /// The GLYPH lead, when the chip has one. A [`ChipLead::Dot`] is painted
    /// by `surface::rich_tab` itself and has no icon.
    fn icon(&self, cx: &App) -> Option<gpui_component::Icon> {
        match self {
            ChipLead::None | ChipLead::Dot(_) => None,
            ChipLead::Status(status) => Some(crate::icons::resolved_status_icon(status, cx)),
            ChipLead::Shell => Some(Icon::new(registry::SESSION_SHELL)),
        }
    }
}

/// How much width the lead reserves in [`ScreensPanel::measure_chip_width`].
///
/// EXP-746: reserving the icon's `size_3` for a `size_1p5` dot over-estimated
/// every session chip and collapsed tabs into "+N" with room to spare — the
/// exact EXP-326 bug the measured strip exists to avoid. Pure (unit-tested);
/// it must move whenever `surface::rich_tab`'s lead does.
fn lead_reserve_rems(lead: &ChipLead) -> f32 {
    /// `Icon::xsmall()` — `size_3` (status AND action leads render xsmall,
    /// so one constant covers both — EXP-426).
    const LEAD_ICON_REMS: f32 = 0.75;
    /// The `size_1p5` liveness dot (`surface::rich_tab`'s `RichTabStatus::Dot`).
    const LEAD_DOT_REMS: f32 = 0.375;
    match lead {
        ChipLead::None => 0.,
        ChipLead::Status(_) | ChipLead::Shell => LEAD_ICON_REMS,
        ChipLead::Dot(_) => LEAD_DOT_REMS,
    }
}

/// EXP-746/EXP-769: a session chip, the web `DockTab` piece for piece — the
/// liveness dot (`tabStatus`), the issue identifier in the mono slot, the
/// subject, and the ` · machine` caption; a paused host dims the chip. Every
/// degrade (no row yet, the issue still syncing) lands on the generic label,
/// like an issue tab's "Issue" — a tab is chrome, so it never renders a
/// transient status string.
fn session_chip_content(session_id: &str, cx: &App) -> ChipContent {
    let muted = cx.theme().muted_foreground.opacity(0.5);
    let Some(store) = Store::try_global(cx) else {
        return ChipContent {
            lead: ChipLead::Dot(muted),
            ..ChipContent::plain("Session".into())
        };
    };
    let collections = store.collections();
    let sessions = collections.coding_sessions.read(cx);
    let Some(row) = sessions.get(session_id) else {
        return ChipContent {
            lead: ChipLead::Dot(muted),
            ..ChipContent::plain("Session".into())
        };
    };
    let issues = collections.issues.read(cx);
    let issue = row.issue_id.as_deref().and_then(|issue_id| issues.get(issue_id));
    let (identifier, title): (Option<gpui::SharedString>, gpui::SharedString) = match issue {
        Some(issue) => {
            let title = issue.title.trim();
            (
                Some(gpui::SharedString::from(issue.identifier.clone())),
                if title.is_empty() {
                    "Untitled issue".into()
                } else {
                    title.to_string().into()
                },
            )
        }
        None if row.issue_id.is_some() => (None, "Session".into()),
        None => (
            None,
            row.action_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(|name| gpui::SharedString::from(name.to_string()))
                // An issue-less, action-less run is a batch (`exp/batch-<id8>`).
                .unwrap_or_else(|| "Batch".into()),
        ),
    };
    let now = chrono::Utc::now().timestamp();
    let presentation = crate::queries::session_device_presentation(
        row,
        collections.devices.read(cx).iter(),
        now * 1_000,
    );
    let caption = presentation
        .label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(|label| gpui::SharedString::from(format!("· {label}")));
    // An ended run keeps its tab as a read-only transcript — its dot says so
    // rather than claiming the agent is still working.
    let ended = row.status.as_deref() == Some(domain::contract::CODING_SESSION_STATUS_ENDED);
    let display = crate::queries::coding_session_display(
        row,
        issue
            .and_then(|issue| issue.pr_state.as_deref())
            .or(row.pr_state.as_deref()),
    );
    let paused = !ended && crate::queries::session_is_paused(display, &presentation);
    let tone = if ended || paused {
        muted
    } else {
        match display {
            crate::queries::CodingSessionDisplay::NeedsInput => theme::tokens::YELLOW.to_hsla(),
            crate::queries::CodingSessionDisplay::Done => theme::tokens::BLUE.to_hsla(),
            crate::queries::CodingSessionDisplay::Review
            | crate::queries::CodingSessionDisplay::Running => theme::tokens::GREEN.to_hsla(),
        }
    };
    ChipContent {
        lead: ChipLead::Dot(tone),
        identifier,
        title: Some(title),
        caption,
        badge: None,
        paused,
    }
}

/// EXP-769: a terminal chip — the retired dock strip's local chip: an issue
/// coding run on the PTY path renders the center issue-tab treatment (status
/// glyph + mono identifier + synced title), everything else the terminal
/// glyph and the tab's own title; an exited child adds its code as a badge.
fn terminal_chip_content(tab: terminal::TabId, cx: &App) -> ChipContent {
    let Some(manager) = crate::session_bar::manager_for_tab(tab, cx) else {
        return ChipContent {
            lead: ChipLead::Shell,
            ..ChipContent::plain("Terminal".into())
        };
    };
    let manager = manager.read(cx);
    let Some(entry) = manager.tab(tab) else {
        return ChipContent {
            lead: ChipLead::Shell,
            ..ChipContent::plain("Terminal".into())
        };
    };
    let badge = entry.exit_code().map(|code| {
        let color = if code == 0 {
            cx.theme().success
        } else {
            cx.theme().danger
        };
        (gpui::SharedString::from(code.to_string()), color)
    });
    // EXP-773: a terminal tab is a plain shell or an agent login — a coding
    // run has its own screen, so no issue chip can resolve from a tab.
    let mut content = ChipContent {
        lead: ChipLead::Shell,
        ..ChipContent::plain(entry.title().clone())
    };
    content.badge = badge;
    content
}

fn chip_content(screen: &Screen, cx: &App) -> ChipContent {
    if let Screen::Session { session_id } = screen {
        // The tab strip says WHAT is running and how it is doing without the
        // tab having to be open — the dock's remote chips did this, and the
        // session screen inherits it.
        return session_chip_content(session_id, cx);
    }
    if let Screen::Terminal { tab } = screen {
        return terminal_chip_content(*tab, cx);
    }
    if let Screen::IssueDetail { issue_id } = screen {
        let store = Store::global(cx);
        let issues = store.collections().issues.read(cx);
        if let Some(issue) = issues.get(issue_id) {
            let title = issue.title.trim();
            let title = if title.is_empty() {
                None
            } else {
                Some(gpui::SharedString::from(title.to_string()))
            };
            let resolved = crate::queries::resolve_issue_status(cx, issue);
            return ChipContent {
                lead: ChipLead::Status(resolved),
                identifier: Some(gpui::SharedString::from(issue.identifier.clone())),
                title,
                caption: None,
                badge: None,
                paused: false,
            };
        }
    }
    ChipContent::plain(screen_title(screen, cx))
}

/// EXP-746, THE rule of the resume swap: an open tab for `resumed_from`
/// becomes `session_id`'s.
///
/// `Screen::Session` keys on the row id and a resume mints a new one, so
/// without this a resumed run opens a SECOND tab beside the one it continues.
/// A run only takes a tab over when it resumes an id that is open and is not
/// itself open yet: a resume of a resume converges one step per tick, and two
/// tabs the user opened by hand stay two tabs.
///
/// Two paths can see the link first and both call this, so the swap happens
/// exactly once whichever wins the race: the synced row ([`resume_swaps`],
/// the only path for a resume started on another device) and, for a LOCAL
/// resume, [`take_over_session_tab`] at open time — the engine starts
/// synchronously while the new row's Electric echo is a network round trip,
/// so the tab is usually already open by the time the row lands.
fn takes_over_tab(open: &[String], resumed_from: &str, session_id: &str) -> bool {
    open.iter().any(|tab| tab == resumed_from) && !open.iter().any(|tab| tab == session_id)
}

/// EXP-746, pure core of the synced half: which OPEN session tab each new row
/// takes over ([`takes_over_tab`]). Sorted by the displaced id, because the
/// synced rows arrive in no useful order and the swap must be deterministic.
fn resume_swaps(open: &[String], rows: &[(String, Option<String>)]) -> Vec<(String, String)> {
    let mut swaps: Vec<(String, String)> = rows
        .iter()
        .filter_map(|(id, resumed_from)| {
            let resumed_from = resumed_from.as_ref()?;
            takes_over_tab(open, resumed_from, id).then(|| (resumed_from.clone(), id.clone()))
        })
        .collect();
    swaps.sort();
    // One tab can only become one session; the sort makes "the first" stable.
    swaps.dedup_by(|a, b| a.0 == b.0);
    swaps
}

/// Which tab the center falls back to when the ACTIVE tab at `ix` closes.
///
/// `strip[i]` is tab `i`'s [`Screen::is_dock_tab`], and `dock` is the closing
/// tab's own — the search stays strictly inside that ONE strip: the next tab
/// to the right, else back to the left. Closing a bottom-bar session tab
/// lands on the next session, closing a top tab on the next top tab.
///
/// EXP-781: `None` when the strip is now empty, which [`set_screen`] reads as
/// "clear the center". This used to fall back to any remaining tab, so
/// closing the LAST session tab activated an unrelated issue.
///
/// `ix` is the index the closed tab occupied, i.e. `strip` is already the
/// post-removal list and `strip[ix]` is the tab that shifted into its place.
fn neighbor_in_strip(strip: &[bool], ix: usize, dock: bool) -> Option<usize> {
    (ix..strip.len())
        .chain((0..ix).rev())
        .find(|&candidate| strip[candidate] == dock)
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
    /// The Devices page (EXP-686 — the user's machines; the same tab-less
    /// full-page mode).
    devices: Entity<crate::devices_view::DevicesView>,
    /// The Actions page (EXP-467 — the team's action rows; EXP-480: a
    /// tab-less full-page mode like Settings).
    actions: Entity<crate::actions_view::ActionsView>,
    /// The Automations page (EXP-686 — the automation rows plus the
    /// "Recent automated runs" log).
    automations: Entity<crate::automations_view::AutomationsView>,
    /// The Chat page (EXP-772 — the centred prompt box; the same tab-less
    /// full-page mode).
    chat: Entity<crate::chat_screen::ChatScreenView>,
    /// The Reviews page (EXP-706 — the team's open PRs; the same tab-less
    /// full-page mode. It was a sidebar tool window until EXP-706).
    reviews: Entity<crate::reviews_view::ReviewsView>,
    /// The Getting-started checklist page (EXP-470 — the same tab-less
    /// full-page mode, behind a conditional rail entry).
    getting_started: Entity<crate::getting_started::GettingStartedView>,
    /// EXP-746: one session screen per OPEN session tab, keyed by the
    /// `coding_sessions` row id. Not a shared single instance like the views
    /// above: each one owns a feed (a relay socket, or the local engine's
    /// drain), so it is created on first activation and lives exactly as long
    /// as its tab — every removal path shuts it down.
    sessions: HashMap<String, Entity<crate::session_screen::SessionScreenView>>,
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
    /// EXP-698 round 5: the "No boards yet" state scrolls — it carries the
    /// Getting-started cards under it.
    empty_scroll: gpui::ScrollHandle,
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
        // EXP-525: only the in-shell instance offers "open in new window" —
        // `build_screen_content`'s undocked-window instances must not.
        pr_diff.update(cx, |diff, _| diff.show_undock = true);
        let devices = cx.new(|cx| crate::devices_view::DevicesView::new(window, cx));
        let actions = cx.new(|cx| crate::actions_view::ActionsView::new(window, cx));
        let automations =
            cx.new(|cx| crate::automations_view::AutomationsView::new(window, cx));
        let chat = cx.new(|cx| crate::chat_screen::ChatScreenView::new(window, cx));
        let reviews = cx.new(|cx| crate::reviews_view::ReviewsView::new(window, cx));
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
            // A go-back can land on a diff whose PR merged while the entry
            // sat on the stack (EXP-525) — retire it immediately.
            this.dismiss_stale_pr_diff(window, cx);
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
                this.dismiss_stale_pr_diff(window, cx);
                cx.notify();
            },
        ));
        subscriptions.push(cx.observe(&collections.boards, |_, _, cx| cx.notify()));
        // EXP-746: session tabs read their identity, their liveness and the
        // resume swap off this collection — a run that ends, or is resumed
        // into a NEW row, has to reach the open tab without a navigation.
        subscriptions.push(cx.observe_in(
            &collections.coding_sessions,
            window,
            |this, _, window, cx| {
                this.sync_session_tabs(window, cx);
                cx.notify();
            },
        ));
        // EXP-698 round 5: the "No boards yet" center renders the
        // Getting-started cards — same reason the issue list observes it
        // (tRPC one-shot signals no collection echo covers).
        let gs_progress = crate::getting_started::GettingStartedProgress::global(cx);
        subscriptions.push(cx.observe(&gs_progress, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe_in(
            &Store::global(cx).state(),
            window,
            |this, _, window, cx| {
                this.sync_tabs(window, cx);
                // The session phase rides this entity: a screen pointed
                // during construction (before the phase reached Synced) never
                // got a tRPC client, so re-drive it here rather than waiting
                // for a navigation that may never come.
                this.redrive_active_screen(cx);
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
            empty_scroll: gpui::ScrollHandle::new(),
            nav,
            issue_detail,
            settings,
            source_control,
            file_viewer,
            support_thread,
            pr_diff,
            devices,
            actions,
            automations,
            chat,
            reviews,
            getting_started,
            sessions: HashMap::new(),
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
        self.sync_active_screen_inner(false, cx);
    }

    /// Re-drive the CURRENT screen even though it hasn't changed. This panel
    /// is built from the shell's constructor, which runs while the session is
    /// still validating — a tRPC-backed view pointed at that moment has no
    /// client, leaves itself unpointed, and would never be asked again
    /// (`sync_active_screen` early-returns on an unchanged screen). The
    /// session observer calls this on every phase flip so the pending screen
    /// finally loads once the phase reaches Synced.
    fn redrive_active_screen(&mut self, cx: &mut gpui::Context<Self>) {
        self.sync_active_screen_inner(true, cx);
    }

    fn sync_active_screen_inner(&mut self, force: bool, cx: &mut gpui::Context<Self>) {
        let screen = resolved_screen(&self.nav, cx);
        let changed = screen != self.active_screen;
        if !changed && !force {
            return;
        }
        if changed {
            let entered_settings = matches!(screen, Some(Screen::Settings));
            // EXP-706: the Reviews page's unlinked-PR half is a tRPC fetch, not
            // a synced shape — entering the screen is its only refresh signal
            // (the same reason Settings marks its personal panes stale).
            let entered_reviews = matches!(screen, Some(Screen::Reviews));
            self.active_screen = screen;
            if entered_settings {
                self.settings
                    .update(cx, |settings, cx| settings.mark_personal_stale(cx));
            }
            if entered_reviews {
                self.reviews
                    .update(cx, |reviews, cx| reviews.mark_pulls_stale(cx));
            }
        }
        // EXP-525: PrDiff is a transient (tab-less) center view — re-point
        // the shared diff view here instead of in `sync_tabs`. Same-id
        // re-points are no-ops, so a forced pass costs nothing once loaded.
        if let Some(Screen::PrDiff { issue_id }) = &self.active_screen {
            let issue_id = issue_id.clone();
            self.pr_diff
                .update(cx, |diff, cx| diff.set_issue(issue_id, cx));
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
        // EXP-288: the origin marker rides every REAL navigation (tab clicks
        // never set it). EXP-791: so does the steer marker — consumed HERE,
        // unconditionally, so a marker left by a navigation that never
        // reached its issue can't survive to the next one.
        let pending_origin = crate::navigation::take_pending_origin(&self.nav, cx);
        let team = active_team_id(&self.nav, cx);
        if team != self.tabs_team {
            // Dropping the tabs tears the issue detail down without a blur —
            // flush a pending description edit first (EXP-68).
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
            self.tabs_team = team;
            // EXP-769: terminal tabs SURVIVE a team switch — a PTY is not
            // team-scoped, and dropping its chip would orphan a running shell
            // (the manager would keep it alive, invisibly). Everything else
            // is team data and goes.
            self.tabs
                .retain(|tab| matches!(tab.screen, Screen::Terminal { .. }));
            // EXP-746: the session views go with their tabs (a dropped tab
            // must not keep a relay socket or an engine drain alive).
            self.shutdown_all_sessions(cx);
            // The sidebar selections are team-scoped too (trunk-relative
            // paths / commit hashes of the OLD team's clone).
            self.rail.update(cx, |rail, cx| {
                rail.clear_selected_file(cx);
                rail.clear_sc_selection(cx);
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
        // ran with its tool already active) — EXP-818: unless the navigation
        // came from a context-free screen, in which case the detail brings
        // its own list (`navigation::derive_origin`). `None` (go-back / tab
        // reactivation of a closed tab) falls back to the same rule.
        let captured = {
            let rail = self.rail.read(cx);
            let tool = rail.tool();
            let captured = TabOrigin {
                board_id: (tool == ToolWindow::BoardIssues)
                    .then(|| active_board_id(&self.nav, cx))
                    .flatten(),
                inbox_tab: (tool == ToolWindow::Inbox).then(|| rail.inbox_tab()),
                tool,
            };
            let target_board = match &screen {
                Screen::IssueDetail { issue_id } | Screen::PrDiff { issue_id } => Store::global(cx)
                    .collections()
                    .issues
                    .read(cx)
                    .get(issue_id)
                    .map(|issue| issue.board_id.clone()),
                _ => None,
            };
            let nav = self.nav.read(cx);
            crate::navigation::derive_origin(nav.previous_screen(), captured, &screen, target_board)
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
                self.tabs.push(TabEntry {
                    screen: screen.clone(),
                    origin,
                });
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
            Screen::Session { session_id } => {
                // ENTRY-OR-INSERT, never a re-point: each session owns its
                // feed, so re-pointing one view at another row would hand the
                // new session the old one's socket. Built lazily — a
                // background tab has no view until it is activated.
                self.sessions.entry(session_id.clone()).or_insert_with(|| {
                    cx.new(|cx| {
                        crate::session_screen::SessionScreenView::new(session_id, window, cx)
                    })
                });
            }
            Screen::Terminal { tab } => {
                // EXP-769: the manager's active tab follows the screen (cmd-w
                // closes the ACTIVE tab; the Latest-changes poll reads it),
                // and the grid takes the keyboard.
                if let Some(host) = crate::session_bar::host_for_window(window, cx) {
                    host.update(cx, |host, cx| host.activate_tab_by_id(tab, window, cx));
                }
            }
            Screen::PrDiff { .. }
            | Screen::Devices
            | Screen::Actions
            | Screen::Automations
            | Screen::Chat
            | Screen::Reviews
            | Screen::GettingStarted { .. }
            | Screen::Settings => {
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
    /// not-found condition.
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
                    Screen::IssueDetail { issue_id } => {
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

    /// EXP-746: reconcile the open session tabs with the synced rows.
    ///
    /// Deliberately NOT `dismiss_stale_pr_diff`'s model: an ended run KEEPS
    /// its tab (it becomes a read-only transcript, and Past reopens it), so
    /// this only (a) marks the ended edge on the view, and (b) performs the
    /// resume swap — a resume mints a NEW row id, which would otherwise open
    /// a second tab beside the run it continues. Titles ride the observer's
    /// `cx.notify()`; nothing here recomputes them.
    fn sync_session_tabs(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let (swaps, ended) = {
            let sessions = Store::global(cx).collections().coding_sessions.read(cx);
            let open = self.open_session_ids();
            if open.is_empty() {
                return;
            }
            let rows: Vec<(String, Option<String>)> = sessions
                .iter()
                .map(|row| (row.id.clone(), row.resumed_from_id.clone()))
                .collect();
            let ended: Vec<String> = open
                .iter()
                .filter(|id| {
                    sessions
                        .get(id.as_str())
                        .and_then(|row| row.status.as_deref())
                        .is_some_and(|status| {
                            status == domain::contract::CODING_SESSION_STATUS_ENDED
                        })
                })
                .cloned()
                .collect();
            (resume_swaps(&open, &rows), ended)
        };
        for session_id in ended {
            if let Some(view) = self.sessions.get(&session_id) {
                // The synced row carries no failure reason (EXP-758): the
                // engine's own exit is what has one.
                view.update(cx, |view, cx| view.mark_ended(None, cx));
            }
        }
        for (old_id, new_id) in swaps {
            self.take_over_session_tab(&old_id, &new_id, window, cx);
        }
    }

    /// EXP-818: the remembered origin of `screen`'s tab, if it has one.
    pub(crate) fn origin_of(&self, screen: &Screen) -> Option<TabOrigin> {
        self.tabs
            .iter()
            .find(|tab| &tab.screen == screen)
            .map(|tab| tab.origin.clone())
    }

    /// EXP-791: whether the bottom session bar has anything to show — it
    /// takes no height without a terminal tab (`session_bar::bar_visible`).
    pub(crate) fn has_terminal_tabs(&self) -> bool {
        self.tabs
            .iter()
            .any(|tab| matches!(tab.screen, Screen::Terminal { .. }))
    }

    /// EXP-791: what a Sessions-rail row's × does — the retired bar chip's
    /// semantics (`dock_close_action`): a live run of the caller's is KILLED
    /// (confirmed), an ended one's transcript tab closes.
    pub(crate) fn close_session_row(
        &mut self,
        session_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let screen = Screen::Session {
            session_id: session_id.to_string(),
        };
        let close = self.dock_close_action(&screen, cx);
        self.run_dock_close(&close, window, cx);
    }

    /// The row ids of this panel's open session tabs, in tab order — EXP-791:
    /// the rail's Sessions rows lead with them (`sidebar::rail_session_rows`).
    pub(crate) fn open_session_ids(&self) -> Vec<String> {
        self.tabs
            .iter()
            .filter_map(|tab| match &tab.screen {
                Screen::Session { session_id } => Some(session_id.clone()),
                _ => None,
            })
            .collect()
    }

    /// EXP-746: hand `resumed_from`'s open tab to `session_id` — D5's in-place
    /// resume swap, applied to whichever of the two paths in
    /// [`takes_over_tab`] gets here first. `false` when there is nothing to
    /// take over (the swap already happened, or the user has both tabs open).
    fn take_over_session_tab(
        &mut self,
        resumed_from: &str,
        session_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !takes_over_tab(&self.open_session_ids(), resumed_from, session_id) {
            return false;
        }
        let old = Screen::Session {
            session_id: resumed_from.to_string(),
        };
        let new = Screen::Session {
            session_id: session_id.to_string(),
        };
        if let Some(ix) = self.tabs.iter().position(|tab| tab.screen == old) {
            // The tab keeps its slot and origin; only its identity changes
            // (its view, if it has one, is dropped below and rebuilt by
            // `sync_tabs` when the tab is next activated). EXP-791: the
            // in-place navigation marker that used to do this for the ACTIVE
            // tab is gone with the prev/next switcher — a plain `set_screen`
            // onto the already-renamed tab is the same swap without it.
            self.tabs[ix].screen = new.clone();
        }
        let active = resolved_screen(&self.nav, cx).as_ref() == Some(&old);
        self.shutdown_session_view(&old, cx);
        if active {
            set_screen(window, cx, Some(new));
        }
        true
    }

    /// This panel's screen for `session_id`, if that tab is open and has been
    /// activated at least once.
    fn session_view(
        &self,
        session_id: &str,
    ) -> Option<Entity<crate::session_screen::SessionScreenView>> {
        self.sessions.get(session_id).cloned()
    }

    /// EXP-525: review diffs are transient center views (no tab). The
    /// moment the PR stops being reviewable — merged or closed, arriving as
    /// the Electric echo flipping `pr_state` on every linked issue (batch
    /// PRs included), or the issue disappearing outright — the diff view
    /// retires itself: matching back-stack entries are purged so go-back
    /// can't resurrect it, then the center falls back — go-back if possible,
    /// else the Reviews PAGE the diff was opened from (EXP-706; it used to be
    /// "clear the center", which under a tool-window Reviews left the list
    /// showing and now would leave a blank page).
    fn dismiss_stale_pr_diff(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(Screen::PrDiff { issue_id }) = resolved_screen(&self.nav, cx) else {
            return;
        };
        let stale = {
            let issues = Store::global(cx).collections().issues.read(cx);
            if !issues.is_ready() {
                return;
            }
            match issues.get(&issue_id) {
                Some(issue) => !crate::queries::is_reviewable(issue),
                None => true,
            }
        };
        if !stale {
            return;
        }
        crate::navigation::purge_from_back_stack(window, cx, |screen| {
            matches!(screen, Screen::PrDiff { issue_id: id } if *id == issue_id)
        });
        if self.nav.read(cx).can_go_back() {
            crate::navigation::go_back(window, cx);
        } else {
            crate::navigation::set_screen(window, cx, Some(Screen::Reviews));
        }
    }

    /// Activate the tab at `ix`: re-select its origin sidebar entry (and
    /// board), then show its screen (EXP-288). Order matters — tool, board,
    /// then screen — so observers reading tool/board during the nav notify
    /// see final state. Deliberately NOT `navigate`: activation must never
    /// rewrite the tab's remembered origin.
    ///
    /// EXP-769: a terminal tab just shows its screen — it has no meaningful
    /// origin at all. EXP-818: a SESSION tab restores its origin like an
    /// issue tab does (the list it was opened from sits beside it).
    fn activate_tab(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(entry) = self.tabs.get(ix).cloned() else {
            return;
        };
        if matches!(entry.screen, Screen::Terminal { .. }) {
            set_screen(window, cx, Some(entry.screen));
            return;
        }
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

    /// Close the tab at `ix`. Closing the active tab activates its right
    /// neighbor (else the new last); closing the last clears the center.
    /// Direct tab management never touches the back stack.
    ///
    /// EXP-769: a TERMINAL tab's close is the terminal's close — the child is
    /// killed and the manager drops the tab (the retired dock's cmd-w; an
    /// issue run on the PTY path ends its row through the exit hook). The
    /// manager's `TabClosed` echo then finds the entry already gone.
    fn close_tab(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.remove_tab(ix, true, window, cx);
    }

    /// [`Self::close_tab`]'s core. `kill_terminal` is false on the paths where
    /// the terminal itself must survive: the manager's own `TabClosed` echo
    /// (already gone) and an undock (the tab moves to its own window).
    fn remove_tab(
        &mut self,
        ix: usize,
        kill_terminal: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if ix >= self.tabs.len() {
            return;
        }
        // Closing (or undocking) the active issue tab unmounts the detail's
        // description editor without a blur — flush the pending edit so it
        // is written before teardown (EXP-68).
        if let Screen::IssueDetail { .. } = &self.tabs[ix].screen {
            self.issue_detail.update(cx, |detail, cx| {
                // EXP-781: the title saves on blur too, so closing straight
                // from a half-typed title dropped it.
                detail.flush_title(cx);
                detail.flush_description(cx);
            });
        }
        let closed = self.tabs.remove(ix);
        self.shutdown_session_view(&closed.screen, cx);
        let active = resolved_screen(&self.nav, cx);
        if active.as_ref() == Some(&closed.screen) {
            // The neighbor within the SAME strip: closing a bottom-bar tab
            // lands on the next bottom-bar tab (the web's dock never jumps to
            // an issue), closing a top tab on the next top tab.
            let strip: Vec<bool> = self.tabs.iter().map(|tab| tab.screen.is_dock_tab()).collect();
            let next = neighbor_in_strip(&strip, ix, closed.screen.is_dock_tab())
                .map(|ix| self.tabs[ix].screen.clone());
            set_screen(window, cx, next);
        }
        if let (true, Screen::Terminal { tab }) = (kill_terminal, &closed.screen) {
            let tab = *tab;
            if let Some(host) = crate::session_bar::host_for_window(window, cx) {
                host.update(cx, |host, cx| host.close_terminal(tab, cx));
            }
        }
        // EXP-769: a terminal that left this strip — closed (the PTY is gone)
        // or undocked (it paints in its own window now) — must not come back
        // through go-back: `sync_tabs` would see a screen with no tab and push
        // a ghost chip over "This terminal was closed". Same rule as the stale
        // PR diff ([`Self::dismiss_stale_pr_diff`]); an issue tab is different
        // — its screen is still openable, so its history stays.
        if matches!(closed.screen, Screen::Terminal { .. }) {
            crate::navigation::purge_from_back_stack(window, cx, |screen| *screen == closed.screen);
        }
        cx.notify();
    }

    /// EXP-769: drop `screen`'s tab if it is open — the session bar host's
    /// `TabClosed` echo (the terminal is already gone, so never kill). A
    /// no-op for a screen without a tab.
    pub(crate) fn close_screen_tab(
        &mut self,
        screen: &Screen,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(ix) = self.tabs.iter().position(|tab| &tab.screen == screen) {
            self.remove_tab(ix, false, window, cx);
        }
    }

    /// EXP-769/EXP-791: the session bar's entries, in bar order — the open
    /// TERMINAL tabs. Sessions live in the rail's Sessions section now
    /// ([`Self::rail_session_ids`]).
    fn dock_entries(&self) -> Vec<DockEntry> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| matches!(tab.screen, Screen::Terminal { .. }))
            .map(|(ix, tab)| DockEntry {
                ix,
                screen: tab.screen.clone(),
            })
            .collect()
    }

    /// Close every TOP-strip tab except `ix` (EXP-235 context menu). The kept
    /// tab becomes active — the active tab may be among the closed ones.
    /// EXP-769: the session bar's tabs are another strip and stay.
    fn close_other_tabs(&mut self, ix: usize, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if ix >= self.tabs.len() || self.top_tab_count() <= 1 {
            return;
        }
        let keep = self.tabs[ix].screen.clone();
        let goes = |screen: &Screen| *screen != keep && !screen.is_dock_tab();
        // Same EXP-68 flush as `close_tab`: a closing issue tab may hold a
        // pending description edit.
        if self
            .tabs
            .iter()
            .any(|tab| goes(&tab.screen) && matches!(tab.screen, Screen::IssueDetail { .. }))
        {
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
        }
        let dropped: Vec<Screen> = self
            .tabs
            .iter()
            .filter(|tab| goes(&tab.screen))
            .map(|tab| tab.screen.clone())
            .collect();
        self.tabs.retain(|tab| !goes(&tab.screen));
        for screen in &dropped {
            self.shutdown_session_view(screen, cx);
        }
        set_screen(window, cx, Some(keep));
        cx.notify();
    }

    /// Close every TOP-strip tab (EXP-235 context menu) and clear the center.
    /// EXP-769: the session bar's tabs stay (a terminal must never be killed
    /// by a context menu on the issue strip).
    fn close_all_tabs(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.top_tab_count() == 0 {
            return;
        }
        if self
            .tabs
            .iter()
            .any(|tab| !tab.screen.is_dock_tab() && matches!(tab.screen, Screen::IssueDetail { .. }))
        {
            self.issue_detail
                .update(cx, |detail, cx| detail.flush_description(cx));
        }
        let dropped: Vec<Screen> = self
            .tabs
            .iter()
            .filter(|tab| !tab.screen.is_dock_tab())
            .map(|tab| tab.screen.clone())
            .collect();
        self.tabs.retain(|tab| tab.screen.is_dock_tab());
        for screen in &dropped {
            self.shutdown_session_view(screen, cx);
        }
        // The center clears only if a closed tab was showing; a session bar
        // tab that was up stays up.
        if resolved_screen(&self.nav, cx).is_some_and(|screen| dropped.contains(&screen)) {
            set_screen(window, cx, None);
        }
        cx.notify();
    }

    /// EXP-769: how many tabs the TOP strip holds.
    fn top_tab_count(&self) -> usize {
        self.tabs
            .iter()
            .filter(|tab| !tab.screen.is_dock_tab())
            .count()
    }

    /// EXP-746: drop the session view a closing tab owned. Never called for
    /// any other screen kind — the shared single-instance views outlive every
    /// tab. Shutting the view down drops its FEED, not the run.
    fn shutdown_session_view(&mut self, screen: &Screen, cx: &mut gpui::Context<Self>) {
        let Screen::Session { session_id } = screen else {
            return;
        };
        if let Some(view) = self.sessions.remove(session_id) {
            view.update(cx, |view, cx| view.shutdown(cx));
        }
    }

    /// [`Self::shutdown_session_view`] for every open session at once (the
    /// team switch and "Close all tabs", which clear the strip wholesale).
    fn shutdown_all_sessions(&mut self, cx: &mut gpui::Context<Self>) {
        let views: Vec<_> = self.sessions.drain().map(|(_, view)| view).collect();
        for view in views {
            view.update(cx, |view, cx| view.shutdown(cx));
        }
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

    /// EXP-769: pop a TERMINAL tab out into its own window (EXP-65's terminal
    /// path: the manager keeps the tab, the new window renders its view). The
    /// tab entry leaves this bar WITHOUT killing the terminal; reattaching
    /// (or closing that window) navigates back here and the entry returns.
    fn undock_terminal_tab(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(Screen::Terminal { tab }) = self.tabs.get(ix).map(|tab| tab.screen.clone())
        else {
            return;
        };
        let Some(host) = crate::session_bar::host_for_window(window, cx) else {
            return;
        };
        self.remove_tab(ix, false, window, cx);
        host.update(cx, |host, cx| host.undock_tab(tab, window, cx));
    }

    /// Chip width for the overflow computation.
    ///
    /// EXP-326: both halves of this used to be guesses that ran long, which
    /// is why the strip collapsed tabs into "+N" with visible room still to
    /// its right. The labels are SHAPED with the window's own text system
    /// now, and the chrome is expressed in the units gpui actually lays it
    /// out in: every spacing helper (`px_2`, `gap_1`, `size_3`, `size_5`,
    /// `gap_0p5`) resolves against the REM size, and this app's rem is
    /// [`theme::FONT_SIZE_PX`] (14px), not the 16px browser root — reading
    /// them as their 16px-rem pixel values inflated every chip. The one
    /// genuine pixel constant is the title's `max_w`.
    fn measure_chip_width(&self, entry: &TabEntry, window: &Window, cx: &App) -> f32 {
        /// `surface::rich_tab`'s `px_2p5`, both sides.
        const CHIP_PADDING_REMS: f32 = 0.625 * 2.;
        /// An icon-only xsmall `Button` — `size_5`.
        const XSMALL_BUTTON_REMS: f32 = 1.25;
        /// The trailing button cluster's own `gap_0p5`.
        const CLUSTER_GAP_REMS: f32 = 0.125;
        /// `surface::RICH_TAB_TITLE_MAX_W` on the title child — a real pixel
        /// value, so it does NOT scale with the rem.
        const TITLE_MAX_W: f32 = crate::surface::RICH_TAB_TITLE_MAX_W;

        let rem = f32::from(window.rem_size());
        let content = chip_content(&entry.screen, cx);
        let base_font = window.text_style().font();
        let mut children: Vec<f32> = Vec::with_capacity(4);
        let lead_reserve = lead_reserve_rems(&content.lead);
        if lead_reserve > 0. {
            children.push(lead_reserve * rem);
        }
        if let Some(identifier) = content.identifier.as_ref() {
            // EXP-310: the shortcode renders `text_xs` in the terminal mono
            // family, not the bar's proportional font.
            let mut font = base_font.clone();
            font.family = theme::terminal::FONT_FAMILY.into();
            children.push(measure_text(window, identifier, font, gpui::rems(0.75)));
        }
        if let Some(title) = content.title.as_ref() {
            let width = measure_text(window, title, base_font.clone(), gpui::rems(0.875));
            children.push(width.min(TITLE_MAX_W));
        }
        // EXP-769: the session chip's ` · machine` caption (`text_xs`, capped
        // at `RICH_TAB_CAPTION_MAX_W`) and the terminal chip's exit badge
        // (`px_1` a side around a `text_xs` code).
        if let Some(caption) = content.caption.as_ref() {
            let width = measure_text(window, caption, base_font.clone(), gpui::rems(0.75));
            children.push(width.min(crate::surface::RICH_TAB_CAPTION_MAX_W));
        }
        if let Some((label, _)) = content.badge.as_ref() {
            children.push(
                0.5 * rem + measure_text(window, label, base_font, gpui::rems(0.75)),
            );
        }
        // The undock slot is `invisible`, not absent, so it keeps its box.
        children.push(if entry.screen.undockable() {
            (XSMALL_BUTTON_REMS * 2. + CLUSTER_GAP_REMS) * rem
        } else {
            XSMALL_BUTTON_REMS * rem
        });

        let gaps = rich_tab_child_gap(window) * children.len().saturating_sub(1) as f32;
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
        // EXP-769: the TOP strip holds the issue/support tabs only — the
        // session and terminal tabs are the bottom session bar's
        // (`render_session_bar_tabs`). `top` maps strip position → real tab
        // index; every handler keys on the real index.
        let top: Vec<usize> = (0..self.tabs.len())
            .filter(|&ix| !self.tabs[ix].screen.is_dock_tab())
            .collect();
        if top.is_empty() {
            return gpui::Empty.into_any_element();
        }
        let active = resolved_screen(&self.nav, cx);
        let active_ix = active
            .as_ref()
            .and_then(|screen| self.tabs.iter().position(|tab| &tab.screen == screen));
        let active_pos = active_ix.and_then(|ix| top.iter().position(|&t| t == ix));
        let panel = cx.entity().downgrade();
        let tab_count = top.len();

        let widths: Vec<f32> = top
            .iter()
            .map(|&ix| self.measure_chip_width(&self.tabs[ix], window, cx))
            .collect();
        let visible = partition_tabs(
            &widths,
            f32::from(available),
            chip_gap(window),
            overflow_button_width(window, tab_count.saturating_sub(1)),
            active_pos,
        );
        let hidden: Vec<usize> = (0..tab_count)
            .filter(|pos| !visible.contains(pos))
            .map(|pos| top[pos])
            .collect();
        let chips: Vec<(usize, Screen)> = visible
            .iter()
            .map(|&pos| (top[pos], self.tabs[top[pos]].screen.clone()))
            .collect();

        let mut strip = h_flex()
            .id("center-tab-strip")
            .max_w_full()
            .gap_1()
            .items_center()
            .children(chips.into_iter().map(|(ix, screen)| {
                let screen = &screen;
                let content = chip_content(screen, cx);
                let mut tab =
                    crate::surface::RichTab::new(("center-tab", ix), Some(ix) == active_ix);
                tab.status = match &content.lead {
                    ChipLead::Dot(tone) => crate::surface::RichTabStatus::Dot(*tone),
                    lead => match lead.icon(cx) {
                        Some(icon) => crate::surface::RichTabStatus::Glyph(icon),
                        None => crate::surface::RichTabStatus::None,
                    },
                };
                tab.identifier = content.identifier;
                tab.title = content.title;
                crate::surface::rich_tab(tab, cx)
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
                    // EXP-698: the lead glyph, the mono shortcode and the
                    // truncating title are `rich_tab`'s standard children;
                    // only the strip-specific trailing cluster is built here.
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
                                                .ghost().cursor_pointer()
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
                                    .ghost().cursor_pointer()
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
            let screens: Vec<Screen> = hidden
                .iter()
                .map(|&ix| self.tabs[ix].screen.clone())
                .collect();
            strip = strip.child(self.overflow_menu("center-tab-overflow", screens, cx));
        }
        strip.into_any_element()
    }

    /// The "+N" dropdown of the tabs a strip could not fit (EXP-288), shared
    /// by both strips (EXP-769). Keyed by SCREEN, not by index: the menu's
    /// closures run at click time, and a tab closed while the dropdown is
    /// open (middle-click on a visible chip, a team switch) shifts every index
    /// after it, which would activate the wrong tab. Tabs are deduped by
    /// screen, so it is a stable identity — and a session-bar entry without a
    /// tab has only its screen anyway.
    fn overflow_menu(
        &self,
        id: &'static str,
        hidden: Vec<Screen>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        // EXP-310: the menu rows carry the same lead glyph + shortcode as
        // the chips (composed into the label — menu items are plain icon +
        // text).
        let hidden_entries: Vec<(Screen, ChipLead, gpui::SharedString)> = hidden
            .into_iter()
            .map(|screen| {
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
        let panel = cx.entity().downgrade();
        Button::new(id)
            .ghost().cursor_pointer()
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
                    menu = menu.item(item.on_click(move |_, window, cx| {
                        let _ = panel.update(cx, |this, cx| {
                            this.open_screen(screen.clone(), window, cx);
                        });
                    }));
                }
                menu
            })
    }

    /// Show `screen`: activate its tab when one is open (origin restore and
    /// all), else navigate to it — a session-bar entry without a tab
    /// (EXP-769) opens exactly like Devices → Running would open it.
    fn open_screen(&mut self, screen: Screen, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match self.tabs.iter().position(|tab| tab.screen == screen) {
            Some(ix) => self.activate_tab(ix, window, cx),
            None => match screen {
                Screen::Session { session_id } => {
                    crate::session_screen::open_session(&session_id, window, cx)
                }
                other => crate::navigation::navigate(window, cx, other),
            },
        }
    }

    /// EXP-769: the bottom session bar's TABS — one rich tab per
    /// [`DockEntry`], the ACTIVE one following the screen the center shows,
    /// the rest folding into "+N" past `available` (the same measured
    /// partition as the top strip). Hosted by the
    /// [`crate::session_bar::SessionBar`], which appends the `+` button and
    /// records `available` off its own painted slot. EXP-791: terminal chips
    /// only — sessions are rail rows now.
    ///
    /// The trailing ×: a terminal's × closes the terminal (kills the child).
    /// Middle-click is the same. The chip's context menu adds "Open in new
    /// window" (EXP-65's terminal undock) — the only place that affordance is
    /// left.
    pub(crate) fn render_session_bar_tabs(
        &mut self,
        available: f32,
        trailing: Vec<gpui::AnyElement>,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::AnyElement {
        let entries = self.dock_entries();
        let active = resolved_screen(&self.nav, cx);
        let active_pos = active
            .as_ref()
            .and_then(|screen| entries.iter().position(|entry| &entry.screen == screen));
        let count = entries.len();
        let widths: Vec<f32> = entries
            .iter()
            .map(|entry| self.measure_chip_width(&self.tabs[entry.ix], window, cx))
            .collect();
        let visible = partition_tabs(
            &widths,
            available,
            chip_gap(window),
            overflow_button_width(window, count.saturating_sub(1)),
            active_pos,
        );
        let hidden: Vec<Screen> = (0..count)
            .filter(|pos| !visible.contains(pos))
            .map(|pos| entries[pos].screen.clone())
            .collect();

        let panel = cx.entity().downgrade();
        let chips: Vec<gpui::AnyElement> = visible
            .into_iter()
            .map(|pos| {
                let entry = &entries[pos];
                let screen = entry.screen.clone();
                let content = chip_content(&screen, cx);
                let mut tab = crate::surface::RichTab::new(
                    ("session-bar-tab", pos),
                    Some(pos) == active_pos,
                );
                tab.status = match &content.lead {
                    ChipLead::Dot(tone) => crate::surface::RichTabStatus::Dot(*tone),
                    lead => match lead.icon(cx) {
                        Some(icon) => crate::surface::RichTabStatus::Glyph(icon),
                        None => crate::surface::RichTabStatus::None,
                    },
                };
                tab.identifier = content.identifier;
                tab.title = content.title;
                tab.caption = content.caption;
                tab.badge = content.badge;
                tab.paused = content.paused;
                let close = self.dock_close_action(&screen, cx);
                let close_for_middle = close.clone();
                let open_screen = screen.clone();
                let close_button = Button::new(("close-session-bar-tab", pos))
                    .ghost()
                    .cursor_pointer()
                    .xsmall()
                    .icon(registry::UI_CLOSE)
                    .tooltip(close.label())
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.run_dock_close(&close, window, cx);
                    }));
                let chip = crate::surface::rich_tab(tab, cx)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_screen(open_screen.clone(), window, cx);
                    }))
                    .on_mouse_down(MouseButton::Middle, {
                        let panel = panel.clone();
                        move |_, window, cx| {
                            cx.stop_propagation();
                            let _ = panel.update(cx, |this, cx| {
                                this.run_dock_close(&close_for_middle, window, cx);
                            });
                        }
                    })
                    .child(close_button);
                if let Screen::Terminal { .. } = &screen {
                    let terminal = screen.clone();
                    let panel = panel.clone();
                    return chip.context_menu(move |menu, _window, _cx| {
                        let undock = panel.clone();
                        let undock_screen = terminal.clone();
                        let close = panel.clone();
                        let close_screen = terminal.clone();
                        menu.item(PopupMenuItem::new("Open in new window").on_click(
                            move |_, window, cx| {
                                let _ = undock.update(cx, |this, cx| {
                                    if let Some(ix) = this
                                        .tabs
                                        .iter()
                                        .position(|tab| tab.screen == undock_screen)
                                    {
                                        this.undock_terminal_tab(ix, window, cx);
                                    }
                                });
                            },
                        ))
                        .item(PopupMenuItem::new("Close").on_click(move |_, window, cx| {
                            let _ = close.update(cx, |this, cx| {
                                this.close_screen_tab_killing(&close_screen, window, cx);
                            });
                        }))
                    })
                    .into_any_element();
                }
                chip.into_any_element()
            })
            .collect();

        // The `+` button rides right AFTER the last tab (the JetBrains
        // placement the retired dock used), inside the same slot — the
        // partition above reserved its width.
        h_flex()
            .id("session-bar-tabs")
            .min_w_0()
            .flex_1()
            .overflow_x_hidden()
            .gap_1()
            .items_center()
            .children(chips)
            .when(!hidden.is_empty(), |this| {
                this.child(self.overflow_menu("session-bar-overflow", hidden, cx))
            })
            .children(trailing)
            .into_any_element()
    }

    /// EXP-769: what a session-bar chip's × does — resolved at render time
    /// off the synced row, so the tooltip can say it.
    fn dock_close_action(&self, screen: &Screen, cx: &App) -> DockClose {
        match screen {
            Screen::Terminal { tab } => DockClose::CloseTerminal(*tab),
            Screen::Session { session_id } => {
                match crate::session_bar::kill_target(session_id, cx) {
                    Some(target) => DockClose::Kill(target),
                    None => DockClose::CloseTab(screen.clone()),
                }
            }
            other => DockClose::CloseTab(other.clone()),
        }
    }

    fn run_dock_close(&mut self, close: &DockClose, window: &mut Window, cx: &mut gpui::Context<Self>) {
        match close {
            DockClose::CloseTerminal(tab) => {
                let screen = Screen::Terminal { tab: *tab };
                self.close_screen_tab_killing(&screen, window, cx);
            }
            DockClose::CloseTab(screen) => self.close_screen_tab(screen, window, cx),
            DockClose::Kill(target) => crate::session_bar::prompt_kill(target.clone(), window, cx),
        }
    }

    /// Close `screen`'s tab AND, for a terminal, the terminal itself.
    fn close_screen_tab_killing(
        &mut self,
        screen: &Screen,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match self.tabs.iter().position(|tab| &tab.screen == screen) {
            Some(ix) => self.close_tab(ix, window, cx),
            // No entry (undocked terminal?) — still honor the close.
            None => {
                if let Screen::Terminal { tab } = screen {
                    let tab = *tab;
                    if let Some(host) = crate::session_bar::host_for_window(window, cx) {
                        host.update(cx, |host, cx| host.close_terminal(tab, cx));
                    }
                }
            }
        }
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
                                    .web_sm()
                                    .label("Create team…")
                                    .on_click(|_, window, cx| {
                                        window.dispatch_action(Box::new(CreateTeam), cx);
                                    }),
                            )
                            .child(
                                Button::new("screens-join-team")
                                    .web_sm()
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
        // EXP-698 round 5: the shared empty-state shape (`controls::empty_state`)
        // with the Getting-started checklist under it — the same block the
        // empty board shows, and the web's `/t/$teamSlug` empty page.
        // The 60% band keeps the state optically centred when no cards
        // follow it (a complete checklist, or a team still answering).
        let mut column = v_flex()
            .w_full()
            .min_w_0()
            .min_h(gpui::relative(0.6))
            .items_center()
            .justify_center()
            .gap_3()
            .child(crate::controls::empty_state(
                Icon::new(registry::NAV_BOARDS),
                "No boards yet",
                "Create a board to start tracking work.",
                cx,
            ));
        // No team resolves (e.g. mid team-switch churn): the create
        // action would silently no-op, so don't offer a dead button. (The
        // fully-teamless account is handled by the zero-team branch above.)
        if active_team.is_some() {
            column = column.child(
                Button::new("screens-new-board")
                    .primary()
                    .web_sm()
                    .label("New board…")
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(NewBoard), cx);
                    }),
            );
        }
        let cards = active_team
            .as_deref()
            .and_then(|team_id| crate::getting_started::inline_cards(team_id, cx));
        v_flex()
            .size_full()
            .min_h_0()
            .child(crate::scroll_pane::v_scroll_pane(
                "screens-empty-scroll",
                &self.empty_scroll,
                v_flex()
                    .w_full()
                    .min_w_0()
                    .px_4()
                    .pb_4()
                    .gap_4()
                    .child(column)
                    .children(cards),
            ))
            .into_any_element()
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

// ---------------------------------------------------------------------------
// DEV-ONLY one-shot dialog hook (§11.4 headless verification)
// ---------------------------------------------------------------------------

/// DEV-ONLY `EXP_DEV_DIALOG` values: `create-issue` | `search` |
/// `start-coding` (the active board's first synced issue) |
/// `start-coding-actions` (the builtin "Fix merge conflicts" run) |
/// `start-coding-chat` | `create-action` | `action-editor:<uuid>` |
/// `automation-new` | `automation-edit:<uuid>` | `create-board` |
/// `create-team` | `join-team[:<invite-token>]` (a token prefills the paste
/// field and previews the invite) | `add-server` | `device-settings:<uuid>` |
/// `duplicate-picker:<issue-uuid>` (anything else = no dialog, logged once).
/// Each opens EXACTLY ONCE, 1500ms after the state it needs has resolved, so
/// a capture run lands on the overlay without synthetic input. Never document
/// for users.
#[derive(Clone, Debug, PartialEq, Eq)]
enum DevDialog {
    CreateIssue,
    Search,
    StartCoding,
    StartCodingActions,
    StartCodingChat,
    CreateAction,
    ActionEditor(String),
    AutomationNew,
    AutomationEdit(String),
    CreateBoard,
    CreateTeam,
    /// EXP-642: an optional invite token — `join-team:<token>` opens the
    /// dialog already previewing that invite.
    JoinTeam(Option<String>),
    AddServer,
    DeviceSettings(String),
    DuplicatePicker(String),
}

/// The accepted [`DevDialog`] spellings, for the parse-failure log — a typo
/// in a capture recipe must name its alternatives, not fail silently.
const DEV_DIALOG_SPECS: &str = "create-issue | search | start-coding | \
    start-coding-actions | start-coding-chat | create-action | \
    action-editor:<uuid> | automation-new | automation-edit:<uuid> | \
    create-board | create-team | join-team[:<invite-token>] | add-server | \
    device-settings:<uuid> | duplicate-picker:<issue-uuid>";

fn parse_dev_dialog(spec: &str) -> Option<DevDialog> {
    match spec {
        "create-issue" => Some(DevDialog::CreateIssue),
        "search" => Some(DevDialog::Search),
        "start-coding" => Some(DevDialog::StartCoding),
        "start-coding-actions" => Some(DevDialog::StartCodingActions),
        "start-coding-chat" => Some(DevDialog::StartCodingChat),
        "create-action" => Some(DevDialog::CreateAction),
        "automation-new" => Some(DevDialog::AutomationNew),
        "create-board" => Some(DevDialog::CreateBoard),
        "create-team" => Some(DevDialog::CreateTeam),
        "join-team" => Some(DevDialog::JoinTeam(None)),
        "add-server" => Some(DevDialog::AddServer),
        _ => {
            if let Some(token) = spec.strip_prefix("join-team:") {
                let token = token.trim();
                return Some(DevDialog::JoinTeam(
                    (!token.is_empty()).then(|| token.to_string()),
                ));
            }
            if let Some(id) = spec.strip_prefix("action-editor:") {
                return Some(DevDialog::ActionEditor(id.to_string()));
            }
            if let Some(id) = spec.strip_prefix("automation-edit:") {
                return Some(DevDialog::AutomationEdit(id.to_string()));
            }
            if let Some(id) = spec.strip_prefix("device-settings:") {
                return Some(DevDialog::DeviceSettings(id.to_string()));
            }
            spec.strip_prefix("duplicate-picker:")
                .map(|id| DevDialog::DuplicatePicker(id.to_string()))
        }
    }
}

/// A [`DevDialog`] whose precondition has RESOLVED — the ids its opener takes
/// are snapshotted here so the delayed spawn never re-reads the store.
enum DevDialogTarget {
    CreateIssue { board_id: String },
    Search,
    StartCodingIssue { issue_id: String },
    StartCodingAction { team_id: String, action_id: String },
    StartCodingChat { team_id: String },
    CreateAction { team_id: String },
    ActionEditor { action_id: String },
    AutomationNew { team_id: String },
    AutomationEdit { automation_id: String },
    CreateBoard { team_id: String },
    CreateTeam,
    JoinTeam { token: Option<String> },
    AddServer,
    DeviceSettings { device_id: String },
    DuplicatePicker { issue_id: String },
}

fn open_dev_dialog(target: DevDialogTarget, window: &mut Window, cx: &mut App) {
    match target {
        DevDialogTarget::CreateIssue { board_id } => {
            crate::create_issue_dialog::open(window, cx, board_id)
        }
        DevDialogTarget::Search => crate::search_sheet::open_search(window, cx),
        DevDialogTarget::StartCodingIssue { issue_id } => {
            crate::start_coding_dialog::open_for_issue(window, cx, issue_id)
        }
        DevDialogTarget::StartCodingAction { team_id, action_id } => {
            crate::start_coding_dialog::open_for_action(window, cx, team_id, action_id)
        }
        DevDialogTarget::StartCodingChat { team_id } => {
            crate::start_coding_dialog::open_for_chat(window, cx, team_id)
        }
        DevDialogTarget::CreateAction { team_id } => {
            crate::create_action_dialog::open(window, cx, team_id)
        }
        DevDialogTarget::ActionEditor { action_id } => {
            crate::action_editor_dialog::open(window, cx, action_id)
        }
        DevDialogTarget::AutomationNew { team_id } => {
            crate::automation_dialog::open_new(window, cx, team_id)
        }
        DevDialogTarget::AutomationEdit { automation_id } => {
            crate::automation_dialog::open_edit(window, cx, automation_id)
        }
        DevDialogTarget::CreateBoard { team_id } => {
            crate::create_board_dialog::open(window, cx, team_id)
        }
        DevDialogTarget::CreateTeam => crate::create_team_dialog::open(window, cx),
        DevDialogTarget::JoinTeam { token } => crate::join_team::open(window, cx, token),
        DevDialogTarget::AddServer => crate::machines::open_add_server_dialog(window, cx),
        DevDialogTarget::DeviceSettings { device_id } => {
            crate::device_settings::open(window, cx, device_id)
        }
        DevDialogTarget::DuplicatePicker { issue_id } => {
            crate::issue_detail::open_duplicate_picker(issue_id, window, cx)
        }
    }
}

/// EXP-633: set once the requested dev dialog is actually UP (or once the
/// spec turned out to be unrecognised, so a typo can never hang the probe).
static DIALOG_OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// EXP-633: has the DEV-ONLY dialog hook finished its work? `true` when this
/// run asked for no dialog at all, otherwise `true` once the dialog opened.
/// The ready probe ([`crate::dev_ready`]) waits on this so a capture never
/// photographs the screen a beat before its overlay appears.
pub(crate) fn dev_dialog_settled() -> bool {
    dev_dialog_spec().is_none() || DIALOG_OPENED.load(std::sync::atomic::Ordering::SeqCst)
}

/// The DEV-ONLY spec this run asked for: `EXP_DEV_DIALOG`, or the legacy
/// `EXP_DEV_CREATE_DIALOG=1` kept working as an alias for `create-issue`.
fn dev_dialog_spec() -> Option<String> {
    if let Ok(spec) = std::env::var("EXP_DEV_DIALOG") {
        let spec = spec.trim();
        if !spec.is_empty() {
            return Some(spec.to_string());
        }
    }
    (std::env::var("EXP_DEV_CREATE_DIALOG").as_deref() == Ok("1"))
        .then(|| "create-issue".to_string())
}

impl ScreensPanel {
    /// Resolve `dialog`'s precondition against the CURRENT state. `None` =
    /// not ready yet (an unsynced row, no scope, still signing in) — the
    /// caller retries on the next render instead of opening the wrong thing.
    fn resolve_dev_dialog(&self, dialog: &DevDialog, cx: &App) -> Option<DevDialogTarget> {
        let store = Store::global(cx);
        let signed_in = matches!(store.session(cx), sync::SessionPhase::Synced { .. });
        Some(match dialog {
            DevDialog::CreateIssue => DevDialogTarget::CreateIssue {
                board_id: active_board_id(&self.nav, cx)?,
            },
            DevDialog::Search => {
                // `open_search` self-guards on both — gate here too so the
                // one-shot latch is never spent on a silent no-op.
                if !signed_in {
                    return None;
                }
                active_team_id(&self.nav, cx)?;
                DevDialogTarget::Search
            }
            DevDialog::StartCoding => DevDialogTarget::StartCodingIssue {
                issue_id: store
                    .collections()
                    .issues_in_board(&active_board_id(&self.nav, cx)?, cx)
                    .first()?
                    .id
                    .clone(),
            },
            DevDialog::StartCodingActions => DevDialogTarget::StartCodingAction {
                team_id: active_team_id(&self.nav, cx)?,
                action_id: api::actions::BUILTIN_FIX_CONFLICTS_ID.to_string(),
            },
            DevDialog::StartCodingChat => DevDialogTarget::StartCodingChat {
                team_id: active_team_id(&self.nav, cx)?,
            },
            DevDialog::CreateAction => DevDialogTarget::CreateAction {
                team_id: active_team_id(&self.nav, cx)?,
            },
            DevDialog::ActionEditor(action_id) => {
                store.collections().actions.read(cx).get(action_id)?;
                DevDialogTarget::ActionEditor {
                    action_id: action_id.clone(),
                }
            }
            DevDialog::AutomationNew => DevDialogTarget::AutomationNew {
                team_id: active_team_id(&self.nav, cx)?,
            },
            DevDialog::AutomationEdit(automation_id) => {
                store.collections().automations.read(cx).get(automation_id)?;
                DevDialogTarget::AutomationEdit {
                    automation_id: automation_id.clone(),
                }
            }
            DevDialog::CreateBoard => DevDialogTarget::CreateBoard {
                team_id: active_team_id(&self.nav, cx)?,
            },
            DevDialog::CreateTeam => {
                if !signed_in {
                    return None;
                }
                DevDialogTarget::CreateTeam
            }
            DevDialog::JoinTeam(token) => {
                if !signed_in {
                    return None;
                }
                DevDialogTarget::JoinTeam {
                    token: token.clone(),
                }
            }
            DevDialog::AddServer => {
                // The snippet names the account's instance — without one it
                // would photograph the hardcoded cloud fallback.
                crate::queries::active_account(cx)?;
                DevDialogTarget::AddServer
            }
            DevDialog::DeviceSettings(device_id) => {
                store.collections().devices.read(cx).get(device_id)?;
                DevDialogTarget::DeviceSettings {
                    device_id: device_id.clone(),
                }
            }
            DevDialog::DuplicatePicker(issue_id) => {
                store.collections().issues.read(cx).get(issue_id)?;
                DevDialogTarget::DuplicatePicker {
                    issue_id: issue_id.clone(),
                }
            }
        })
    }

    /// DEV-ONLY (§11.4 headless verification, EXP_DEV_* family): open the
    /// [`DevDialog`] this run asked for, exactly once, from the render path.
    /// Unset in normal runs.
    fn fire_dev_dialog(&self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        use std::sync::atomic::{AtomicBool, Ordering};
        static FIRED: AtomicBool = AtomicBool::new(false);
        if FIRED.load(Ordering::SeqCst) {
            return;
        }
        let Some(spec) = dev_dialog_spec() else {
            return;
        };
        let Some(dialog) = parse_dev_dialog(&spec) else {
            // A typo would otherwise photograph the bare screen with no hint
            // that no dialog was ever going to open. Once per run — this is
            // the render path. (A spec that parses but whose precondition is
            // still resolving stays SILENT: that one is expected per frame.)
            static WARNED: AtomicBool = AtomicBool::new(false);
            if !WARNED.swap(true, Ordering::SeqCst) {
                eprintln!(
                    "[exp-desktop] dev: EXP_DEV_DIALOG={spec} unrecognized — \
                     expected {DEV_DIALOG_SPECS}"
                );
            }
            // Nothing will ever open — release the ready probe (EXP-633).
            DIALOG_OPENED.store(true, Ordering::SeqCst);
            return;
        };
        let Some(target) = self.resolve_dev_dialog(&dialog, cx) else {
            return;
        };
        if FIRED.swap(true, Ordering::SeqCst) {
            return;
        }
        cx.spawn_in(window, async move |_this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1500))
                .await;
            let opened = cx.update(|window, cx| open_dev_dialog(target, window, cx));
            // EXP-633: the ready probe waits for the overlay, not the timer.
            DIALOG_OPENED.store(true, Ordering::SeqCst);
            eprintln!("[exp-desktop] dev: EXP_DEV_DIALOG={spec} fired ({opened:?})");
        })
        .detach();
    }
}

impl Render for ScreensPanel {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let screen = resolved_screen(&self.nav, cx);

        self.fire_dev_dialog(window, cx);

        let content = match &screen {
            Some(Screen::IssueDetail { .. }) => self.issue_detail.clone().into_any_element(),
            Some(Screen::Settings) => self.settings.clone().into_any_element(),
            Some(Screen::SupportThread { .. }) => {
                self.support_thread.clone().into_any_element()
            }
            Some(Screen::PrDiff { .. }) => self.pr_diff.clone().into_any_element(),
            // EXP-746: built by `sync_tabs` on activation — the fallback only
            // shows for the frame between a navigation and that observer.
            Some(Screen::Session { session_id }) => self
                .sessions
                .get(session_id)
                .cloned()
                .map(gpui::IntoElement::into_any_element)
                .unwrap_or_else(|| self.render_syncing(cx)),
            // EXP-769: the terminal fills the center — its grid, exit strip
            // and Latest-changes bar are the session bar host's (the entity
            // that owns the manager and the dock-scoped key bindings).
            Some(Screen::Terminal { tab }) => {
                match crate::session_bar::host_for_window(window, cx) {
                    Some(host) => {
                        let tab = *tab;
                        host.update(cx, |host, cx| host.render_terminal_screen(tab, window, cx))
                    }
                    None => self.render_syncing(cx),
                }
            }
            Some(Screen::Devices) => self.devices.clone().into_any_element(),
            Some(Screen::Actions) => self.actions.clone().into_any_element(),
            Some(Screen::Automations) => self.automations.clone().into_any_element(),
            Some(Screen::Chat) => self.chat.clone().into_any_element(),
            Some(Screen::Reviews) => self.reviews.clone().into_any_element(),
            Some(Screen::GettingStarted { .. }) => {
                self.getting_started.clone().into_any_element()
            }
            // EXP-288: no tab selected — the active TOOL owns the center.
            // Source Control shows its diff (following the History
            // selection), Files the read-only viewer (its Idle phase covers
            // "no file selected"); everything else keeps the empty state.
            None => match self.rail.read(cx).tool() {
                ToolWindow::SourceControl => self.source_control.clone().into_any_element(),
                ToolWindow::Files => self.file_viewer.clone().into_any_element(),
                // EXP-818: the Agent page with no session selected IS the
                // chat prompt.
                ToolWindow::Sessions => self.chat.clone().into_any_element(),
                _ if !shapes_ready(cx) => self.render_syncing(cx),
                _ => self.render_empty(cx),
            },
        };

        // EXP-277: the tab strip lives in the titlebar (AppTitleBar) whenever
        // the window paints its own chrome; under Linux server-side
        // decorations the titlebar is hidden, so the strip renders here in
        // its legacy in-panel position (with its own EXP-288 divider — the
        // titlebar carries it otherwise).
        let fallback_strip = (self.top_tab_count() > 0
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
    use super::{
        lead_reserve_rems, neighbor_in_strip, partition_tabs, resume_swaps, takes_over_tab,
        ChipLead,
    };

    /// EXP-781: closing a tab activates its own strip's neighbour and NOTHING
    /// else. The last session tab closing leaves the center empty rather than
    /// pulling an issue tab into view.
    #[test]
    fn a_close_never_activates_the_other_strip() {
        // [issue, issue, session] — the session closes, no session is left.
        assert_eq!(neighbor_in_strip(&[false, false], 2, true), None);
        // [issue] — the last issue closes with a session still open.
        assert_eq!(neighbor_in_strip(&[true], 0, false), None);

        // Within the strip: the tab that shifted into the closed slot wins.
        // [issue, session, session] after closing the middle session.
        assert_eq!(neighbor_in_strip(&[false, true, true], 1, true), Some(1));
        // Closing the LAST session falls back leftwards, skipping the issue.
        assert_eq!(neighbor_in_strip(&[false, true], 2, true), Some(1));
        // The same for the top strip, skipping the session between them.
        assert_eq!(neighbor_in_strip(&[false, true], 1, false), Some(0));
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    fn row(id: &str, resumed_from: Option<&str>) -> (String, Option<String>) {
        (id.to_string(), resumed_from.map(str::to_string))
    }

    /// EXP-746: a resumed run takes over the tab of the run it continues —
    /// that is the whole point of keying the screen on the row id. Rows that
    /// resume something not open, or whose own tab is already up, change
    /// nothing.
    #[test]
    fn resume_swap_targets_the_open_tab() {
        let open = ids(&["s1", "s2"]);
        let rows = vec![
            row("s1", None),
            row("s2", None),
            row("s3", Some("s1")),
            // Resumes a run nobody has open.
            row("s4", Some("s9")),
        ];
        assert_eq!(
            resume_swaps(&open, &rows),
            vec![("s1".to_string(), "s3".to_string())]
        );

        // Both rows already have tabs: either the swap already happened at
        // open time, or the user opened the continuation itself (Devices →
        // Running) beside the run it continues. Neither is ours to undo.
        assert!(resume_swaps(&ids(&["s1", "s3"]), &rows).is_empty());
        // No session tabs at all.
        assert!(resume_swaps(&[], &rows).is_empty());
    }

    /// EXP-746 regression: the swap must not depend on which side sees the
    /// resume first. A desktop-initiated resume reaches
    /// `session_screen::open_session` — and through it
    /// `ScreensPanel::take_over_session_tab`, i.e. [`takes_over_tab`] — a
    /// round trip BEFORE the resumed row syncs, so both orderings have to end
    /// with exactly one tab. Keying the swap on the synced row alone left the
    /// continued run's transcript sitting beside its live continuation on the
    /// common (local) path.
    #[test]
    fn a_resume_swaps_once_whichever_side_sees_it_first() {
        let rows = vec![row("s2", Some("s1"))];

        // The local start wins the race: the tab is taken over at open
        // time, and the row's echo then finds nothing left to do.
        assert!(takes_over_tab(&ids(&["s1"]), "s1", "s2"));
        assert!(resume_swaps(&ids(&["s2"]), &rows).is_empty());

        // The echo wins: the sync swaps, and the open-time call that
        // follows finds the tab already renamed.
        assert_eq!(
            resume_swaps(&ids(&["s1"]), &rows),
            vec![("s1".to_string(), "s2".to_string())]
        );
        assert!(!takes_over_tab(&ids(&["s2"]), "s1", "s2"));

        // Nothing open for the run being continued (Past → Resume without
        // opening it first, the usual case): a plain new tab either way.
        assert!(!takes_over_tab(&ids(&["s9"]), "s1", "s2"));
        assert!(resume_swaps(&ids(&["s9"]), &rows).is_empty());
    }

    /// A chain (`s3` resumed `s1`, `s5` resumed `s3`) converges one step per
    /// tick, and two rows claiming ONE tab resolve deterministically — the
    /// row ids arrive in no useful order.
    #[test]
    fn resume_swaps_take_one_step_and_stay_deterministic() {
        let rows = vec![row("s5", Some("s3")), row("s3", Some("s1"))];
        assert_eq!(
            resume_swaps(&ids(&["s1"]), &rows),
            vec![("s1".to_string(), "s3".to_string())]
        );
        // After that swap the next tick moves the same tab on to `s5`.
        assert_eq!(
            resume_swaps(&ids(&["s3"]), &rows),
            vec![("s3".to_string(), "s5".to_string())]
        );
        let contested = vec![row("s9", Some("s1")), row("s2", Some("s1"))];
        assert_eq!(
            resume_swaps(&ids(&["s1"]), &contested),
            vec![("s1".to_string(), "s2".to_string())]
        );
    }

    /// The strip's `gap_1` and the "+N" button at the app's rem
    /// ([`theme::FONT_SIZE_PX`]). Every assertion below is expressed in terms
    /// of these two, so the exact values only have to be plausible.
    const GAP: f32 = 3.5;
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

    /// EXP-746: the width reserve mirrors what `surface::rich_tab` actually
    /// paints. A dot is half an icon wide; reserving the icon for it inflates
    /// every session chip and folds tabs away with room still to their right.
    #[test]
    fn lead_reserve_matches_the_rendered_lead() {
        assert_eq!(lead_reserve_rems(&ChipLead::None), 0.);
        assert_eq!(
            lead_reserve_rems(&ChipLead::Dot(gpui::red())),
            0.375,
            "`size_1p5`"
        );
        assert_eq!(
            lead_reserve_rems(&ChipLead::Status(domain::statuses::constructed_default(
                domain::enums::IssueStatus::InProgress
            ))),
            0.75,
            "`Icon::xsmall()` = `size_3`"
        );
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
    /// machine rows, the action row list (EXP-618 replaced the old wrapping
    /// card grid), all with real text inside the page's capped `mx_auto`
    /// column and scroll pane — must resolve its column to exactly
    /// `min(1024, panel)` at EVERY panel width, the machines section and its
    /// band must span that column, and the list must never run past the
    /// window's right edge. Two gates: a width sweep under the production
    /// `h_resizable` split (settled frames stay healthy), then the stray
    /// fit-content pass modeled directly — the one place the un-pinned tree
    /// demonstrably breaks (clean `cx.draw` frames alone never reproduce the
    /// live app's between-frame passes).
    ///
    /// The sweep runs to 3000px because the EXP-508 failure only started at
    /// ~1940px (the EXP-499 sweep stopped at 1700 and missed it): a `w_full`
    /// PERCENT child of the centered column resolves against the UNCLAMPED
    /// ancestor available width, so content resolved its max-content line
    /// and the machines section shrink-wrapped — the EXP-436 block-hop leak.
    /// The probe mirrors the fixed page: the column's direct children carry
    /// NO `w_full` and are sized by flex-col stretch; percent widths below
    /// those stretch-sized parents (the band, the machine and action rows)
    /// resolve fine and stay covered.
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
                // machines.rs), with production-like text. The action rows
                // mirror `render_action_row` (EXP-618): full-width, a
                // truncating middle column, a nowrap trailing control.
                let action_row = |name: &'static str, blurb: &'static str| {
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_2()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap_0p5()
                                .child(
                                    div()
                                        .min_w_0()
                                        .text_sm()
                                        .truncate()
                                        .child(SharedString::from(name)),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .text_xs()
                                        .line_clamp(2)
                                        .child(SharedString::from(blurb)),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .whitespace_nowrap()
                                .child(SharedString::from("Run")),
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
                let list = v_flex()
                    .min_w_0()
                    .debug_selector(|| "probe-list".into())
                    .child(action_row(
                        "Fix merge conflicts",
                        "Pick a conflicted pull request and let your agent rebase, \
                         resolve, and merge it",
                    ))
                    .child(action_row(
                        "Triage code review findings",
                        "Re-check the latest code review board against today's code, \
                         cancel stale findings, and file one grouped issue",
                    ))
                    .child(action_row(
                        "Release staging",
                        "Copy the prod DB over staging, refresh staging web + steer \
                         relay, and push the staging iOS/Android build",
                    ))
                    .child(action_row(
                        "Release prod",
                        "Review the unreleased commit wave, then ship production — \
                         prod web, relays, marketing, desktop",
                    ))
                    .child(action_row(
                        "Release all",
                        "Run the full release train in one go: the staging refresh \
                         first, then the prod train — one combined wave, one report",
                    ))
                    .child(action_row(
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
                    .child(v_flex().min_w_0().gap_2().child(list));
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
            // The literal EXP-499 symptom: the actions content running off
            // the window's right edge.
            let list = cx
                .debug_bounds("probe-list")
                .unwrap_or_else(|| panic!("list bounds missing at width {width}"));
            let right = f32::from(list.origin.x) + f32::from(list.size.width);
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
        // band shrink-wraps and the list resolves its max-content line. The
        // recorded-width pin must make even this pass resolve the page like
        // the real panel slot.
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
        let list = cx
            .debug_bounds("probe-list")
            .expect("list bounds in the fit-content pass");
        let list_right = f32::from(list.origin.x) + f32::from(list.size.width);
        assert!(
            list_right <= recorded + 1.5,
            "fit-content pass ran the action list past the panel edge: \
             {list_right} > {recorded}"
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
