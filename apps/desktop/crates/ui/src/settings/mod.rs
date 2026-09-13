//! The ONE settings screen (masterplan-v3 §4.2 "Settings", §7.9 integrations
//! surface; EXP-238 folded the old Account screen in).
//!
//! Web parity targets: the `routes/t/$teamSlug/settings/` pages and
//! their `components/team/*-section.tsx` + `components/account/*` cards. The
//! screen mirrors the web's grouped master-detail layout (EXP-146): a fixed
//! left nav with the groups — **Team** (General, Members, Labels, Statuses,
//! Storage — the EXP-297 owner-only attachment manager), **Boards**
//! (one entry PER board + New board + Repositories — EXP-288 flattened the
//! old flat Boards list into per-board detail pages), **Features**
//! (Feedback widget, Helpdesk — EXP-771), the desktop-only
//! **This device** group (Tools, Agents, Worktrees), and
//! **Personal** (Account, Notifications, Security, About — EXP-238); the
//! detail column shows ONE selected pane with the web's `isOwner &&` gating;
//! each pane mirrors its web card field-for-field — and since EXP-771 sits
//! under the web route's OWN header ("Settings" + "Manage … and your
//! account") in a centered `max-w-4xl` column.
//!
//! Navigation INTO these screens: the rail's gear dispatches `OpenSettings`
//! (see `sidebar.rs` + `navigation::init`); this module only provides the
//! screens.
//!
//! Billing (EXP-288): General carries a READ-ONLY plan/usage summary with a
//! "Manage on the web" hand-off — still no in-app purchase/pricing UI and no
//! admin surface. Plan-cap failures (HTTP 412 from `lib/billing.ts`) render
//! as a neutral "Upgrade on the web" notice. The GitHub App *install* is a
//! browser hand-off (§7.9).
//!
//! EXP-771: the widget and helpdesk settings are NO LONGER web-only. The
//! Features group carries a READ-ONLY widget list with a "Manage on the web"
//! hand-off (authoring a config stays a browser flow — it needs the embed
//! snippet, the domain allowlist and the theme editor) plus the team's
//! helpdesk switch, which is a plain `teams.update` this app owns outright.

mod about;
mod account;
mod add_repository_dialog;
mod agents;
mod api_keys;
pub(crate) mod doctor_section;
mod helpdesk;
mod labels;
// EXP-792: `pub(crate)` because the READINESS vocabulary lives here —
// `launch_options`' multiselect greys a row with the same rule the pane's
// status line reads, and start-coding's blocker with the same sentences.
pub(crate) mod mcp_servers;
// EXP-810: the pane's own add/edit form — a WINDOW, not an alert (the form
// re-renders on its own state).
mod mcp_server_dialog;
mod statuses;
mod local_repos;
mod members;
mod archived_boards;
mod notifications_prefs;
mod board_detail;
mod repositories;
mod storage;
mod team_general;
mod tools;
mod widget;

/// EXP-851: the left column's BACK row — the settings nav's header row,
/// shared with the `ListNav` (`sidebar::ListPanel::nav_back_row`) so the two
/// occupants of that column wear the same affordance. Returns the row WITHOUT
/// a click handler: where back goes is the caller's business.
pub(crate) fn nav_back_row(
    id: &'static str,
    label: impl Into<gpui::SharedString>,
    cx: &App,
) -> gpui::Stateful<gpui::Div> {
    h_flex()
        .id(id)
        .mx_2()
        .px_2()
        .py_1()
        .gap_2()
        .items_center()
        .rounded(cx.theme().radius)
        .cursor_pointer()
        .flex_shrink_0()
        .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
        // EXP-862: the BARE 16px chevron (`controls::back_glyph`) — the whole
        // row is the target, so a nested button would be a second hit target
        // for the same action.
        .child(crate::controls::back_glyph())
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(label.into()),
        )
}

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _,
};
use sync::Store;

use crate::navigation::{
    active_team_id, go_back, nav_for_window, navigate, set_screen, Navigation, Screen,
};
use crate::icons::registry;
use crate::queries;
use crate::sidebar::{rail_shared_for_window, select_settings_section, RailShared};

use about::AboutPane;
use account::AccountPane;
use api_keys::ApiKeysPane;
use helpdesk::HelpdeskPane;
use labels::LabelsPane;
use mcp_servers::McpServersPane;
use widget::WidgetPane;
use statuses::StatusesPane;
use local_repos::LocalReposPane;
use members::MembersPane;
use agents::AgentsPane;
use archived_boards::ArchivedBoardsPane;
use board_detail::BoardDetailPane;
use notifications_prefs::NotificationsPrefsPane;
use repositories::RepositoriesPane;
use storage::StoragePane;
use team_general::GeneralPane;
use tools::ToolsPane;

// ---------------------------------------------------------------------------
// Section nav model (EXP-146 grouped master-detail)
// ---------------------------------------------------------------------------

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum SettingsSection {
    General,
    Members,
    Labels,
    /// EXP-314 per-team custom issue statuses. Member-editable like Labels
    /// (the server's writes are `mutate_resources`, NOT owner-only).
    Statuses,
    /// EXP-297 team file manager: every attachment via
    /// `attachments.listForTeam` + per-file delete + the unreferenced-image
    /// sweep. Owner-only, like the router behind it.
    Storage,
    /// One board's detail settings page (EXP-288 — the Boards group lists
    /// every board as its own nav entry; the payload is the board id).
    Board(String),
    /// EXP-500: the team's archived boards, with unarchive. Owner-only, and
    /// necessarily its own pane: an archived board is excluded from the boards
    /// shape, so it has no per-board nav row to hang off and can only be read
    /// over tRPC. Web keeps the equivalent as a card on its single Boards
    /// settings page, which desktop flattened into per-board panes (EXP-288).
    ArchivedBoards,
    Repositories,
    /// EXP-771: the team's feedback widgets — a READ-ONLY list (name,
    /// submission count, disabled state) plus the "Manage on the web"
    /// hand-off. Owner-only, like the web's `canManageWidgets` gate and the
    /// `widgets.list` router behind it.
    Widget,
    /// EXP-771: the team's helpdesk switch — the web widget page's second
    /// card, split out into its own pane because the desktop nav is one
    /// section per page. Owner-only.
    Helpdesk,
    /// EXP-792/EXP-807: the team's MCP servers — the registry (non-secret
    /// config only) plus THIS machine's readiness and the device-side
    /// sign-in / typed-secret actions. Member-visible: every member reads
    /// the list and holds their OWN credentials; the owner-only writes are
    /// gated inside the pane, like the router behind it.
    McpServers,
    /// This-device tools (EXP-288: renamed from "Coding" — repos root,
    /// branch prefix, terminal shell).
    Tools,
    /// The per-agent launcher settings + doctor (EXP-288: split out of the
    /// old Coding pane).
    Agents,
    LocalRepos,
    /// EXP-238: identity + timezone (the old Account screen's head — the
    /// notification prefs split into [`SettingsSection::Notifications`]).
    /// Never gated.
    Account,
    /// EXP-238: email/push notification prefs as their own section (web
    /// parity: `settings/notifications`). Never gated.
    Notifications,
    /// EXP-238: personal `expu_` API keys — list/mint/revoke. EXP-862
    /// relabelled the nav entry "Security" (web `/settings/security`); the
    /// variant keeps its name, and so does the `settings-api` icon.
    ApiKeys,
    /// EXP-262: version + third-party licence notices. Since EXP-238 an
    /// ordinary Personal-group item in [`NAV_GROUPS`] — it sits LAST, so the
    /// fallback scan (first visible item) still never lands on it. Never
    /// gated.
    About,
}

