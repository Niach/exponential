//! Team settings + account screens (masterplan-v3 §4.2 "Settings" /
//! "Account", §7.9 integrations surface).
//!
//! Web parity targets: the `routes/t/$teamSlug/settings/` pages and
//! their `components/team/*-section.tsx` cards, plus
//! `routes/_authenticated/account/notifications.tsx`. The team-settings
//! screen mirrors the web's grouped master-detail layout (EXP-146): a fixed
//! left nav with the groups — **Team** (General, Members, Labels, Storage —
//! the EXP-297 owner-only attachment manager), **Boards**
//! (one entry PER board + New board + Repositories — EXP-288 flattened the
//! old flat Boards list into per-board detail pages), and the desktop-only
//! **This device** group (Tools, Agents, Local repositories); the detail
//! column shows ONE selected pane with the web's `isOwner &&` gating
//! (General additionally hides when solo — the pane renders nothing there,
//! matching the web); each pane mirrors its web card field-for-field.
//!
//! Navigation INTO these screens: the rail's gear dispatches `OpenSettings`
//! (see `sidebar.rs` + `navigation::init`); this module only provides the
//! screens.
//!
//! Billing (EXP-288): General carries a READ-ONLY plan/usage summary with a
//! "Manage on the web" hand-off — still no in-app purchase/pricing UI, no
//! widget-config pane, no admin surface. Plan-cap failures (HTTP 412 from
//! `lib/billing.ts`) render as a neutral "Upgrade on the web" notice. The
//! GitHub App *install* is a browser hand-off (§7.9).

mod account;
mod add_repository_dialog;
mod agents;
mod labels;
mod statuses;
mod local_repos;
mod members;
mod notifications_prefs;
mod board_detail;
mod repositories;
mod storage;
mod team_general;
mod tools;

pub use account::AccountView;

/// EXP-282: width of the settings nav column — it REPLACES the tool column
/// while a settings screen is up (rendered by `shell::CenterPanel`), so it
/// owns a fixed width like the right detail sidebars rather than riding the
/// resizable split.
pub const SETTINGS_NAV_WIDTH: f32 = 212.;

use gpui::{
    div, prelude::FluentBuilder as _, px, App, AppContext as _, Entity, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon, Sizable as _};
use sync::Store;

use crate::navigation::{
    active_team_id, nav_for_window, navigate, resolved_screen, Navigation, Screen,
};
use crate::icons::registry;
use crate::queries;
use crate::sidebar::{rail_shared_for_window, select_settings_section, RailShared};

use labels::LabelsPane;
use statuses::StatusesPane;
use local_repos::LocalReposPane;
use members::MembersPane;
use agents::AgentsPane;
use board_detail::BoardDetailPane;
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
    Repositories,
    /// This-device tools (EXP-288: renamed from "Coding" — repos root,
    /// branch prefix, terminal shell).
    Tools,
    /// The per-agent launcher settings + doctor (EXP-288: split out of the
    /// old Coding pane).
    Agents,
    LocalRepos,
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
/// web-only Billing/Widget items, plus the desktop-only "This device" group.
/// The Boards group's per-board rows + "New board" are injected dynamically
/// at render (EXP-288); order defines both the nav and the fallback scan
/// (first visible item).
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
            // together, and both are member-editable.
            NavItem {
                label: "Issue statuses",
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
        items: &[NavItem {
            label: "Repositories",
            section: SettingsSection::Repositories,
        }],
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
                label: "Local repositories",
                section: SettingsSection::LocalRepos,
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
        SettingsSection::Board(_) => Icon::from(registry::SETTINGS_BOARDS),
        SettingsSection::Repositories => Icon::from(registry::SETTINGS_REPOSITORIES),
        SettingsSection::Tools => Icon::from(registry::SETTINGS_TOOLS),
        SettingsSection::Agents => Icon::from(registry::SETTINGS_AGENTS),
        SettingsSection::LocalRepos => Icon::from(registry::SETTINGS_LOCAL_REPOS),
    }
}

/// Web nav `visible` gating: General/board pages/Repositories are owner-only,
/// and General additionally hides when solo (GeneralPane renders nothing
/// there, mirroring the web section's `if (solo) return null`).
fn section_visible(section: &SettingsSection, owner: bool, solo: bool) -> bool {
    match section {
        SettingsSection::General => owner && !solo,
        SettingsSection::Storage
        | SettingsSection::Board(_)
        | SettingsSection::Repositories => owner,
        _ => true,
    }
}