struct NavItem {
    label: &'static str,
    section: SettingsSection,
}

struct NavGroup {
    label: &'static str,
    items: &'static [NavItem],
}

/// The STATIC nav skeleton — the web's `SETTINGS_NAV` groups minus the
/// web-only Billing item, plus the desktop-only "This device" group.
/// EXP-238 appended the Personal group (Account, Notifications, Security,
/// About) as ordinary items — web parity again, and last so the fallback
/// scan below never lands on a personal pane. The Boards group's per-board
/// rows + "New board" are injected dynamically at render (EXP-288); order
/// defines both the nav and the fallback scan (first visible item).
const NAV_GROUPS: &[NavGroup] = &[
    NavGroup {
        label: "Team",
        items: &[
            NavItem {
                label: "General",
                section: SettingsSection::General,
            },
            NavItem {
                label: "Members",
                section: SettingsSection::Members,
            },
            NavItem {
                label: "Labels",
                section: SettingsSection::Labels,
            },
            // EXP-314: right after Labels — the two team vocabularies sit
            // together, and both are member-editable. EXP-771: the label is
            // the web's ("Statuses"), not the longer desktop-only one.
            NavItem {
                label: "Statuses",
                section: SettingsSection::Statuses,
            },
            // EXP-297: after Labels — the web nav's Team group order minus
            // the web-only Plan & Billing entry between them.
            NavItem {
                label: "Storage",
                section: SettingsSection::Storage,
            },
        ],
    },
    NavGroup {
        label: "Boards",
        items: &[
            NavItem {
                label: "Archived boards",
                section: SettingsSection::ArchivedBoards,
            },
            NavItem {
                label: "Repositories",
                section: SettingsSection::Repositories,
            },
        ],
    },
    // EXP-771: the web's Features group, verbatim — plus Helpdesk, which the
    // web keeps as the widget page's second card and the desktop nav (one
    // section per page) gives its own row.
    NavGroup {
        label: "Features",
        items: &[
            NavItem {
                label: "Feedback widget",
                section: SettingsSection::Widget,
            },
            NavItem {
                label: "Helpdesk",
                section: SettingsSection::Helpdesk,
            },
            // EXP-792: member-visible, unlike its two neighbours — every
            // member reads the registry and signs in on their own machines
            // (the web nav's `visible: () => true`).
            NavItem {
                label: "MCP servers",
                section: SettingsSection::McpServers,
            },
        ],
    },
    NavGroup {
        label: "This device",
        items: &[
            NavItem {
                label: "Tools",
                section: SettingsSection::Tools,
            },
            NavItem {
                label: "Agents",
                section: SettingsSection::Agents,
            },
            NavItem {
                label: "Worktrees",
                section: SettingsSection::LocalRepos,
            },
        ],
    },
    // EXP-238: the Personal group is ordinary nav now — the web merged the
    // account pages into its settings nav the same way, so the two screens
    // stay row-for-row parallel. Last on purpose: the fallback scan must
    // keep landing on General/Members.
    NavGroup {
        label: "Personal",
        items: &[
            NavItem {
                label: "Account",
                section: SettingsSection::Account,
            },
            NavItem {
                label: "Notifications",
                section: SettingsSection::Notifications,
            },
            NavItem {
                label: "Security",
                section: SettingsSection::ApiKeys,
            },
            NavItem {
                label: "About",
                section: SettingsSection::About,
            },
        ],
    },
];

/// EXP-288: every nav entry carries an icon. Board rows use the tinted board
/// glyph instead (`icons::board_icon`), so they don't route through here.
///
/// EXP-317: every arm resolves through the shared registry's `settings-*`
/// concepts — the same ones the web nav renders (`settings/-shared.tsx`), so
/// the two settings screens can no longer drift apart. Locked by the web
/// `lib/icons.test.ts` settings-nav parity check.
fn section_icon(section: &SettingsSection) -> Icon {
    match section {
        SettingsSection::General => Icon::from(registry::SETTINGS_GENERAL),
        SettingsSection::Members => Icon::from(registry::SETTINGS_MEMBERS),
        SettingsSection::Labels => Icon::from(registry::SETTINGS_LABELS),
        SettingsSection::Statuses => Icon::from(registry::SETTINGS_STATUSES),
        SettingsSection::Storage => Icon::from(registry::SETTINGS_STORAGE),
        SettingsSection::ArchivedBoards => Icon::from(registry::UI_ARCHIVE),
        SettingsSection::Board(_) => Icon::from(registry::SETTINGS_BOARDS),
        SettingsSection::Repositories => Icon::from(registry::SETTINGS_REPOSITORIES),
        SettingsSection::Widget => Icon::from(registry::SETTINGS_WIDGET),
        SettingsSection::Helpdesk => Icon::from(registry::SETTINGS_HELPDESK),
        SettingsSection::McpServers => Icon::from(registry::SETTINGS_MCP),
        SettingsSection::Tools => Icon::from(registry::SETTINGS_TOOLS),
        SettingsSection::Agents => Icon::from(registry::SETTINGS_AGENTS),
        SettingsSection::LocalRepos => Icon::from(registry::SETTINGS_LOCAL_REPOS),
        SettingsSection::Account => Icon::from(registry::SETTINGS_ACCOUNT),
        SettingsSection::Notifications => Icon::from(registry::SETTINGS_NOTIFICATIONS),
        SettingsSection::ApiKeys => Icon::from(registry::SETTINGS_API),
        SettingsSection::About => Icon::from(registry::SETTINGS_ABOUT),
    }
}