/// The synced board ids of the active team, used to clamp a stale
/// `Board(id)` selection. `None` while the boards collection is still
/// loading — treat any selection as valid then (bias kept, mirroring
/// `is_solo_team`'s loading bias) so a restored selection doesn't bounce.
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
    solo: bool,
    board_ok: impl Fn(&str) -> bool,
) -> SettingsSection {
    let visible = section_visible(&selected, owner, solo)
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
        .find(|section| section_visible(section, owner, solo))
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
    /// EXP-288: ONE per-board detail pane — it reads the selected `Board(id)`
    /// from the shared selection itself and re-points at board switches.
    board_detail: Entity<BoardDetailPane>,
    repositories: Entity<RepositoriesPane>,
    /// This-device tools (EXP-288: repos root, branch prefix, terminal
    /// shell) — local per-install state, so NOT owner-gated.
    tools: Entity<ToolsPane>,
    /// Per-agent launcher settings + doctor (EXP-288: split out of Tools).
    agents: Entity<AgentsPane>,
    /// §4.7 desktop-only Local repositories section (clone disk usage +
    /// prune/remove) — local per-install state, un-gated.
    local_repos: Entity<LocalReposPane>,
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
        let board_detail =
            cx.new(|cx| BoardDetailPane::new(nav.clone(), shared.clone(), window, cx));
        let repositories = cx.new(|cx| RepositoriesPane::new(nav.clone(), window, cx));
        let tools = cx.new(|cx| ToolsPane::new(window, cx));
        let agents = cx.new(|cx| AgentsPane::new(window, cx));
        let local_repos = cx.new(LocalReposPane::new);

        // The section nav + header depend on role (owner gating) and the
        // solo heuristic — re-render when membership/team data moves.
        let collections = Store::global(cx).collections().clone();
        let subscriptions = vec![
            cx.observe(&nav, |_, _, cx| cx.notify()),
            // EXP-282: the nav column mutates the shared selection.
            cx.observe(&shared, |_, _, cx| cx.notify()),
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
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
            board_detail,
            repositories,
            tools,
            agents,
            local_repos,
            shared,
            _subscriptions: subscriptions,
        }
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let owner = active_team_id(&self.nav, cx)
            .map(|ws| is_owner(cx, &ws))
            .unwrap_or(false);
        let solo = {
            let team_id = active_team_id(&self.nav, cx);
            team_id
                .as_deref()
                .map(|ws| !show_team_chrome(cx, ws))
                .unwrap_or(true)
        };
        // Web settings layout route (EXP-146): grouped left nav + one
        // selected section pane in the detail column. EXP-282: the nav is
        // the window's left column now ([`SettingsNavPanel`]) — this view is
        // the detail column alone.
        let board_ids = valid_board_ids(cx, &self.nav);
        let effective = effective_selection(
            self.shared.read(cx).settings_section(),
            owner,
            solo,
            |id| board_ids.as_ref().is_none_or(|ids| ids.contains(id)),
        );

        let pane: gpui::AnyElement = match &effective {
            SettingsSection::General => self.general.clone().into_any_element(),
            SettingsSection::Members => self.members.clone().into_any_element(),
            SettingsSection::Labels => self.labels.clone().into_any_element(),
            SettingsSection::Statuses => self.statuses.clone().into_any_element(),
            SettingsSection::Storage => self.storage.clone().into_any_element(),
            // The pane reads the selected board id from the shared selection
            // itself (it needs to flush a pending rename on board switches).
            SettingsSection::Board(_) => self.board_detail.clone().into_any_element(),
            SettingsSection::Repositories => self.repositories.clone().into_any_element(),
            SettingsSection::Tools => self.tools.clone().into_any_element(),
            SettingsSection::Agents => self.agents.clone().into_any_element(),
            SettingsSection::LocalRepos => self.local_repos.clone().into_any_element(),
        };

        // EXP-277: no screen header (the center tab already carries the
        // title). EXP-282: no nav column and no centering wrapper either —
        // the content column hugs the left edge under a 672px cap, so every
        // pane's headings line up with the nav column beside it.
        //
        // Scroll id keyed by section so each section keeps an independent
        // scroll offset.
        div()
            .id(SharedString::from(format!("settings-detail-{effective:?}")))
            .size_full()
            .min_w_0()
            .overflow_y_scroll()
            .child(detail_column().child(pane))
    }
}