/// Web nav `visible` gating: General/board pages are owner-only.
/// Repositories is member-visible since EXP-557 (per-user repo sharing:
/// every member connects their own GitHub and shares repos there).
/// EXP-771: the Features group is owner-only — the web's `canManageWidgets`
/// is `isOwner`, and `widgets.list`/`teams.update` are owner-gated servers.
fn section_visible(section: &SettingsSection, owner: bool) -> bool {
    match section {
        SettingsSection::General
        | SettingsSection::Storage
        | SettingsSection::Board(_)
        | SettingsSection::ArchivedBoards
        | SettingsSection::Widget
        | SettingsSection::Helpdesk => owner,
        _ => true,
    }
}

/// The synced board ids of the active team, used to clamp a stale
/// `Board(id)` selection. `None` while the boards collection is still
/// loading — treat any selection as valid then so a restored selection
/// doesn't bounce.
fn valid_board_ids(
    cx: &App,
    nav: &Entity<Navigation>,
) -> Option<std::collections::HashSet<String>> {
    let collections = Store::global(cx).collections();
    if !collections.boards.read(cx).is_ready() {
        return None;
    }
    let Some(team_id) = active_team_id(nav, cx) else {
        return Some(Default::default());
    };
    Some(
        collections
            .boards_in_team(&team_id, cx)
            .into_iter()
            .map(|board| board.id)
            .collect(),
    )
}

/// The selected section, clamped to what's visible. Clamped at render time —
/// never mutated — so a membership change that hides the selection falls back
/// (to Members, the first never-gated item) and restores it if ownership
/// returns. A selected `Board(id)` additionally requires the board to still
/// exist in the active team (`board_ok`) — a trashed board falls back.
fn effective_selection(
    selected: SettingsSection,
    owner: bool,
    board_ok: impl Fn(&str) -> bool,
) -> SettingsSection {
    let visible = section_visible(&selected, owner)
        && match &selected {
            SettingsSection::Board(id) => board_ok(id),
            _ => true,
        };
    if visible {
        return selected;
    }
    NAV_GROUPS
        .iter()
        .flat_map(|group| group.items)
        .map(|item| item.section.clone())
        .find(|section| section_visible(section, owner))
        .expect("Members is never gated")
}

// ---------------------------------------------------------------------------
// Team settings shell
// ---------------------------------------------------------------------------

/// The team-settings screen (`Screen::Settings`) — the web settings
/// pages' grouped master-detail layout (billing/widget/danger-zone
/// skipped: web-only, §4.9).
pub struct SettingsView {
    nav: Entity<Navigation>,
    general: Entity<GeneralPane>,
    members: Entity<MembersPane>,
    labels: Entity<LabelsPane>,
    /// EXP-314 per-team issue statuses — un-gated (member-editable).
    statuses: Entity<StatusesPane>,
    /// EXP-297 owner-only attachment manager (fetch-on-open server read).
    storage: Entity<StoragePane>,
    /// EXP-500 owner-only archived-boards list (fetch-on-open server read —
    /// archived boards are not synced).
    archived_boards: Entity<ArchivedBoardsPane>,
    /// EXP-288: ONE per-board detail pane — it reads the selected `Board(id)`
    /// from the shared selection itself and re-points at board switches.
    board_detail: Entity<BoardDetailPane>,
    repositories: Entity<RepositoriesPane>,
    /// EXP-771 owner-only widget list (fetch-on-open server read —
    /// `widget_configs` is never synced).
    widget: Entity<WidgetPane>,
    /// EXP-771 owner-only helpdesk switch (the team row's synced flag).
    helpdesk: Entity<HelpdeskPane>,
    /// EXP-792 team MCP servers (fetch-on-open server read — `mcp_servers`
    /// is never synced), member-visible.
    mcp_servers: Entity<McpServersPane>,
    /// This-device tools (EXP-288: repos root, branch prefix, terminal
    /// shell) — local per-install state, so NOT owner-gated.
    tools: Entity<ToolsPane>,
    /// Per-agent launcher settings + doctor (EXP-288: split out of Tools).
    agents: Entity<AgentsPane>,
    /// §4.7 desktop-only Worktrees section (clone disk usage +
    /// prune/remove) — local per-install state, un-gated.
    local_repos: Entity<LocalReposPane>,
    /// EXP-238: identity + timezone (the old Account screen, folded in).
    account: Entity<AccountPane>,
    /// EXP-238: email/push notification prefs — their own section now.
    notifications: Entity<NotificationsPrefsPane>,
    /// EXP-238: personal `expu_` API keys (fetch-on-open server read).
    api_keys: Entity<ApiKeysPane>,
    /// EXP-262: version + third-party licence notices (stateless, un-gated).
    about: Entity<AboutPane>,
    /// EXP-771: the ONE scroll offset of the detail column — the header +
    /// pane share a scroll region now, so the handle lives here and resets to
    /// the top on every section switch (the web's route change does the same).
    scroll: gpui::ScrollHandle,
    /// The section shown at the previous render — a transition INTO one of
    /// the Personal server-read panes drops its cache so it refetches
    /// (EXP-369 semantics, re-homed from the old Account screen).
    last_section: Option<SettingsSection>,
    /// EXP-282: the nav selection lives on the window's [`RailShared`] now —
    /// the nav column renders OUTSIDE this view (it replaces the tool column
    /// while settings are up), so both must read one value. Clamped through
    /// `effective_selection` at render time so gated sections never show for
    /// non-owners.
    shared: Entity<RailShared>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let general = cx.new(|cx| GeneralPane::new(nav.clone(), window, cx));
        let members = cx.new(|cx| MembersPane::new(nav.clone(), window, cx));
        let labels = cx.new(|cx| LabelsPane::new(nav.clone(), window, cx));
        let statuses = cx.new(|cx| StatusesPane::new(nav.clone(), window, cx));
        let storage = cx.new(|cx| StoragePane::new(nav.clone(), cx));
        let archived_boards = cx.new(|cx| ArchivedBoardsPane::new(nav.clone(), cx));
        let board_detail =
            cx.new(|cx| BoardDetailPane::new(nav.clone(), shared.clone(), window, cx));
        let repositories = cx.new(|cx| RepositoriesPane::new(nav.clone(), window, cx));
        let widget = cx.new(|cx| WidgetPane::new(nav.clone(), cx));
        let helpdesk = cx.new(|cx| HelpdeskPane::new(nav.clone(), cx));
        let mcp_servers = cx.new(|cx| McpServersPane::new(nav.clone(), window, cx));
        let tools = cx.new(|cx| ToolsPane::new(window, cx));
        let agents = cx.new(|cx| AgentsPane::new(window, cx));
        let local_repos = cx.new(LocalReposPane::new);
        let account = cx.new(|cx| AccountPane::new(window, cx));
        let notifications = cx.new(NotificationsPrefsPane::new);
        let api_keys = cx.new(|cx| ApiKeysPane::new(window, cx));
        let about = cx.new(|_| AboutPane::new());