/// EXP-282: the shared settings detail column — left-aligned, capped, with
/// generous section spacing (the panes are flat sections now, so whitespace
/// is the only separator left). Also used by the Account screen so both
/// settings surfaces sit on the same grid.
pub(crate) fn detail_column() -> gpui::Div {
    v_flex().w_full().max_w(px(672.)).p_5().gap_6()
}

// ---------------------------------------------------------------------------
// SettingsNavPanel — the settings nav as the window's LEFT column (EXP-282)
// ---------------------------------------------------------------------------

/// The settings navigation, rendered by `shell::CenterPanel` IN PLACE of the
/// tool column while `Screen::Settings`/`Screen::Account` is up. It reads and
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
            // Owner/solo gating hides rows — same data the detail view reads.
            cx.observe(&collections.teams, |_, _, cx| cx.notify()),
            cx.observe(&collections.team_members, |_, _, cx| cx.notify()),
            cx.observe(&collections.users, |_, _, cx| cx.notify()),
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
        let solo = active_team_id(&self.nav, cx)
            .as_deref()
            .map(|ws| !show_team_chrome(cx, ws))
            .unwrap_or(true);
        // The Account screen IS a settings section as far as this column is
        // concerned — while it is up no team/device row is highlighted.
        let on_account = matches!(resolved_screen(&self.nav, cx), Some(Screen::Account));
        let board_ids = valid_board_ids(cx, &self.nav);
        let effective = effective_selection(
            self.shared.read(cx).settings_section(),
            owner,
            solo,
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
                .filter(|item| section_visible(&item.section, owner, solo))
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
                    let selected = !on_account && section == effective;
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
                let selected = !on_account && section == effective;
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
                        // A section click from the Account screen returns
                        // to the settings detail.
                        navigate(window, cx, Screen::Settings);
                    })),
                );
            }
        }
        // EXP-282: Account moved INTO the settings chrome (it used to be a
        // second account-dropdown entry with its own bare screen).
        list = list
            .child(Self::group_divider())
            .child(Self::group_label("Personal", cx))
            .child(
                Self::row(
                    "settings-nav-account",
                    "Account",
                    Some(Icon::new(registry::SETTINGS_ACCOUNT)),
                    on_account,
                    cx,
                )
                .on_click(cx.listener(|_, _, window, cx| {
                    navigate(window, cx, Screen::Account);
                })),
            );

        v_flex()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            // EXP-285: like the tool column it replaces — no fill, just a
            // hairline boundary over the one page gradient.
            .border_r_1()
            .border_color(theme::tokens::glass::STROKE_ROW.to_hsla())
            .text_color(cx.theme().sidebar_foreground)
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
// Shared query helpers (settings-scoped; the general chrome helper moves to
// `queries.rs` with the §4.8 sidebar solo rule)
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

/// Web `useIsSolo`: true while data loads (bias hidden), else "≤1 human
/// member" (agents excluded).
pub(crate) fn is_solo_team(cx: &App, team_id: &str) -> bool {
    let collections = Store::global(cx).collections();
    if !collections.team_members.read(cx).is_ready()
        || !collections.teams.read(cx).is_ready()
    {
        return true;
    }
    if collections.teams.read(cx).get(team_id).is_none() {
        return true;
    }
    let member_count = collections
        .team_members
        .read(cx)
        .iter()
        .filter(|member| member.team_id == team_id)
        .count();
    member_count <= 1
}

/// Web `useShowTeamChrome`: revealed when the team stops being solo
/// OR the user explicitly reasons about 2+ teams.
pub(crate) fn show_team_chrome(cx: &App, team_id: &str) -> bool {
    let is_solo = is_solo_team(cx, team_id);
    let Some(me) = queries::active_account(cx) else {
        return !is_solo;
    };
    let collections = Store::global(cx).collections();
    let membership_ids: std::collections::HashSet<String> = collections
        .team_members
        .read(cx)
        .iter()
        .filter(|member| member.user_id == me.user_id)
        .map(|member| member.team_id.clone())
        .collect();
    // Web parity: the count reduces to "teams I have a membership row
    // in".
    let teams = collections.teams.read(cx);
    let explicit_count = teams
        .iter()
        .filter(|team| membership_ids.contains(&team.id))
        .count();
    !is_solo || explicit_count > 1
}

// ---------------------------------------------------------------------------
// Shared chrome bits (web Card + notices at compact density)
// ---------------------------------------------------------------------------