        // The section nav + header depend on role (owner gating) —
        // re-render when membership/team data moves.
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // EXP-282: the nav column mutates the shared selection.
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            // EXP-288: a trashed board clamps a selected `Board(id)` back.
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
        ];

        Self {
            nav,
            general,
            members,
            labels,
            statuses,
            storage,
            archived_boards,
            board_detail,
            repositories,
            widget,
            helpdesk,
            mcp_servers,
            tools,
            agents,
            local_repos,
            account,
            notifications,
            api_keys,
            about,
            scroll: gpui::ScrollHandle::new(),
            last_section: None,
            shared,
            _subscriptions: subscriptions,
        }
    }

    /// EXP-369 (re-homed by EXP-238): the Personal panes hold server-only
    /// reads (timezone, email prefs, API keys) that would otherwise show the
    /// first visit's snapshot forever — every transition INTO the settings
    /// screen drops their caches; each pane refetches on its next render.
    pub fn mark_personal_stale(&mut self, cx: &mut gpui::Context<Self>) {
        self.account.update(cx, |pane, cx| pane.mark_stale(cx));
        self.notifications.update(cx, |pane, cx| pane.mark_stale(cx));
        self.api_keys.update(cx, |pane, cx| pane.mark_stale(cx));
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let owner = active_team_id(&self.nav, cx)
            .map(|ws| is_owner(cx, &ws))
            .unwrap_or(false);
        // Web settings layout route (EXP-146): grouped left nav + one
        // selected section pane in the detail column. EXP-282: the nav is
        // the window's left column now ([`SettingsNavPanel`]) — this view is
        // the detail column alone.
        let board_ids = valid_board_ids(cx, &self.nav);
        let effective = effective_selection(
            self.shared.read(cx).settings_section(),
            owner,
            |id| board_ids.as_ref().is_none_or(|ids| ids.contains(id)),
        );

        // A switch INTO a server-read pane refetches it (the screen-level
        // staleness only fires on Settings entry, not on section clicks
        // within it).
        if self.last_section.as_ref() != Some(&effective) {
            match &effective {
                SettingsSection::Account => {
                    self.account.update(cx, |pane, cx| pane.mark_stale(cx))
                }
                SettingsSection::Notifications => self
                    .notifications
                    .update(cx, |pane, cx| pane.mark_stale(cx)),
                SettingsSection::ApiKeys => {
                    self.api_keys.update(cx, |pane, cx| pane.mark_stale(cx))
                }
                // The widget list is tRPC too, and its one affordance sends
                // the user to the web to change it — so coming back must see
                // the change.
                SettingsSection::Widget => {
                    self.widget.update(cx, |pane, cx| pane.mark_stale(cx))
                }
                // Same reason, plus one of its own: the readiness line is
                // computed from THIS machine's store, and a sign-in run from
                // the CLI (or another window) has to show on re-entry.
                SettingsSection::McpServers => {
                    self.mcp_servers.update(cx, |pane, cx| pane.mark_stale(cx))
                }
                _ => {}
            }
            // EXP-771: one scroll region for header + pane, so a section
            // switch has to rewind it — the web's route change lands at the
            // top of the page, never mid-list.
            self.scroll.set_offset(gpui::point(px(0.), px(0.)));
            self.last_section = Some(effective.clone());
        }

        let pane: gpui::AnyElement = match &effective {
            SettingsSection::General => self.general.clone().into_any_element(),
            SettingsSection::Members => self.members.clone().into_any_element(),
            SettingsSection::Labels => self.labels.clone().into_any_element(),
            SettingsSection::Statuses => self.statuses.clone().into_any_element(),
            SettingsSection::Storage => self.storage.clone().into_any_element(),
            SettingsSection::ArchivedBoards => {
                self.archived_boards.clone().into_any_element()
            }
            // The pane reads the selected board id from the shared selection
            // itself (it needs to flush a pending rename on board switches).
            SettingsSection::Board(_) => self.board_detail.clone().into_any_element(),
            SettingsSection::Repositories => self.repositories.clone().into_any_element(),
            SettingsSection::Widget => self.widget.clone().into_any_element(),
            SettingsSection::Helpdesk => self.helpdesk.clone().into_any_element(),
            SettingsSection::McpServers => self.mcp_servers.clone().into_any_element(),
            SettingsSection::Tools => self.tools.clone().into_any_element(),
            SettingsSection::Agents => self.agents.clone().into_any_element(),
            SettingsSection::LocalRepos => self.local_repos.clone().into_any_element(),
            SettingsSection::Account => self.account.clone().into_any_element(),
            SettingsSection::Notifications => self.notifications.clone().into_any_element(),
            SettingsSection::ApiKeys => self.api_keys.clone().into_any_element(),
            SettingsSection::About => self.about.clone().into_any_element(),
        };

        // EXP-771: the web settings route's own chrome, on EVERY pane —
        // "Settings" over "Manage {team} and your account", a hairline, then
        // the pane, all in one centered `max-w-4xl` column.
        //
        // This replaces the EXP-277 "no screen header" rule (the center tab
        // carried the title — but Settings is a TAB-LESS full-page screen, so
        // nothing named the page) and the EXP-282 "hug the left edge under a
        // 672px cap" one (the nav is the window's left column now; a column
        // pinned to its right edge left the rest of a wide window empty).
        //
        // The SCROLL REGION stays full width with the scrollbar at the
        // viewport edge — only the content column is capped and centered, the
        // page-scaffold recipe every other full-page screen uses
        // (`actions_view::page_scaffold_with`).
        let team_name = active_team(cx, &self.nav)
            .map(|team| team.name)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "your team".to_string());
        let header = v_flex()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .child("Settings"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(SharedString::from(format!(
                        "Manage {team_name} and your account"
                    ))),
            );
        // `flex_shrink_0`: a 1px child in a flex column is the first thing
        // the layout eats.
        let separator = div()
            .w_full()
            .h(px(1.))
            .flex_shrink_0()
            .bg(row_stroke(cx));

        v_flex()
            .size_full()
            .min_h_0()
            .min_w_0()
            .child(crate::scroll_pane::v_scroll_pane(
                SharedString::from(format!("settings-detail-{effective:?}")),
                &self.scroll,
                div().w_full().min_w_0().child(
                    detail_column()
                        .child(header)
                        .child(separator)
                        .child(pane),
                ),
            ))
    }
}

/// EXP-771: the shared settings detail column — the web route's
/// `mx-auto w-full max-w-4xl space-y-6 p-6` grid. It used to be a
/// left-aligned 672px column (EXP-282); the nav lives in the window's left
/// column since EXP-456, so hugging its edge just left the rest of a wide
/// window blank. The `space-y-6` is the gap between the header, its hairline
/// and the pane — every pane spaces its OWN sections.
pub(crate) fn detail_column() -> gpui::Div {
    v_flex()
        .w_full()
        .min_w_0()
        .max_w(px(896.))
        .mx_auto()
        .p(px(24.))
        .gap(px(24.))
}

// ---------------------------------------------------------------------------
// SettingsNavPanel — the settings nav as the window's LEFT column (EXP-282)
// ---------------------------------------------------------------------------

/// The settings navigation, rendered by the `Shell` as the window's LEFT
/// column IN PLACE of the rail while `Screen::Settings` is up (EXP-456 — it
/// slides in as the rail slides out). It reads and
/// writes the window's shared [`RailShared::settings_section`], so the detail
/// view ([`SettingsView`]) always shows what this column highlights.
pub struct SettingsNavPanel {
    nav: Entity<Navigation>,
    shared: Entity<RailShared>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsNavPanel {
    pub fn new(window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let nav = nav_for_window(window, cx);
        let shared = rail_shared_for_window(window, cx);
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            // The Account row's highlight follows the active screen.
            cx.observe(&nav, |_, _, cx| cx.notify()),
            cx.observe(&shared, |_, _, cx| cx.notify()),
            // Owner gating hides rows — same data the detail view reads.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            // EXP-288: the Boards group lists the live synced boards.
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
        ];
        Self {
            nav,
            shared,
            _subscriptions: subscriptions,
        }
    }

    /// One nav row (hand-rolled — gpui-component's `Button` centers its
    /// inner layout, and these rows must read as a left-aligned list).
    /// EXP-288: id/label are `impl Into<…>` so dynamic per-board rows fit.
    fn row(
        id: impl Into<gpui::ElementId>,
        label: impl Into<SharedString>,
        icon: Option<Icon>,
        selected: bool,
        cx: &App,
    ) -> gpui::Stateful<gpui::Div> {
        h_flex()
            .id(id.into())
            .w_full()
            .px_2()
            .py_1()
            .gap_2()
            .items_center()
            .rounded(cx.theme().radius)
            .text_sm()
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(theme::tokens::glass::FILL_ACTIVE.to_hsla())
            })
            .hover(|this| this.bg(theme::tokens::glass::FILL_ROW.to_hsla()))
            .children(icon.map(|icon| icon.xsmall().flex_shrink_0()))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(label.into()),
            )
    }

    fn group_label(label: &'static str, cx: &App) -> impl IntoElement {
        div()
            .px_2()
            .pt_2()
            .pb_0p5()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().muted_foreground)
            .child(label)
    }

    /// EXP-288: the hairline between nav groups — the group labels alone
    /// read as floating headings.
    fn group_divider() -> impl IntoElement {
        div()
            .h(px(1.))
            .mx_2()
            .my_1()
            .bg(theme::tokens::glass::STROKE_ROW.to_hsla())
    }
}

impl Render for SettingsNavPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let owner = active_team_id(&self.nav, cx)
            .map(|ws| is_owner(cx, &ws))
            .unwrap_or(false);
        let board_ids = valid_board_ids(cx, &self.nav);
        let effective = effective_selection(
            self.shared.read(cx).settings_section(),
            owner,
            |id| board_ids.as_ref().is_none_or(|ids| ids.contains(id)),
        );
        let team_id = active_team_id(&self.nav, cx);
        let boards = team_id
            .as_deref()
            .map(|team_id| Store::global(cx).collections().boards_in_team(team_id, cx))
            .unwrap_or_default();

        let mut list = v_flex().p_2().gap_0p5();
        let mut first_group = true;
        for group in NAV_GROUPS {
            let visible: Vec<&NavItem> = group
                .items
                .iter()
                .filter(|item| section_visible(&item.section, owner))
                .collect();
            // EXP-288: the Boards group's per-board rows are injected ahead
            // of its static items (owner-only, like the pages themselves).
            let board_group = group.label == "Boards" && owner;
            if visible.is_empty() && !board_group {
                continue;
            }
            if !first_group {
                list = list.child(Self::group_divider());
            }
            first_group = false;
            list = list.child(Self::group_label(group.label, cx));
            if board_group {
                for board in &boards {
                    let section = SettingsSection::Board(board.id.clone());
                    let selected = section == effective;
                    let color = board
                        .color
                        .as_deref()
                        .and_then(parse_hex_color)
                        .unwrap_or(cx.theme().muted_foreground);
                    let icon = crate::icons::board_icon(board).text_color(color);
                    list = list.child(
                        Self::row(
                            SharedString::from(format!("settings-nav-board-{}", board.id)),
                            board.name.clone(),
                            Some(icon),
                            selected,
                            cx,
                        )
                        .on_click(cx.listener(move |_, _, window, cx| {
                            select_settings_section(window, cx, section.clone());
                            navigate(window, cx, Screen::Settings);
                        })),
                    );
                }
                if let Some(team_id) = team_id.clone() {
                    let new_board = Self::row(
                        "settings-nav-new-board",
                        "New board",
                        Some(Icon::new(registry::UI_ADD)),
                        false,
                        cx,
                    )
                    .text_color(cx.theme().muted_foreground)
                    .on_click(cx.listener(move |_, _, window, cx| {
                        crate::create_board_dialog::open(window, cx, team_id.clone());
                    }));
                    list = list.child(new_board);
                }
            }
            for item in visible {
                let section = item.section.clone();
                let selected = section == effective;
                list = list.child(
                    Self::row(
                        item.label,
                        item.label,
                        Some(section_icon(&section)),
                        selected,
                        cx,
                    )
                    .on_click(cx.listener(move |_, _, window, cx| {
                        select_settings_section(window, cx, section.clone());
                        navigate(window, cx, Screen::Settings);
                    })),
                );
            }
        }

        // EXP-456: the back affordance — web parity with the settings
        // sidebar's header row. Direct calls, not action dispatch (EXP-17).
        let back_row = nav_back_row("settings-nav-back", "Settings", cx)
            .on_click(cx.listener(|this, _, window, cx| {
                if this.nav.read(cx).can_go_back() {
                    go_back(window, cx);
                } else {
                    set_screen(window, cx, None);
                }
            }));

        v_flex()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            // EXP-456: the nav takes the rail's slot, so it wears the rail's
            // exact material — which since EXP-723/EXP-767 is NOTHING: no
            // section wash, no ramp, no rounded corners. It sits on the
            // Shell root's one ground like the rail does (a wash here read as
            // a lighter block with a hard right edge next to the content).
            .text_color(cx.theme().sidebar_foreground)
            // EXP-863: the column's titlebar strip and its fixed header (team
            // switcher, Search, New issue) are the `Shell`'s, above this
            // pane — the nav starts at its back row.
            .child(back_row)
            .child(
                div()
                    .id("settings-nav")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(list),
            )
    }
}