/// EXP-282: the settings panes' section container — FLAT. It used to be the
/// glass card (EXP-269); stacked cards inside an already-glass column read as
/// boxes-in-boxes, so a section is now just a left-aligned block whose
/// heading ([`card_header`]) carries the structure. Renamed `card` → `section`
/// across the panes.
pub(crate) fn section(_cx: &App) -> gpui::Div {
    v_flex().w_full().gap_3()
}

/// EXP-282: the hairline the panes' rows/chips draw — the glass row stroke
/// instead of the heavier `theme.border`, now that no card frames them.
pub(crate) fn row_stroke(_cx: &App) -> gpui::Hsla {
    theme::tokens::glass::STROKE_ROW.to_hsla()
}

/// EXP-285: one settings preference row — label + hint column left (the hint
/// wraps at a readable measure instead of sprawling the full pane), the
/// control pinned right, hairline separators BETWEEN rows carrying the
/// rhythm (`first` rows draw none).
pub(crate) fn pref_row(
    label: impl IntoElement,
    hint: impl Into<SharedString>,
    control: impl IntoElement,
    first: bool,
    cx: &App,
) -> gpui::Div {
    h_flex()
        .w_full()
        .items_center()
        .gap_4()
        .py_2p5()
        .when(!first, |row| row.border_t_1().border_color(row_stroke(cx)))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(label)
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .max_w(px(460.))
                        .child(hint.into()),
                ),
        )
        .child(div().flex_none().child(control))
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

/// Web `CardTitle` alone — a section whose rows speak for themselves needs no
/// explanatory paragraph under the heading (EXP-328).
pub(crate) fn card_title(title: impl Into<SharedString>) -> impl IntoElement {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .child(title.into())
}

/// Web `CardTitle` + `CardDescription`.
pub(crate) fn card_header(
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    cx: &App,
) -> impl IntoElement {
    v_flex()
        .gap_0p5()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(title.into()),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(description.into()),
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
        other => format!("Couldn't delete the team: {other}").into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any_board(_: &str) -> bool {
        true
    }

    #[test]
    fn owner_defaults_to_general() {
        assert_eq!(
            effective_selection(SettingsSection::General, true, false, any_board),
            SettingsSection::General
        );
    }

    #[test]
    fn non_owner_falls_back_to_members() {
        for gated in [
            SettingsSection::General,
            SettingsSection::Storage,
            SettingsSection::Board("b-1".to_string()),
            SettingsSection::Repositories,
        ] {
            assert_eq!(
                effective_selection(gated, false, false, any_board),
                SettingsSection::Members
            );
        }
    }

    #[test]
    fn solo_owner_hides_general() {
        // GeneralPane renders nothing when solo (web parity), so the nav must
        // hide it and the default selection must fall through to Members.
        assert!(!section_visible(&SettingsSection::General, true, true));
        assert_eq!(
            effective_selection(SettingsSection::General, true, true, any_board),
            SettingsSection::Members
        );
        // Solo does NOT gate the other owner sections.
        assert!(section_visible(
            &SettingsSection::Board("b-1".to_string()),
            true,
            true
        ));
    }

    #[test]
    fn device_sections_never_gated() {
        for section in [
            SettingsSection::Tools,
            SettingsSection::Agents,
            SettingsSection::LocalRepos,
        ] {
            assert!(section_visible(&section, false, true));
            assert_eq!(
                effective_selection(section.clone(), false, true, any_board),
                section
            );
        }
    }

    #[test]
    fn ungated_selection_is_kept() {
        assert_eq!(
            effective_selection(SettingsSection::Labels, false, false, any_board),
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
            message: "This team has an active subscription — cancel the subscription in \
                      team settings → Billing before deleting the team"
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

    #[test]
    fn transport_failures_get_a_generic_prefix() {
        let err = api::ApiError::Transport("connection reset".to_string());
        assert!(team_delete_error_message(&err).starts_with("Couldn't delete the team:"));
    }

    /// EXP-288: a selected board page survives while the board exists and
    /// falls back once it's gone (trash/team switch) — and General (the
    /// owner's first item) wins the fallback scan over Members.
    #[test]
    fn stale_board_selection_falls_back() {
        let selected = SettingsSection::Board("b-1".to_string());
        assert_eq!(
            effective_selection(selected.clone(), true, false, |id| id == "b-1"),
            selected
        );
        assert_eq!(
            effective_selection(selected, true, false, |_| false),
            SettingsSection::General
        );
    }
}