// ---------------------------------------------------------------------------
// Shared query helpers (settings-scoped)
// ---------------------------------------------------------------------------

/// The window's active synced team row.
pub(crate) fn active_team(
    cx: &App,
    nav: &Entity<Navigation>,
) -> Option<domain::rows::Team> {
    let team_id = active_team_id(nav, cx)?;
    Store::global(cx)
        .collections()
        .teams
        .read(cx)
        .get(&team_id)
        .cloned()
}

/// My membership row in `team_id` (id + role), from the synced
/// collections.
pub(crate) fn my_membership(cx: &App, team_id: &str) -> Option<(String, String)> {
    let me = queries::active_account(cx)?;
    Store::global(cx)
        .collections()
        .team_members
        .read(cx)
        .iter()
        .find(|member| member.team_id == team_id && member.user_id == me.user_id)
        .map(|member| {
            (
                member.id.clone(),
                member.role.clone().unwrap_or_else(|| "member".to_string()),
            )
        })
}

/// Web `isOwner` gate (settings route: `currentMember?.role === 'owner'`).
pub(crate) fn is_owner(cx: &App, team_id: &str) -> bool {
    my_membership(cx, team_id)
        .map(|(_, role)| role == domain::contract::TEAM_ROLE_OWNER)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Shared chrome bits (web Card + notices at compact density)
// ---------------------------------------------------------------------------

/// The settings panes' section container. EXP-862 takes the CARD back off:
/// a section is a HEADER BAND (`surface::glass_section_header`) over its
/// rows, the same shape every list page wears, so the panes stop being a
/// stack of bordered boxes holding other boxes. Nothing here paints a fill,
/// a stroke or padding of its own — the rows do that
/// (`surface::glass_group_rows`, `surface::flat_row`) and the detail column
/// owns the page gutter.
pub(crate) fn section(_cx: &App) -> gpui::Div {
    v_flex().w_full().min_w_0().gap_2()
}

/// EXP-282: the hairline the panes' rows/chips draw — the glass row stroke
/// instead of the heavier `theme.border`, now that no card frames them.
pub(crate) fn row_stroke(_cx: &App) -> gpui::Hsla {
    theme::tokens::glass::STROKE_ROW.to_hsla()
}

/// Web `formatStorage`: MB under a GB, one-decimal GB above. Shared by the
/// General billing summary and the Storage pane's usage meter (EXP-297).
pub(super) fn format_storage(mb: f64) -> String {
    if mb >= 1024. {
        let gb = mb / 1024.;
        if (gb - gb.round()).abs() < 0.05 {
            format!("{} GB", gb.round() as i64)
        } else {
            format!("{gb:.1} GB")
        }
    } else if (mb - mb.round()).abs() < 0.05 {
        format!("{} MB", mb.round() as i64)
    } else {
        format!("{mb:.1} MB")
    }
}

/// One usage row (web `UsageBar`): label left, "current / limit" right, a
/// thin progress track underneath (no track for unlimited).
pub(super) fn usage_bar(
    label: &'static str,
    current: String,
    limit: Option<String>,
    fraction: Option<f64>,
    cx: &App,
) -> impl IntoElement {
    let amount = match limit {
        Some(limit) => format!("{current} / {limit}"),
        None => format!("{current} / unlimited"),
    };
    let mut row = v_flex()
        .gap_1()
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(div().text_sm().child(label))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(SharedString::from(amount)),
                ),
        );
    if let Some(fraction) = fraction {
        let fraction = fraction.clamp(0., 1.) as f32;
        row = row.child(
            div()
                .h(gpui::px(6.))
                .w_full()
                .rounded_full()
                .bg(cx.theme().muted.opacity(0.35))
                .child(
                    div()
                        .h_full()
                        .rounded_full()
                        .w(gpui::relative(fraction))
                        .bg(cx.theme().primary),
                ),
        );
    }
    row
}

/// EXP-771: the web sections' `<p className="px-1 pb-2 text-xs
/// text-foreground/50">` — the explanatory line UNDER a
/// [`crate::surface::glass_section_header`], where the web puts it, instead of
/// a `CardDescription` fused into the heading. Its own `pb_2` is the gap to
/// the list below (same rule as the header's — never wrap it in a gapped
/// column, EXP-697).
pub(crate) fn section_description(
    description: impl Into<SharedString>,
    cx: &App,
) -> impl IntoElement {
    div()
        .px_1()
        .pb_2()
        .text_xs()
        .text_color(cx.theme().foreground.opacity(0.5))
        .child(description.into())
}

/// EXP-720: the ONE danger-zone recipe, the web `settings/general.tsx` twin —
/// a [`section`] whose heading is tinted `danger` over ONE glass row:
/// the muted description leading, the destructive action trailing. Both the
/// team's "Delete team" and the device's "Reset IDE data" wear it; the
/// red-bordered tinted card General used to draw is gone (it was the only
/// bordered box left in the panes after EXP-282 flattened them).
pub(crate) fn danger_zone(
    description: impl Into<SharedString>,
    action: impl IntoElement,
    cx: &App,
) -> gpui::Div {
    // EXP-818: the group row IS the card here — not a card inside `section`.
    v_flex()
        .w_full()
        .gap_3()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().danger)
                .child("Danger zone"),
        )
        .child(
            crate::surface::glass_group_rows(vec![crate::surface::glass_row_shell()
                .justify_between()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(description.into()),
                )
                .child(div().flex_shrink_0().child(action))]),
        )
}

/// Inline destructive error box (web `text-destructive` / bordered error).
pub(crate) fn error_notice(message: SharedString, cx: &App) -> impl IntoElement {
    div()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().danger.opacity(0.5))
        .bg(cx.theme().danger.opacity(0.1))
        .text_sm()
        .text_color(cx.theme().danger)
        .child(message)
}

/// §4.9 plan-cap surface: a neutral "Upgrade on the web" notice — never an
/// in-app purchase/pricing UI.
pub(crate) fn upgrade_notice(message: SharedString, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_1()
        .px_3()
        .py_2()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().primary.opacity(0.4))
        .bg(cx.theme().primary.opacity(0.05))
        .text_sm()
        .child(message)
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Upgrade on the web to raise this limit."),
        )
}

/// `#rrggbb` → Hsla (label/board colors are stored as hex strings).
pub(crate) fn parse_hex_color(hex: &str) -> Option<gpui::Hsla> {
    let hex = hex.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(
        gpui::Rgba {
            r: r as f32 / 255.,
            g: g as f32 / 255.,
            b: b as f32 / 255.,
            a: 1.0,
        }
        .into(),
    )
}

// ---------------------------------------------------------------------------
// Shared mutation plumbing
// ---------------------------------------------------------------------------

/// §4.1 un-gated fire-and-forget mutation: run the blocking tRPC call on a
/// background thread; the UI updates via the Electric echo. Errors are
/// logged (the web's inline sections behave the same for these calls).
pub(crate) fn spawn_trpc<T, F>(cx: &mut App, what: &'static str, call: F)
where
    T: Send + 'static,
    F: FnOnce(&api::TrpcClient) -> Result<T, api::ApiError> + Send + 'static,
{
    let Some(trpc) = queries::trpc_client(cx) else {
        log::warn!("[ui] {what} skipped: no signed-in account");
        return;
    };
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = call(&trpc) {
                log::warn!("[ui] {what} failed: {err}");
            }
        })
        .detach();
}

/// Open a URL through the robust opener chain (never a raw xdg-open),
/// off the foreground thread.
pub(crate) fn open_url(cx: &mut App, url: String) {
    cx.background_executor()
        .spawn(async move {
            if let Err(err) = api::opener::open_in_browser(&url) {
                log::warn!("[ui] open-in-browser failed: {err}");
            }
        })
        .detach();
}

/// The inline error a settings form shows (EXP-771, web parity): the SERVER's
/// own message when the rejection carried one (the web's
/// `err instanceof Error ? err.message : …`), otherwise the form's own fallback
/// sentence — a transport or decode fault never reaches the user as a dump.
pub(crate) fn form_error(err: &api::ApiError, fallback: &str) -> String {
    match err {
        api::ApiError::Http { message, .. } if !message.trim().is_empty() => message.clone(),
        _ => fallback.to_string(),
    }
}

/// A plan-cap rejection (`planLimitError` → PRECONDITION_FAILED / HTTP 412
/// with the "Your plan allows" prefix). Drives the §4.9 "Upgrade on the web"
/// notice. Delegates to the ONE prefix-checking classifier — a bare-412 match
/// here once rendered "GitHub suspended…"/"No GitHub account connected" as a
/// plan-limit upsell (EXP-365).
pub(crate) fn is_plan_limit(err: &api::ApiError) -> bool {
    crate::create_board_dialog::is_plan_limit(err)
}

/// Leading clause of the server's team-delete billing gate (REV2-55):
/// `teams.delete` refuses a team whose subscription is still live. Kept in
/// sync with `TEAM_DELETE_ACTIVE_SUBSCRIPTION_MESSAGE`
/// (apps/web/src/lib/billing/billing-handover.ts) — only this stable clause is
/// matched, because the server's trailing pointer names a web-only screen.
pub(crate) const TEAM_DELETE_SUBSCRIPTION_PREFIX: &str = "This team has an active subscription";

/// Desktop copy for that gate. The server sends the owner to "team settings →
/// Billing"; this app's Plan & Billing card is READ-ONLY and hands off to the
/// browser (no in-app purchase/cancel UI), so the refusal names the web —
/// pairing with the "Manage billing on the web" button right above the
/// Danger Zone.
pub(crate) const TEAM_DELETE_SUBSCRIPTION_MESSAGE: &str =
    "This team has an active subscription. Cancel the subscription on the web before deleting the team.";

/// Sanitizer for a failed `teams.delete`: the server's message is
/// user-presentable and rendered verbatim (web parity), except the billing
/// gate, whose web-only wording is swapped for the desktop twin. Non-HTTP
/// failures (transport/decode) get a generic prefix instead of a raw dump.
pub(crate) fn team_delete_error_message(err: &api::ApiError) -> SharedString {
    match err {
        api::ApiError::Http { status: 412, message }
            if message.starts_with(TEAM_DELETE_SUBSCRIPTION_PREFIX) =>
        {
            TEAM_DELETE_SUBSCRIPTION_MESSAGE.into()
        }
        api::ApiError::Http { message, .. } => message.clone().into(),
        // EXP-533: a plain sentence (offline says so), never a reqwest dump.
        other => format!("Couldn't delete the team: {}", other.user_message()).into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any_board(_: &str) -> bool {
        true
    }

    /// EXP-862 (×4): the personal keys pane is called **Security** in the nav
    /// (web `/settings/security`), and the variant it selects is unchanged —
    /// a rename that moved the section would orphan every stored selection.
    #[test]
    fn the_personal_keys_entry_is_labelled_security() {
        let entry = NAV_GROUPS
            .iter()
            .flat_map(|group| group.items)
            .find(|item| item.section == SettingsSection::ApiKeys)
            .expect("the personal keys entry is in the nav");
        assert_eq!(entry.label, "Security");
        assert!(
            !NAV_GROUPS
                .iter()
                .flat_map(|group| group.items)
                .any(|item| item.label == "API keys"),
            "the old label is gone everywhere"
        );
    }

    #[test]
    fn owner_defaults_to_general() {
        assert_eq!(
            effective_selection(SettingsSection::General, true, any_board),
            SettingsSection::General
        );
    }

    #[test]
    fn non_owner_falls_back_to_members() {
        for gated in [
            SettingsSection::General,
            SettingsSection::Storage,
            SettingsSection::Board("b-1".to_string()),
            SettingsSection::ArchivedBoards,
            // EXP-771: the Features group mirrors the web's
            // `canManageWidgets` gate, which IS `isOwner`.
            SettingsSection::Widget,
            SettingsSection::Helpdesk,
        ] {
            assert_eq!(
                effective_selection(gated, false, any_board),
                SettingsSection::Members
            );
        }
        // EXP-557: Repositories is member-visible — the selection sticks
        // instead of falling back.
        assert_eq!(
            effective_selection(SettingsSection::Repositories, false, any_board),
            SettingsSection::Repositories
        );
    }

    #[test]
    fn device_sections_never_gated() {
        for section in [
            SettingsSection::Tools,
            SettingsSection::Agents,
            SettingsSection::LocalRepos,
        ] {
            assert!(section_visible(&section, false));
            assert_eq!(
                effective_selection(section.clone(), false, any_board),
                section
            );
        }
    }

    /// EXP-262/EXP-238: the Personal sections are never gated and never fall
    /// back — for any owner state the selection sticks. They sit LAST in
    /// `NAV_GROUPS`, so the fallback scan still lands on General/Members,
    /// never on a personal pane.
    #[test]
    fn personal_sections_never_gated_and_never_fall_back() {
        for section in [
            SettingsSection::Account,
            SettingsSection::Notifications,
            SettingsSection::ApiKeys,
            SettingsSection::About,
        ] {
            for owner in [false, true] {
                assert!(section_visible(&section, owner));
                assert_eq!(
                    effective_selection(section.clone(), owner, any_board),
                    section
                );
            }
        }
    }

    #[test]
    fn ungated_selection_is_kept() {
        assert_eq!(
            effective_selection(SettingsSection::Labels, false, any_board),
            SettingsSection::Labels
        );
    }

    /// REV2-55: the billing gate's web-only wording ("team settings →
    /// Billing") never reaches the Danger Zone — desktop billing is a
    /// read-only hand-off to the browser.
    #[test]
    fn team_delete_billing_gate_is_rewritten_for_desktop() {
        let err = api::ApiError::Http {
            status: 412,
            message: "This team has an active subscription. Cancel the subscription in \
                      team settings → Billing before deleting the team."
                .to_string(),
        };
        let message = team_delete_error_message(&err);
        assert_eq!(message.as_ref(), TEAM_DELETE_SUBSCRIPTION_MESSAGE);
        assert!(!message.contains("team settings"));
    }

    #[test]
    fn other_delete_failures_keep_the_server_message() {
        // billing.cancelSubscription's "has NO active subscription" is a
        // different precondition — the prefix must not swallow it.
        let err = api::ApiError::Http {
            status: 412,
            message: "This team has no active subscription".to_string(),
        };
        assert_eq!(
            team_delete_error_message(&err).as_ref(),
            "This team has no active subscription"
        );
        let err = api::ApiError::Http {
            status: 403,
            message: "Only team owners can delete a team".to_string(),
        };
        assert_eq!(
            team_delete_error_message(&err).as_ref(),
            "Only team owners can delete a team"
        );
    }

    /// EXP-771: the settings forms show the SERVER's sentence when there is
    /// one and the web's own fallback otherwise — never a transport dump.
    #[test]
    fn form_errors_prefer_the_server_message_then_the_web_fallback() {
        let err = api::ApiError::Http {
            status: 409,
            message: "A status with this name already exists.".to_string(),
        };
        assert_eq!(
            form_error(&err, "Failed to create status."),
            "A status with this name already exists."
        );
        let blank = api::ApiError::Http {
            status: 500,
            message: "   ".to_string(),
        };
        assert_eq!(
            form_error(&blank, "Failed to create status."),
            "Failed to create status."
        );
        let offline = api::ApiError::transport("connection reset".to_string());
        assert_eq!(
            form_error(&offline, "Failed to rename label."),
            "Failed to rename label."
        );
    }

    #[test]
    fn transport_failures_get_a_generic_prefix() {
        let err = api::ApiError::transport("connection reset".to_string());
        assert!(team_delete_error_message(&err).starts_with("Couldn't delete the team:"));
    }

    /// EXP-288: a selected board page survives while the board exists and
    /// falls back once it's gone (trash/team switch) — and General (the
    /// owner's first item) wins the fallback scan over Members.
    #[test]
    fn stale_board_selection_falls_back() {
        let selected = SettingsSection::Board("b-1".to_string());
        assert_eq!(
            effective_selection(selected.clone(), true, |id| id == "b-1"),
            selected
        );
        assert_eq!(
            effective_selection(selected, true, |_| false),
            SettingsSection::General
        );
    }

    /// EXP-771/EXP-792: the Features group sits between Boards and This
    /// device, its labels are the web's, and its gating is the web's too —
    /// the two widget panes are owner-only (`canManageWidgets`), MCP servers
    /// is member-visible (`visible: () => true`), because every member reads
    /// the registry and signs in on their OWN machines. The nav order also
    /// feeds the fallback scan, which must keep landing on a Team section.
    #[test]
    fn features_group_sits_between_boards_and_this_device() {
        let groups: Vec<&str> = NAV_GROUPS.iter().map(|group| group.label).collect();
        assert_eq!(
            groups,
            vec!["Team", "Boards", "Features", "This device", "Personal"]
        );
        let features = NAV_GROUPS
            .iter()
            .find(|group| group.label == "Features")
            .expect("Features group");
        let labels: Vec<&str> = features.items.iter().map(|item| item.label).collect();
        assert_eq!(labels, vec!["Feedback widget", "Helpdesk", "MCP servers"]);
        for item in features.items {
            assert!(section_visible(&item.section, true));
            assert_eq!(
                section_visible(&item.section, false),
                item.section == SettingsSection::McpServers,
                "{} owner gating must match the web nav's `visible`",
                item.label
            );
        }
    }

    /// EXP-771: web copy wins — the nav says "Statuses", not the longer
    /// desktop-only "Issue statuses".
    #[test]
    fn statuses_nav_label_matches_the_web() {
        let label = NAV_GROUPS
            .iter()
            .flat_map(|group| group.items)
            .find(|item| item.section == SettingsSection::Statuses)
            .map(|item| item.label);
        assert_eq!(label, Some("Statuses"));
    }

    /// EXP-500: archiving the board whose settings pane is open drops it out
    /// of the synced collection, which is exactly the stale-`Board(id)` case
    /// above — the nav clamps to General instead of rendering a blank pane.
    /// The Archived boards pane it was archived from stays reachable.
    #[test]
    fn archived_boards_pane_is_owner_only_and_never_clamped() {
        assert!(section_visible(&SettingsSection::ArchivedBoards, true));
        assert!(!section_visible(&SettingsSection::ArchivedBoards, false));
        assert_eq!(
            effective_selection(SettingsSection::ArchivedBoards, true, |_| false),
            SettingsSection::ArchivedBoards
        );
    }
}
