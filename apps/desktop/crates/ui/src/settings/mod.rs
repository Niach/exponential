//! The ONE settings screen (masterplan-v3 §4.2 "Settings", §7.9 integrations
//! surface; EXP-238 folded the old Account screen in).
//!
//! Web parity targets: the `routes/t/$teamSlug/settings/` pages and
//! their `components/team/*-section.tsx` + `components/account/*` cards. The
//! screen mirrors the web's grouped master-detail layout (EXP-146): a fixed
//! left nav and ONE selected pane with the web's `isOwner &&` gating. Since
//! #EXP-2027 (no current team) the nav is one of TWO (`settings-nav.json`
//! `desktopNavs`): a team's (General, Members, Issues ⟩ Labels, Statuses,
//! Repositories, Prompts, MCP, Skills, Widget, Storage, All boards) under
//! the team's mark + name, or the member's personal one (Account, Assistant,
//! Notifications, Security, About) under `← Settings`. A board's, agent's
//! or app's page opens from its sidebar row; the This device pages
//! ([`DEVICE_PAGES`]) belong to the device page. Each pane mirrors its web
//! card field-for-field under the web route's OWN header (EXP-771) in a
//! centered `max-w-4xl` column.
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
//! EXP-771: the widget settings are NO LONGER web-only. The
//! Features group carries a READ-ONLY widget list with a "Manage on the web"
//! hand-off (authoring a config stays a browser flow — it needs the embed
//! snippet, the domain allowlist and the theme editor). SLOP-4: the helpdesk
//! pane beside it is gone with the helpdesk.

pub(crate) mod about;
// #EXP-1629: what the panes' eUI documents share.
mod doc;
// #EXP-1629: the bench's Branch rows (IDE vs genUI `trailing`).
#[cfg(test)]
pub(crate) mod branch_row_shots;
// #EXP-1764: the bench's Account rows (IDE vs genUI `triggerIcon`).
#[cfg(test)]
pub(crate) mod account_row_shots;
// #EXP-1765: the bench's repo branch pickers (IDE vs genUI `pill` Select).
#[cfg(test)]
pub(crate) mod branch_picker_shots;
pub(crate) mod account;
pub(crate) mod add_repository_dialog;
// EXP-1294: one team agent's page (the board page's twin); its device
// candidates + picker are shared with the New agent dialog.
pub(crate) mod agent_detail;
mod agent_doc;
pub(crate) mod agent_triggers;
// VAPP-177: the app editor's Settings tab (rendered by the app screen).
pub(crate) mod app_detail;
// #EXP-1850: the app screen's Events tab.
pub(crate) mod app_events;
pub(crate) mod app_mcp;
pub(crate) mod new_agent;
pub(crate) mod agents;
pub(crate) mod agents_index;
pub(crate) mod api_keys;
pub(crate) mod doctor_section;
// EXP-630: THE invite-by-email form (the pane's own and the resend dialog's)
// and the "Resend invite" window that hosts a prefilled copy of it.
mod invite_form;
pub(crate) mod resend_invite_dialog;
pub(crate) mod issues;
pub(crate) mod labels;
pub(crate) mod mcp;
pub(crate) mod mcp_servers;
// EXP-810: the pane's own add/edit form — a WINDOW, not an alert (the form
// re-renders on its own state).
pub(crate) mod mcp_server_dialog;
pub(crate) mod statuses;
pub(crate) mod local_repos;
pub(crate) mod members;
pub(crate) mod all_boards;
pub(crate) mod change_email_dialog;
pub(crate) mod sign_in_methods;
pub(crate) mod notifications_prefs;
// NEW-67: Settings → Personal → Voice (`fixtures/voice.json`).
pub(crate) mod voice_prefs;
// NEW-6: Settings → Prompts (the list + one prompt's page).
pub(crate) mod prompts;
// #EXP-1660: Settings › Skills (the list + one skill's page).
pub(crate) mod skills;
pub(crate) mod board_detail;
pub(crate) mod readiness;
pub(crate) mod repositories;
// #EXP-1616: one repository's page (repo-page.json).
pub(crate) mod repo_page;
pub(crate) mod sessions;
pub(crate) mod storage;
pub(crate) mod team_general;
pub(crate) mod tools;
pub(crate) mod widget;
// #EXP-1629: the detail column's chrome (the route header over the pane).
pub(crate) mod shell;
use shell::{empty_pane, pane_of, PaneBuild};

use gpui::{
    div, App, AppContext as _, Entity,
    InteractiveElement as _, IntoElement, ParentElement, Render, SharedString,
    StatefulInteractiveElement as _, Styled, Subscription, Window,
};
use gpui_component::{
    v_flex, ActiveTheme as _,
};
use sync::Store;

use crate::navigation::{
    screen_team_id, go_back, nav_for_window, navigate, set_screen, Navigation, Screen,
};
use crate::queries;
use crate::sidebar::{rail_shared_for_window, select_settings_section, RailShared};

use about::AboutPane;
use account::AccountPane;
use api_keys::ApiKeysPane;
use issues::IssuesPane;
use labels::LabelsPane;
use mcp::{McpActivityPane, McpPane};
use widget::WidgetPane;
use statuses::StatusesPane;
use local_repos::LocalReposPane;
use members::MembersPane;
use agent_detail::AgentDetailPane;
use new_agent::NewAgentPane;
use agents::AgentsPane;
use all_boards::AllBoardsPane;
use board_detail::BoardDetailPane;
use notifications_prefs::NotificationsPrefsPane;
use voice_prefs::VoicePrefsPane;
use prompts::{PromptPage, PromptsPane};
use skills::{SkillPage, SkillsPane};
use repositories::RepositoriesPane;
use sessions::SessionsPane;
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
    /// EXP-630: how issues behave team-wide — the estimate scale and the PR
    /// automation card; Labels and Statuses nest under it in the nav.
    /// Member-visible (the scale control itself is owner-only).
    Issues,
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
    /// #NEW-37: the Boards page (settings-nav.json `all-boards`): the live
    /// boards, then the archived ones (Unarchive) and the 48h trash
    /// (Restore). Owner-only: archived and trashed boards are out of the
    /// boards shape (EXP-500) and only owners can read them over tRPC.
    AllBoards,
    Repositories,
    /// #EXP-1616: one repository's page (repo-page.json: Pull requests,
    /// Runs, Branches, Settings); the payload is the repository id (or
    /// `name:<name>`, the dev drive). The Repositories row stays lit.
    Repository(String),
    /// EXP-771: the team's feedback widgets — a READ-ONLY list (name,
    /// submission count, disabled state) plus the "Manage on the web"
    /// hand-off. Owner-only, like the web's `canManageWidgets` gate and the
    /// `widgets.list` router behind it.
    Widget,
    /// VAPP-185 (`settings-mcp.json`): Settings › MCP, the merged page —
    /// Endpoint, Servers (EXP-792/EXP-807: the registry plus the person's OWN
    /// connection to each), Clients, Access, Built-in tools. Member-visible;
    /// the owner-only controls are gated inside the pane.
    Mcp,
    /// VAPP-185: the MCP page's Activity sub-page (the `mcp_calls` log).
    /// The MCP nav row stays lit on it.
    McpActivity,
    /// This-device tools (EXP-288: renamed from "Coding" — repos root,
    /// branch prefix, terminal shell).
    Tools,
    /// The per-agent-CLI launcher settings + doctor (EXP-288: split out of
    /// the old Coding pane; EXP-1294 renamed it from `Agents` — "Agents" are
    /// the team's agents now).
    AgentClis,
    /// NEW-6: the Agents index (web `settings/agents`): the team's agents
    /// over the team's Events. The Agents group's LABEL opens it.
    /// Member-visible.
    Agents,
    /// EXP-1294: one team agent's settings page (the Agents group lists every
    /// agent root as its own nav entry; the payload is the agent's session
    /// id). Member-visible: every member manages agents, only Delete is the
    /// owner's (`agent-settings.json`).
    Agent(String),
    /// EXP-1319: the agent page in CREATE mode (web `settings/agents/new`) —
    /// the Agents group's "New agent" row and the rail's `+`. Owner-only.
    NewAgent,
    /// VAPP-110: one app's settings (the payload is the `apps` row id or a
    /// slug). VAPP-177: no page of its own any more: selecting it opens the
    /// app's EDITOR on its Settings tab (the dev `app:<slug>` drive).
    App(String),
    /// NEW-6: the team's saved prompts (web `settings/prompts`) — the
    /// Agents group's Prompts entry. Member-visible (members read them);
    /// writes are owner-only inside the pane.
    Prompts,
    /// NEW-6: one prompt's page (web `settings/prompts/$promptId`); the
    /// payload is the prompt id. Member-visible, read-only for members.
    Prompt(String),
    /// NEW-6: the prompt page in CREATE mode (web `settings/prompts/new`).
    /// Owner-only.
    NewPrompt,
    /// #EXP-1660: the team's skill library (skills.json `list`) — the Agents
    /// group's row after MCP. Member-visible; writes are owner-only.
    Skills,
    /// #EXP-1660: one skill's page (the payload is the skill id).
    /// Member-visible, read-only for members.
    Skill(String),
    /// #EXP-1660: the skill page in CREATE mode. Owner-only.
    NewSkill,
    LocalRepos,
    /// EXP-886: how long this machine keeps finished runs' transcripts and
    /// resume records (`sessionRetentionDays`). Never gated.
    Sessions,
    /// EXP-238: identity + timezone (the old Account screen's head — the
    /// notification prefs split into [`SettingsSection::Notifications`]).
    /// Never gated.
    Account,
    /// EXP-238: email/push notification prefs as their own section (web
    /// parity: `settings/notifications`). Never gated.
    Notifications,
    /// NEW-67: the member's voice settings (web `settings/voice`). Never
    /// gated.
    Voice,
    /// EXP-238: personal `expu_` API keys — list/mint/revoke. EXP-862
    /// relabelled the nav entry "Security" (web `/settings/security`); the
    /// variant keeps its name, and so does the `settings-api` icon.
    ApiKeys,
    /// EXP-262: version + third-party licence notices. Since EXP-238 an
    /// ordinary Personal-group item in [`PERSONAL_NAV`] — it sits LAST, so the
    /// fallback scan (first visible item) still never lands on it. Never
    /// gated.
    About,
}

struct NavItem {
    /// The page id in `settings-nav.json` (`nav.groups[].items`, `rows`,
    /// `pages`).
    id: &'static str,
    label: &'static str,
    section: SettingsSection,
    /// EXP-630: sub-pages folded under this entry (Issues → Labels,
    /// Statuses) — the fixture's `nav.children`.
    children: &'static [NavItem],
}

impl NavItem {
    const fn leaf(id: &'static str, label: &'static str, section: SettingsSection) -> Self {
        Self {
            id,
            label,
            section,
            children: &[],
        }
    }
}

/// Every nav entry in order, sub-pages right after their parent (the web's
/// `flattenSettingsNav`): the team nav, then the personal nav — what the
/// fallback scan (first visible item: General / Members) and the tests walk.
fn flat_nav_items() -> impl Iterator<Item = &'static NavItem> {
    [&TEAM_NAV, &PERSONAL_NAV]
        .into_iter()
        .flat_map(|group| group.items)
        .flat_map(|item| std::iter::once(item).chain(item.children.iter()))
}

/// #EXP-1674: the section a `settings/<section>` link names: a nav page's id
/// (`settings-nav.json`), a This device page's ([`DEVICE_PAGES`]) or a web
/// route the desktop folds into one. `None` = no such page here (Billing,
/// Import): the browser's. Which nav it opens in = [`section_scope`]
/// ([`open_settings_section`]).
pub(crate) fn section_for_link(section: &str) -> Option<SettingsSection> {
    match section {
        "agents" => Some(SettingsSection::Agents),
        // The Voice section resolves to the viewer's Assistant page.
        "assistant" | "voice" => Some(SettingsSection::Voice),
        "mcp-servers" => Some(SettingsSection::Mcp),
        _ => flat_nav_items()
            .find(|item| item.id == section)
            .map(|item| item.section.clone())
            .or_else(|| DEVICE_PAGES.iter().find(|(id, _, _)| *id == section).map(|(_, _, page)| page.clone())),
    }
}

struct NavGroup {
    /// The fixture's group id (`desktopNavs.team[].id`, `.personal[].id`).
    id: &'static str,
    label: &'static str,
    items: &'static [NavItem],
}

/// #EXP-2027 THE SPLIT (`settings-nav.json` `nav.desktopNavs`, the test
/// replays `desktopNavCases`): there is no current team, so Settings is TWO
/// navs. [`TEAM_NAV`] = a team row's menu › Settings (`settings_team` =
/// Some): that team's pages only (Plan & Billing and Import are web-only);
/// [`PERSONAL_NAV`] = the gear next to the member's name (`settings_team` =
/// None): only what is theirs. The Boards, Agents and Apps groups LEFT
/// Settings (a board's, agent's or app's own settings open from its sidebar
/// row; their pages still render here, no row lit), and so did This device
/// ([`DEVICE_PAGES`], the device page's). Order defines both the nav and the
/// fallback scan (first visible item).
const TEAM_NAV: NavGroup = NavGroup {
    id: "team",
    label: "Team",
    items: &[
        NavItem::leaf("general", "General", SettingsSection::General),
        NavItem::leaf("members", "Members", SettingsSection::Members),
        // EXP-630: Issues with Labels and Statuses as its sub-pages.
        NavItem {
            id: "issues",
            label: "Issues",
            section: SettingsSection::Issues,
            children: &[
                NavItem::leaf("labels", "Labels", SettingsSection::Labels),
                NavItem::leaf("statuses", "Statuses", SettingsSection::Statuses),
            ],
        },
        NavItem::leaf("repositories", "Repositories", SettingsSection::Repositories),
        NavItem::leaf("prompts", prompts::PROMPTS_TITLE, SettingsSection::Prompts),
        NavItem::leaf("mcp", mcp::SETTINGS_MCP.title, SettingsSection::Mcp),
        // #EXP-1660 (settings-nav.json): Skills right after MCP.
        NavItem::leaf("skills", domain::skills::TITLE, SettingsSection::Skills),
        NavItem::leaf("widget", "Widget", SettingsSection::Widget),
        NavItem::leaf("storage", "Storage", SettingsSection::Storage),
        NavItem::leaf("all-boards", "All boards", SettingsSection::AllBoards),
    ],
};

/// #EXP-2027: the personal nav (see [`TEAM_NAV`]).
const PERSONAL_NAV: NavGroup = NavGroup {
    id: "personal",
    label: "Personal",
    items: &[
        NavItem::leaf("account", "Account", SettingsSection::Account),
        // #EXP-1987: Settings › Assistant is an ACCOUNT page: the viewer's
        // ONE Assistant whichever team (it left the Agents group). `Voice`
        // resolves to its agent page (`first_agent_resolved`); the row wears
        // the brand mark.
        NavItem::leaf("assistant", ASSISTANT_PAGE, SettingsSection::Voice),
        NavItem::leaf("notifications", "Notifications", SettingsSection::Notifications),
        NavItem::leaf("security", "Security", SettingsSection::ApiKeys),
        NavItem::leaf("about", "About", SettingsSection::About),
    ],
};

/// #EXP-2027 (`settings-nav.json` `nav.desktopNavs.device`): the This device
/// pages, which LEFT Settings for the device page of the viewer's THIS
/// device — `(page id, label, section)` in the fixture's order. Never gated;
/// [`SettingsView`] still renders their panes and [`section_for_link`] still
/// resolves their ids.
pub(crate) const DEVICE_PAGES: &[(&str, &str, SettingsSection)] = &[
    ("tools", "Tools", SettingsSection::Tools),
    ("coding-clis", "Coding CLIs", SettingsSection::AgentClis),
    ("worktrees", "Worktrees", SettingsSection::LocalRepos),
    ("sessions", "Sessions", SettingsSection::Sessions),
];

/// #EXP-2027: which nav a settings page belongs to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SectionScope {
    /// The personal nav's (`settings_team` = None).
    Personal,
    /// A team's (`settings_team` = Some; none set = the fallback team).
    Team,
    /// Either: a page opened with its own team (a board's, an agent's, an
    /// app's: the sidebar sets `settings_team` to the item's team; an agent
    /// page may be the viewer's Assistant) or a This device page.
    Either,
}

/// #EXP-2027: [`SectionScope`] of `section`.
pub(crate) fn section_scope(section: &SettingsSection) -> SectionScope {
    use SettingsSection as S;
    match section {
        S::Account | S::Voice | S::Notifications | S::ApiKeys | S::About => SectionScope::Personal,
        S::Board(_) | S::Agent(_) | S::App(_) | S::Tools | S::AgentClis | S::LocalRepos | S::Sessions => SectionScope::Either,
        S::General
        | S::Members
        | S::Issues
        | S::Labels
        | S::Statuses
        | S::Storage
        | S::AllBoards
        | S::Repositories
        | S::Repository(_)
        | S::Widget
        | S::Mcp
        | S::McpActivity
        | S::Agents
        | S::NewAgent
        | S::Prompts
        | S::Prompt(_)
        | S::NewPrompt
        | S::Skills
        | S::Skill(_)
        | S::NewSkill => SectionScope::Team,
    }
}

/// #EXP-2027: the `settings_team` a page shows under, given the one set
/// (`current`): a personal page none, a team page `current` else the
/// fallback team (`fallback`), any other page `current`.
fn scoped_team(scope: SectionScope, current: Option<String>, fallback: impl FnOnce() -> Option<String>) -> Option<String> {
    match scope {
        SectionScope::Personal => None,
        SectionScope::Team => current.or_else(fallback),
        SectionScope::Either => current,
    }
}

/// #EXP-2027: the scope a frame draws: the SELECTED page's (`Voice` is
/// personal before it resolves to the Assistant's agent page), else the
/// shown one's (a stale board page falls back to a team page).
fn nav_scope(selected: &SettingsSection, effective: &SettingsSection) -> SectionScope {
    match section_scope(selected) {
        SectionScope::Either => section_scope(effective),
        scope => scope,
    }
}

/// #EXP-2027: open Settings on `section` in the right nav: a personal page
/// in the personal nav, a team page in `team`'s (`None` = the fallback
/// team), any other page under `team` when given (else the team set now).
/// What a `settings/<id>` link, a notification or a checklist step calls.
pub(crate) fn open_settings_section(window: &mut Window, cx: &mut App, section: SettingsSection, team: Option<String>) {
    let current = nav_for_window(window, cx).read(cx).settings_team.clone();
    let team = scoped_team(section_scope(&section), team.or(current), || crate::navigation::fallback_team_id(cx));
    crate::navigation::open_settings(window, cx, team);
    select_settings_section(window, cx, section);
}

#[cfg(test)]
/// The desktop navs for a viewer: (`personal`, `team`, `device`) page ids,
/// the visible top-level pages (children folded under their parent).
/// Locked by `settings-nav.json` `desktopNavCases` (platform desktop).
fn desktop_nav_model(owner: bool) -> (Vec<&'static str>, Vec<&'static str>, Vec<&'static str>) {
    let visible = |group: &NavGroup| -> Vec<&'static str> {
        group.items.iter().filter(|item| section_visible(&item.section, owner)).map(|item| item.id).collect()
    };
    (
        visible(&PERSONAL_NAV),
        visible(&TEAM_NAV),
        DEVICE_PAGES.iter().filter(|(_, _, section)| section_visible(section, owner)).map(|(id, _, _)| *id).collect(),
    )
}

/// #EXP-1987: the Personal group's Assistant page label =
/// `agent-settings.json` `assistant.copy.title` (a test locks the two).
pub(crate) const ASSISTANT_PAGE: &str = "Assistant";

/// Web nav `visible` gating: General/board pages are owner-only.
/// Repositories is member-visible since EXP-557 (per-user repo sharing:
/// every member connects their own GitHub and shares repos there).
/// EXP-771: the Features group is owner-only — the web's `canManageWidgets`
/// is `isOwner`, and `widgets.list`/`teams.update` are owner-gated servers.
fn section_visible(section: &SettingsSection, owner: bool) -> bool {
    match section {
        SettingsSection::General
        | SettingsSection::Storage
        | SettingsSection::Widget
        | SettingsSection::NewAgent
        | SettingsSection::NewPrompt
        | SettingsSection::NewSkill => owner,
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
    let Some(team_id) = screen_team_id(nav, cx) else {
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

/// EXP-1294: [`valid_board_ids`]' twin for a selected `Agent(id)`: NEW-2:
/// the synced agent DEFINITIONS of the active team, `None` while the agents
/// collection is still loading.
fn valid_agent_ids(
    cx: &App,
    nav: &Entity<Navigation>,
) -> Option<std::collections::HashSet<String>> {
    if !Store::global(cx).collections().agents.read(cx).is_ready() {
        return None;
    }
    let Some(team_id) = screen_team_id(nav, cx) else {
        return Some(crate::session_data::my_assistant_anywhere(None, cx).map(|agent| agent.id).into_iter().collect());
    };
    Some(
        crate::session_data::agents_in_team(&team_id, cx)
            .into_iter()
            .map(|agent| agent.agent_id)
            // #EXP-1987: the account page's Assistant, whatever its team.
            .chain(crate::session_data::my_assistant_anywhere(Some(&team_id), cx).map(|agent| agent.id))
            .collect(),
    )
}

/// VAPP-110: the apps of the active team (a selected `App(id)` clamps to
/// them), `None` while the apps collection loads.
fn valid_app_ids(
    cx: &App,
    nav: &Entity<Navigation>,
) -> Option<std::collections::HashSet<String>> {
    if !Store::global(cx).collections().apps.read(cx).is_ready() {
        return None;
    }
    let Some(team_id) = screen_team_id(nav, cx) else {
        return Some(Default::default());
    };
    Some(crate::apps_view::app_entries(&team_id, cx).into_iter().map(|app| app.id).collect())
}

/// EXP-1294: `Agent("")` (the DEV `EXP_DEV_SETTINGS=agent` seed) = the
/// active team's first agent, or the empty Agents pane while it has none.
fn first_agent_resolved(
    selected: SettingsSection,
    nav: &Entity<Navigation>,
    cx: &App,
) -> SettingsSection {
    match selected {
        SettingsSection::Agent(id) if id.is_empty() => screen_team_id(nav, cx)
            .and_then(|team_id| crate::session_data::agents_in_team(&team_id, cx).into_iter().next())
            .map(|agent| SettingsSection::Agent(agent.agent_id))
            .unwrap_or(SettingsSection::Agent(id)),
        // S-3: the Voice page left the nav; its block is the Assistant
        // page's Voice section (the tray / meeting links land there).
        // #EXP-1987: Personal's `assistant` page = the viewer's ONE
        // Assistant whichever team is current.
        SettingsSection::Voice => crate::session_data::my_assistant_anywhere(screen_team_id(nav, cx).as_deref(), cx)
            .map(|assistant| SettingsSection::Agent(assistant.id))
            .unwrap_or(SettingsSection::Voice),
        // VAPP-110: `App("")` = the team's first app; a slug (the dev
        // `app:<slug>`) names its app.
        SettingsSection::App(id) => {
            let apps = screen_team_id(nav, cx)
                .map(|team_id| crate::apps_view::app_entries(&team_id, cx))
                .unwrap_or_default();
            let hit = if id.is_empty() {
                apps.first()
            } else {
                apps.iter().find(|app| app.id == id || app.slug == id)
            };
            match hit {
                Some(app) => SettingsSection::App(app.id.clone()),
                None => SettingsSection::App(id),
            }
        }
        other => other,
    }
}

/// Whether the static nav row for `item` is lit while `effective` shows.
/// NEW-6: the Prompts row stays lit on one prompt's page and on New prompt
/// (web: the `/settings/prompts` entry matches its sub-routes).
fn nav_lit(item: &SettingsSection, effective: &SettingsSection) -> bool {
    match item {
        // VAPP-185: the MCP row stays lit on its Activity page.
        SettingsSection::Mcp => {
            matches!(effective, SettingsSection::Mcp | SettingsSection::McpActivity)
        }
        SettingsSection::Prompts => matches!(
            effective,
            SettingsSection::Prompts | SettingsSection::Prompt(_) | SettingsSection::NewPrompt
        ),
        SettingsSection::Skills => matches!(
            effective,
            SettingsSection::Skills | SettingsSection::Skill(_) | SettingsSection::NewSkill
        ),
        SettingsSection::Repositories => {
            matches!(effective, SettingsSection::Repositories | SettingsSection::Repository(_))
        }
        _ => item == effective,
    }
}

/// The selected section, clamped to what's visible. Clamped at render time —
/// never mutated — so a membership change that hides the selection falls back
/// (to Members, the first never-gated item) and restores it if ownership
/// returns. A selected `Board(id)` additionally requires the board to still
/// exist in the active team (`board_ok`) — a trashed board falls back; a
/// selected `Agent(id)` likewise its agent (`agent_ok`, EXP-1294).
fn effective_selection(
    selected: SettingsSection,
    owner: bool,
    board_ok: impl Fn(&str) -> bool,
    agent_ok: impl Fn(&str) -> bool,
) -> SettingsSection {
    let visible = section_visible(&selected, owner)
        && match &selected {
            SettingsSection::Board(id) => board_ok(id),
            // VAPP-110: an app page shares the agent check's closure shape
            // (`agent_ok` answers for every id-keyed team page).
            SettingsSection::Agent(id) | SettingsSection::App(id) => agent_ok(id),
            _ => true,
        };
    if visible {
        return selected;
    }
    flat_nav_items()
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
    /// EXP-630: estimate scale + PR automation — un-gated (member-visible).
    issues: Entity<IssuesPane>,
    labels: Entity<LabelsPane>,
    /// EXP-314 per-team issue statuses — un-gated (member-editable).
    statuses: Entity<StatusesPane>,
    /// EXP-297 owner-only attachment manager (fetch-on-open server read).
    storage: Entity<StoragePane>,
    /// #NEW-37 owner-only Boards page: live boards + the archived and trashed
    /// ones (fetch-on-open server reads, those two are not synced).
    all_boards: Entity<AllBoardsPane>,
    /// EXP-288: ONE per-board detail pane — it reads the selected `Board(id)`
    /// from the shared selection itself and re-points at board switches.
    board_detail: Entity<BoardDetailPane>,
    /// EXP-1294: ONE per-agent detail pane, the board pane's twin.
    agent_detail: Entity<AgentDetailPane>,
    /// EXP-1319: the New agent page, built FRESH on every entry (and per
    /// team) so a left-behind draft never reappears.
    new_agent: Option<(String, Entity<NewAgentPane>)>,
    /// NEW-6: the Agents index (the agents + the Events band).
    agents_index: Entity<agents_index::AgentsIndexPane>,
    /// NEW-6: the Prompts list (live off the synced `prompts` shape).
    prompts: Entity<PromptsPane>,
    /// NEW-6: the open prompt page, keyed by its prompt id (`new:<team>` in
    /// create mode) — rebuilt on every switch so a draft never leaks into
    /// another prompt; a create keeps the same page as the new prompt's.
    prompt_page: Option<(String, Entity<PromptPage>)>,
    /// #EXP-1660: the Skills list (live off the synced `skills` shape) and
    /// the open skill page, keyed like `prompt_page`.
    skills: Entity<SkillsPane>,
    skill_page: Option<(String, Entity<SkillPage>)>,
    repositories: Entity<RepositoriesPane>,
    /// #EXP-1616: the open repo page, keyed by its repository key (rebuilt
    /// on every switch).
    repo_page: Option<(String, Entity<repo_page::RepoPage>)>,
    /// EXP-771 owner-only widget list (fetch-on-open server read —
    /// `widget_configs` is never synced).
    widget: Entity<WidgetPane>,
    /// VAPP-185 Settings › MCP (fetch-on-open server reads: `mcp_servers`
    /// and the MCP clients/access/calls never sync) and its Activity page.
    mcp: Entity<McpPane>,
    mcp_activity: Entity<McpActivityPane>,
    /// This-device tools (EXP-288: repos root, branch prefix, terminal
    /// shell) — local per-install state, so NOT owner-gated.
    tools: Entity<ToolsPane>,
    /// Per-agent-CLI launcher settings + doctor (EXP-288: split out of Tools).
    agent_clis: Entity<AgentsPane>,
    /// §4.7 desktop-only Worktrees section (clone disk usage +
    /// prune/remove) — local per-install state, un-gated.
    local_repos: Entity<LocalReposPane>,
    /// EXP-886: the session history window — local per-install, un-gated.
    sessions: Entity<SessionsPane>,
    /// EXP-238: identity + timezone (the old Account screen, folded in).
    account: Entity<AccountPane>,
    /// EXP-238: email/push notification prefs — their own section now.
    notifications: Entity<NotificationsPrefsPane>,
    voice: Entity<VoicePrefsPane>,
    /// EXP-238: personal `expu_` API keys (fetch-on-open server read).
    api_keys: Entity<ApiKeysPane>,
    /// EXP-262: version + third-party licence notices (stateless, un-gated).
    about: Entity<AboutPane>,
    /// EXP-771: the ONE scroll offset of the detail column — the header +
    /// pane share a scroll region now, so the handle lives here and resets to
    /// the top on every section switch (the web's route change does the same).
    shell: shell::DetailShell,
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
        let issues = cx.new(|cx| IssuesPane::new(nav.clone(), window, cx));
        let labels = cx.new(|cx| LabelsPane::new(nav.clone(), window, cx));
        let statuses = cx.new(|cx| StatusesPane::new(nav.clone(), window, cx));
        let storage = cx.new(|cx| StoragePane::new(nav.clone(), window, cx));
        let all_boards = cx.new(|cx| AllBoardsPane::new(nav.clone(), window, cx));
        let board_detail =
            cx.new(|cx| BoardDetailPane::new(nav.clone(), shared.clone(), window, cx));
        let agent_detail =
            cx.new(|cx| AgentDetailPane::new(nav.clone(), shared.clone(), window, cx));
        let repositories = cx.new(|cx| RepositoriesPane::new(nav.clone(), window, cx));
        let prompts = cx.new(|cx| PromptsPane::new(nav.clone(), window, cx));
        let skills = cx.new(|cx| SkillsPane::new(nav.clone(), window, cx));
        let agents_index = cx.new(|cx| agents_index::AgentsIndexPane::new(nav.clone(), window, cx));
        let widget = cx.new(|cx| WidgetPane::new(nav.clone(), window, cx));
        let mcp = cx.new(|cx| McpPane::new(nav.clone(), window, cx));
        let mcp_activity = cx.new(|cx| McpActivityPane::new(nav.clone(), window, cx));
        let tools = cx.new(|cx| ToolsPane::new(window, cx));
        let agent_clis = cx.new(|cx| AgentsPane::new(window, cx));
        let local_repos = cx.new(|cx| LocalReposPane::new(window, cx));
        let sessions = cx.new(|cx| SessionsPane::new(window, cx));
        let account = cx.new(|cx| AccountPane::new(nav.clone(), window, cx));
        let notifications = cx.new(|cx| NotificationsPrefsPane::new(window, cx));
        let voice = cx.new(|cx| VoicePrefsPane::new(window, cx));
        let api_keys = cx.new(|cx| ApiKeysPane::new(window, cx));
        let about = cx.new(|cx| AboutPane::new(window, cx));

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
            // EXP-1294: … and a deleted agent a selected `Agent(id)`.
            cx.observe(&collections.sessions, |_, _, cx| cx.notify()),
            cx.observe(&collections.agents, |_, _, cx| cx.notify()),
            // NEW-6: a deleted prompt drops its page back to the list.
            cx.observe(&collections.prompts, |_, _, cx| cx.notify()),
            // #EXP-1660: … and a deleted skill its page.
            cx.observe(&collections.skills, |_, _, cx| cx.notify()),
        ];

        Self {
            nav,
            general,
            members,
            issues,
            labels,
            statuses,
            storage,
            all_boards,
            board_detail,
            agent_detail,
            new_agent: None,
            agents_index,
            prompts,
            prompt_page: None,
            skills,
            skill_page: None,
            repositories,
            repo_page: None,
            widget,
            mcp,
            mcp_activity,
            tools,
            agent_clis,
            local_repos,
            sessions,
            account,
            notifications,
            voice,
            api_keys,
            about,
            shell: shell::DetailShell::new(window, cx),
            last_section: None,
            shared,
            _subscriptions: subscriptions,
        }
    }

    /// NEW-6: one prompt's page. Reuses the open page while it shows this
    /// prompt (a create re-points its own page here); otherwise builds one
    /// off the synced row. The skeleton while the shape syncs; the list once
    /// the prompt is gone (web: a deleted prompt lands on the list).
    fn prompt_pane(
        &mut self,
        prompt_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> PaneBuild {
        if let Some((_, page)) = self.prompt_page.as_ref() {
            if page.read(cx).prompt_id() == Some(prompt_id) {
                if page.read(cx).is_gone(cx) {
                    self.prompt_page = None;
                    return pane_of(&self.prompts);
                }
                let page = page.clone();
                self.prompt_page = Some((prompt_id.to_string(), page.clone()));
                return pane_of(&page);
            }
        }
        let collections = Store::global(cx).collections();
        let row = collections
            .prompts
            .read(cx)
            .get(prompt_id)
            .map(api::prompts::from_row);
        match row {
            Some(prompt) => {
                let team_id = prompt.team_id.clone();
                let page = cx.new(|cx| PromptPage::new(team_id, Some(prompt), window, cx));
                self.prompt_page = Some((prompt_id.to_string(), page.clone()));
                pane_of(&page)
            }
            None if !collections.prompts.read(cx).is_ready() => {
                std::rc::Rc::new(|_, cx| settings_skeleton("prompt-page-skeleton", cx))
            }
            None => pane_of(&self.prompts),
        }
    }

    /// #EXP-1660: one skill's page (the prompt page's rules: reuse the open
    /// page, build one off the synced row, the skeleton while the shape
    /// syncs, the list once the skill is gone).
    fn skill_pane(
        &mut self,
        skill_id: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> PaneBuild {
        if let Some((_, page)) = self.skill_page.as_ref() {
            if page.read(cx).skill_id() == Some(skill_id) {
                if page.read(cx).is_gone(cx) {
                    self.skill_page = None;
                    return pane_of(&self.skills);
                }
                let page = page.clone();
                self.skill_page = Some((skill_id.to_string(), page.clone()));
                return pane_of(&page);
            }
        }
        let collections = Store::global(cx).collections();
        let row = collections.skills.read(cx).get(skill_id).map(api::skills::from_row);
        match row {
            Some(skill) => {
                let team_id = skill.team_id.clone();
                let page = cx.new(|cx| SkillPage::new(team_id, Some(skill), window, cx));
                self.skill_page = Some((skill_id.to_string(), page.clone()));
                pane_of(&page)
            }
            None if !collections.skills.read(cx).is_ready() => std::rc::Rc::new(|_, cx| settings_skeleton("skill-page-skeleton", cx)),
            None => pane_of(&self.skills),
        }
    }

    /// EXP-369 (re-homed by EXP-238): the Personal panes hold server-only
    /// reads (timezone, email prefs, API keys) that would otherwise show the
    /// first visit's snapshot forever — every transition INTO the settings
    /// screen drops their caches; each pane refetches on its next render.
    pub fn mark_personal_stale(&mut self, cx: &mut gpui::Context<Self>) {
        self.account.update(cx, |pane, cx| pane.mark_stale(cx));
        self.notifications.update(cx, |pane, cx| pane.mark_stale(cx));
        self.voice.update(cx, |pane, cx| pane.mark_stale(cx));
        self.api_keys.update(cx, |pane, cx| pane.mark_stale(cx));
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "SettingsView").entered();
        let owner = screen_team_id(&self.nav, cx)
            .map(|ws| is_owner(cx, &ws))
            .unwrap_or(false);
        // Web settings layout route (EXP-146): grouped left nav + one
        // selected section pane in the detail column. EXP-282: the nav is
        // the window's left column now ([`SettingsNavPanel`]) — this view is
        // the detail column alone.
        // #EXP-1987: Settings › Assistant is the ACCOUNT's page: it asks
        // `agents.ensureAssistant` WITHOUT a team (once per account).
        if self.shared.read(cx).settings_section() == SettingsSection::Voice {
            crate::session_data::ensure_my_assistant(cx);
        }
        let board_ids = valid_board_ids(cx, &self.nav);
        let agent_ids = valid_agent_ids(cx, &self.nav);
        let app_ids = valid_app_ids(cx, &self.nav);
        let effective = effective_selection(
            first_agent_resolved(self.shared.read(cx).settings_section(), &self.nav, cx),
            owner,
            |id| board_ids.as_ref().is_none_or(|ids| ids.contains(id)),
            |id| {
                id.is_empty()
                    || agent_ids.as_ref().is_none_or(|ids| ids.contains(id))
                    || app_ids.as_ref().is_none_or(|ids| ids.contains(id))
            },
        );

        // #EXP-2027: the page shows under its own nav: a personal page
        // under none, a team page under a team (the fallback team when
        // none is set: a link, a notification, an old caller) — the nav
        // column follows the same rule ([`nav_scope`]).
        let current = self.nav.read(cx).settings_team.clone();
        let wanted = scoped_team(nav_scope(&self.shared.read(cx).settings_section(), &effective), current.clone(), || {
            crate::navigation::fallback_team_id(cx)
        });
        if wanted != current {
            let nav = self.nav.clone();
            cx.defer(move |cx| {
                nav.update(cx, |nav, cx| {
                    nav.settings_team = wanted;
                    cx.notify();
                })
            });
        }

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
                SettingsSection::Voice => self.voice.update(cx, |pane, cx| pane.mark_stale(cx)),
                SettingsSection::ApiKeys => {
                    self.api_keys.update(cx, |pane, cx| pane.mark_stale(cx))
                }
                // The widget list is tRPC too, and its one affordance sends
                // the user to the web to change it — so coming back must see
                // the change.
                SettingsSection::Widget => {
                    self.widget.update(cx, |pane, cx| pane.mark_stale(cx))
                }
                // Same reason, plus one of its own: a connection made on the
                // web or from the CLI has to show on re-entry.
                SettingsSection::Mcp => self.mcp.update(cx, |pane, cx| pane.mark_stale(cx)),
                SettingsSection::McpActivity => {
                    self.mcp_activity.update(cx, |pane, cx| pane.mark_stale(cx))
                }
                SettingsSection::NewAgent => self.new_agent = None,
                // A fresh create page on every entry (web: a new route).
                SettingsSection::NewPrompt => self.prompt_page = None,
                SettingsSection::NewSkill => self.skill_page = None,
                _ => {}
            }
            // EXP-771: one scroll region for header + pane, so a section
            // switch has to rewind it — the web's route change lands at the
            // top of the page, never mid-list.
            self.shell.rewind();
            self.last_section = Some(effective.clone());
        }

        let pane: PaneBuild = match &effective {
            SettingsSection::General => pane_of(&self.general),
            SettingsSection::Members => pane_of(&self.members),
            SettingsSection::Issues => pane_of(&self.issues),
            SettingsSection::Labels => pane_of(&self.labels),
            SettingsSection::Statuses => pane_of(&self.statuses),
            SettingsSection::Storage => pane_of(&self.storage),
            SettingsSection::AllBoards => pane_of(&self.all_boards),
            // The pane reads the selected board id from the shared selection
            // itself (it needs to flush a pending rename on board switches).
            SettingsSection::Board(_) => pane_of(&self.board_detail),
            SettingsSection::Agent(_) => pane_of(&self.agent_detail),
            SettingsSection::NewAgent => {
                let team_id = screen_team_id(&self.nav, cx).unwrap_or_default();
                if self.new_agent.as_ref().is_none_or(|(id, _)| *id != team_id) {
                    let pane = cx.new(|cx| NewAgentPane::new(team_id.clone(), window, cx));
                    self.new_agent = Some((team_id, pane));
                }
                self.new_agent
                    .as_ref()
                    .map(|(_, pane)| pane_of(pane))
                    .unwrap_or_else(empty_pane)
            }
            SettingsSection::Agents => pane_of(&self.agents_index),
            // VAPP-177: an app's settings = its editor's Settings tab (the
            // web's redirect): the selection resolved to a synced app hands
            // over to the editor (replace) and Settings falls back to the
            // Apps index; until the apps sync, nothing.
            SettingsSection::App(id) => {
                let team_id = screen_team_id(&self.nav, cx);
                if let Some(app) = crate::apps_view::find_app(id, team_id.as_deref(), cx) {
                    cx.defer_in(window, move |_, window, cx| {
                        select_settings_section(window, cx, SettingsSection::General);
                        crate::app_screen::open_app_tab(
                            window,
                            cx,
                            app.id,
                            domain::apps::EditorTab::Settings,
                            true,
                        );
                    });
                }
                empty_pane()
            }
            SettingsSection::Prompts => pane_of(&self.prompts),
            SettingsSection::NewPrompt => {
                let team_id = screen_team_id(&self.nav, cx).unwrap_or_default();
                let key = format!("new:{team_id}");
                if self.prompt_page.as_ref().is_none_or(|(k, _)| *k != key) {
                    let page = cx.new(|cx| PromptPage::new(team_id, None, window, cx));
                    self.prompt_page = Some((key, page));
                }
                self.prompt_page
                    .as_ref()
                    .map(|(_, page)| pane_of(page))
                    .unwrap_or_else(empty_pane)
            }
            SettingsSection::Prompt(prompt_id) => self.prompt_pane(prompt_id, window, cx),
            SettingsSection::Skills => pane_of(&self.skills),
            SettingsSection::NewSkill => {
                let team_id = screen_team_id(&self.nav, cx).unwrap_or_default();
                let key = format!("new:{team_id}");
                if self.skill_page.as_ref().is_none_or(|(k, _)| *k != key) {
                    let page = cx.new(|cx| SkillPage::new(team_id, None, window, cx));
                    self.skill_page = Some((key, page));
                }
                self.skill_page
                    .as_ref()
                    .map(|(_, page)| pane_of(page))
                    .unwrap_or_else(empty_pane)
            }
            SettingsSection::Skill(skill_id) => self.skill_pane(skill_id, window, cx),
            SettingsSection::Repositories => pane_of(&self.repositories),
            SettingsSection::Repository(key) => {
                if self.repo_page.as_ref().is_none_or(|(k, _)| k != key) {
                    let nav = self.nav.clone();
                    let page = cx.new(|cx| repo_page::RepoPage::new(nav, key.clone(), window, cx));
                    self.repo_page = Some((key.clone(), page));
                }
                self.repo_page.as_ref().map(|(_, page)| pane_of(page)).unwrap_or_else(empty_pane)
            }
            SettingsSection::Widget => pane_of(&self.widget),
            SettingsSection::Mcp => pane_of(&self.mcp),
            SettingsSection::McpActivity => pane_of(&self.mcp_activity),
            SettingsSection::Tools => pane_of(&self.tools),
            SettingsSection::AgentClis => pane_of(&self.agent_clis),
            SettingsSection::LocalRepos => pane_of(&self.local_repos),
            SettingsSection::Sessions => pane_of(&self.sessions),
            SettingsSection::Account => pane_of(&self.account),
            SettingsSection::Notifications => pane_of(&self.notifications),
            SettingsSection::Voice => pane_of(&self.voice),
            SettingsSection::ApiKeys => pane_of(&self.api_keys),
            SettingsSection::About => pane_of(&self.about),
        };

        self.shell.render(SharedString::from(format!("settings-detail-{effective:?}")), pane, window, cx)
    }
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
    /// EXP-630: fold state of the nested entries, keyed by label. Unset =
    /// unfolded while the parent or one of its sub-pages is selected.
    folded: std::collections::HashMap<&'static str, bool>,
    #[cfg(test)]
    shot: Option<NavShot>,
    paint: shell::NavPaint,
    /// What each row's key does (this frame's).
    routes: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, shell::NavAction>>>,
    list_binding: Entity<eui_gpui::binding::Binding<serde_json::Value>>,
    list_view: Entity<eui_gpui::view::SurfaceView>,
    back_view: Entity<eui_gpui::view::SurfaceView>,
    /// #EXP-2027: the header's state (`← Settings`, or `←` + the team's mark
    /// and name).
    back_binding: Entity<eui_gpui::binding::Binding<serde_json::Value>>,
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
            // A stale board / agent / app page clamps back (the lit row).
            cx.observe(&collections.boards, |_, _, cx| cx.notify()),
            cx.observe(&collections.sessions, |_, _, cx| cx.notify()),
            cx.observe(&collections.agents, |_, _, cx| cx.notify()),
            cx.observe(&collections.apps, |_, _, cx| cx.notify()),
        ];
        let routes: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, shell::NavAction>>> = Default::default();
        let handle = window.window_handle();
        let panel = cx.weak_entity();
        let list_binding = cx.new(|_| {
            let mut binding = eui_gpui::binding::Binding::new("/nav", serde_json::json!({"items": []}), |state: &serde_json::Value, _| state.clone());
            let routes = routes.clone();
            binding.on_action("pick", move |_, event, cx| {
                let key = event.context.get("key").and_then(serde_json::Value::as_str).unwrap_or_default();
                let action = routes.borrow().get(key).cloned()?;
                let panel = panel.clone();
                // Deferred: the binding is mid-update.
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, cx| nav_dispatch(action, &panel, window, cx));
                });
                None
            });
            binding
        });
        let back_nav = nav.clone();
        let back_binding = cx.new(|_| {
            let mut binding = eui_gpui::binding::Binding::new("/back", shell::back_state(None), |state: &serde_json::Value, _| state.clone());
            binding.on_action("back", move |_, _, cx| {
                let nav = back_nav.clone();
                // EXP-456: the back affordance — direct calls, not action
                // dispatch (EXP-17).
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, cx| {
                        if nav.read(cx).can_go_back() {
                            go_back(window, cx);
                        } else {
                            set_screen(window, cx, None);
                        }
                    });
                });
                None
            });
            binding
        });
        let paint = shell::NavPaint::of(cx);
        let plugin = || -> std::rc::Rc<dyn eui_gpui::host::HostPlugin> { std::rc::Rc::new(shell::NavPlugin { inner: doc::ide_plugin() }) };
        let list_view = doc::mount_with("settings-nav", shell::nav_document(&paint), &list_binding, plugin(), false, window, cx);
        let back_view = doc::mount_with("settings-nav-back", shell::back_document(&paint), &back_binding, plugin(), false, window, cx);
        Self {
            nav,
            shared,
            paint,
            routes,
            list_binding,
            list_view,
            back_view,
            back_binding,
            folded: Default::default(),
            #[cfg(test)]
            shot: None,
            _subscriptions: subscriptions,
        }
    }

}

/// The data one nav frame draws: who looks (owner gating), whose nav
/// (#EXP-2027: `team` = None is the personal nav), the lit section.
pub(crate) struct NavInputs {
    pub(crate) owner: bool,
    pub(crate) team: Option<NavTeam>,
    /// The team the Assistant row resolves in (the frame's
    /// [`screen_team_id`], the SettingsView's lookup).
    pub(crate) scope_team: Option<String>,
    pub(crate) effective: SettingsSection,
}

/// #EXP-2027: the team whose settings are open — the header's mark + name.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NavTeam {
    pub(crate) id: String,
    pub(crate) name: String,
    /// The team image's src (`team-image.json` `team_mark`); None = the
    /// team glyph.
    pub(crate) image: Option<String>,
}

impl NavTeam {
    fn of(team: &domain::rows::Team) -> Self {
        let image = match domain::team_image::team_mark(team.icon_url.as_deref()) {
            domain::team_image::TeamMark::Image { src } => Some(src),
            domain::team_image::TeamMark::Glyph => None,
        };
        Self { id: team.id.clone(), name: team.name.clone(), image }
    }
}

/// The bench's nav (#EXP-1629): the inputs pinned instead of read off the
/// store (`selected` still clamps through [`effective_selection`]).
/// #EXP-2027: `team_id` Some = that team's nav (named after the synced row,
/// else the id), None = the personal nav. The board / agent / app lists
/// left the nav: the bench still pins them, nothing draws them.
#[cfg(test)]
#[allow(dead_code)]
pub(crate) struct NavShot {
    pub(crate) owner: bool,
    pub(crate) team_id: Option<String>,
    pub(crate) selected: SettingsSection,
    pub(crate) boards: Vec<domain::rows::Board>,
    pub(crate) agents: Vec<domain::session_tree::AgentRow>,
    pub(crate) apps: Vec<crate::apps_view::AppEntry>,
}

impl SettingsNavPanel {
    /// The bench's panel showing `shot`.
    #[cfg(test)]
    pub(crate) fn show(&mut self, shot: NavShot, cx: &mut gpui::Context<Self>) {
        self.shot = Some(shot);
        cx.notify();
    }

    /// The bench's chevron press: `label`'s sub-pages folded.
    #[cfg(test)]
    pub(crate) fn fold_for_shots(&mut self, label: &'static str, cx: &mut gpui::Context<Self>) {
        self.folded.insert(label, true);
        cx.notify();
    }

    fn inputs(&self, cx: &mut gpui::Context<Self>) -> NavInputs {
        #[cfg(test)]
        if let Some(shot) = self.shot.as_ref() {
            let effective = effective_selection(shot.selected.clone(), shot.owner, |_| true, |_| true);
            let team = shot.team_id.as_ref().map(|id| {
                Store::try_global(cx)
                    .and_then(|store| store.collections().teams.read(cx).get(id).map(NavTeam::of))
                    .unwrap_or_else(|| NavTeam { id: id.clone(), name: id.clone(), image: None })
            });
            return NavInputs { owner: shot.owner, team, scope_team: shot.team_id.clone(), effective };
        }
        let scope_team = screen_team_id(&self.nav, cx);
        let owner = scope_team.as_deref().map(|team_id| is_owner(cx, team_id)).unwrap_or(false);
        let board_ids = valid_board_ids(cx, &self.nav);
        let agent_ids = valid_agent_ids(cx, &self.nav);
        let app_ids = valid_app_ids(cx, &self.nav);
        let selected = self.shared.read(cx).settings_section();
        let effective = effective_selection(
            first_agent_resolved(selected.clone(), &self.nav, cx),
            owner,
            |id| board_ids.as_ref().is_none_or(|ids| ids.contains(id)),
            |id| {
                id.is_empty()
                    || agent_ids.as_ref().is_none_or(|ids| ids.contains(id))
                    || app_ids.as_ref().is_none_or(|ids| ids.contains(id))
            },
        );
        // #EXP-2027: the nav the page belongs to (the SettingsView moves
        // `settings_team` to match): a team nav reads the frame's team (the
        // one set, or the fallback while a team page waits for it).
        let team_nav = match nav_scope(&selected, &effective) {
            SectionScope::Personal => false,
            SectionScope::Team => true,
            SectionScope::Either => self.nav.read(cx).settings_team.is_some(),
        };
        let team = team_nav
            .then(|| scope_team.as_deref().and_then(|team_id| active_team_row(cx, team_id)))
            .flatten()
            .map(|team| NavTeam::of(&team));
        NavInputs { owner, team, scope_team, effective }
    }
}

/// The synced teams row of `team_id`.
fn active_team_row(cx: &App, team_id: &str) -> Option<domain::rows::Team> {
    Store::global(cx).collections().teams.read(cx).get(team_id).cloned()
}

impl Render for SettingsNavPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "SettingsNavPanel").entered();
        let inputs = self.inputs(cx);
        let (items, actions) = shell::nav_entries(&inputs, &self.folded, &self.paint, cx);
        *self.routes.borrow_mut() = actions;
        self.list_binding.update(cx, |binding, cx| binding.update_state(|state| *state = serde_json::json!({"items": items}), cx));
        let header = shell::back_state(inputs.team.as_ref());
        self.back_binding.update(cx, |binding, cx| {
            if *binding.state() != header {
                binding.update_state(|state| *state = header, cx);
            }
        });
        v_flex()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            // EXP-456: the nav takes the rail's slot, so it wears the rail's
            // exact material — which since EXP-723/EXP-767 is NOTHING: no
            // section wash, no ramp, no rounded corners. EXP-863: the
            // column's titlebar strip and its fixed header (team switcher,
            // Search, New issue) are the `Shell`'s, above this pane — the
            // nav starts at its back row.
            .text_color(cx.theme().sidebar_foreground)
            .child(self.back_view.clone())
            .child(div().id("settings-nav").flex_1().min_h_0().overflow_y_scroll().child(self.list_view.clone()))
    }
}

/// A nav row's press.
fn nav_dispatch(action: shell::NavAction, panel: &gpui::WeakEntity<SettingsNavPanel>, window: &mut Window, cx: &mut App) {
    match action {
        shell::NavAction::Section(section) => {
            select_settings_section(window, cx, section);
            navigate(window, cx, Screen::Settings);
        }
        shell::NavAction::Fold(label, folded) => {
            let _ = panel.update(cx, |panel, cx| {
                panel.folded.insert(label, folded);
                cx.notify();
            });
        }
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
    let team_id = screen_team_id(nav, cx)?;
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

/// Web `isMember` gate (`useTeamPermissions`: the signed-in user has a
/// `team_members` row of the team — v7's one capability gate, every
/// mutation affordance rides it). The chat strip's `+` and the Guide
/// menu read it (web chat route `permissions.isMember`).
pub(crate) fn is_member(cx: &App, team_id: &str) -> bool {
    my_membership(cx, team_id).is_some()
}

/// #EXP-1987 (agent-settings.json `assistant`: THE ASSISTANT'S CHAT IS THE
/// MEMBER'S OWN): [`is_member`] for a chat of `team_id` started by
/// `starter`: a member of the team, or the starter themselves in a team the
/// viewer does not sync (their hidden personal team: no teams row, no
/// `team_members` rows). Nothing counts as personal before the teams sync.
pub(crate) fn is_chat_member(cx: &App, team_id: &str, starter: Option<&str>) -> bool {
    if is_member(cx, team_id) {
        return true;
    }
    let me = queries::active_account(cx).map(|account| account.user_id);
    let team_ids = crate::session_data::synced_team_ids(cx);
    me.is_some() && starter == me.as_deref() && !team_ids.is_empty() && !team_ids.iter().any(|id| id == team_id)
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




/// The settings entity row's height (`flat_row` + `py_2` around a 24px
/// pill line): a section's FIRST load draws its skeleton rows at it
/// (EXP-1259 D8).
pub(crate) const SETTINGS_ROW_H: f32 = 40.;

/// A settings list's first-load placeholder: the skeleton rows in the
/// entity row's geometry (a refetch keeps its rows and spins the band).
pub(crate) fn settings_skeleton(id: impl Into<gpui::ElementId>, cx: &App) -> gpui::AnyElement {
    crate::feedback::skeleton_rows(id, crate::feedback::SKELETON_ROWS, SETTINGS_ROW_H, cx)
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
/// off the foreground thread. Only an `http(s)://` URL reaches the OS opener
/// (`api::opener::is_web_url`): every settings link is a web page, and a
/// server- or config-derived value must never launch a `file:` or
/// custom-scheme handler.
pub(crate) fn open_url(cx: &mut App, url: String) {
    if !api::opener::is_web_url(&url) {
        log::warn!("[ui] open-in-browser refused a non-http(s) URL");
        return;
    }
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
        let entry = flat_nav_items()
            .find(|item| item.section == SettingsSection::ApiKeys)
            .expect("the personal keys entry is in the nav");
        assert_eq!(entry.label, "Security");
        assert!(
            !flat_nav_items().any(|item| item.label == "API keys"),
            "the old label is gone everywhere"
        );
    }

    #[test]
    fn owner_defaults_to_general() {
        assert_eq!(
            effective_selection(SettingsSection::General, true, any_board, any_board),
            SettingsSection::General
        );
    }

    #[test]
    fn non_owner_falls_back_to_members() {
        for gated in [
            SettingsSection::General,
            SettingsSection::Storage,
            // EXP-771: the Features group mirrors the web's
            // `canManageWidgets` gate, which IS `isOwner`.
            SettingsSection::Widget,
        ] {
            assert_eq!(
                effective_selection(gated, false, any_board, any_board),
                SettingsSection::Members
            );
        }
        // S-3 (settings-nav.json `nav`): the @boards rows show to every
        // member (the page reads only for them), like web.
        assert_eq!(
            effective_selection(SettingsSection::Board("b-1".to_string()), false, any_board, any_board),
            SettingsSection::Board("b-1".to_string())
        );
        // EXP-557: Repositories is member-visible — the selection sticks
        // instead of falling back.
        assert_eq!(
            effective_selection(SettingsSection::Repositories, false, any_board, any_board),
            SettingsSection::Repositories
        );
    }

    #[test]
    fn device_sections_never_gated() {
        for section in [
            SettingsSection::Tools,
            SettingsSection::AgentClis,
            SettingsSection::LocalRepos,
            SettingsSection::Sessions,
        ] {
            assert!(section_visible(&section, false));
            assert_eq!(
                effective_selection(section.clone(), false, any_board, any_board),
                section
            );
        }
        // EXP-886: the run-history pane closes the This device pages.
        let labels: Vec<&str> = DEVICE_PAGES.iter().map(|(_, label, _)| *label).collect();
        // EXP-1294: the per-device CLI pane is "Coding CLIs" — "Agents" are
        // the team's agents.
        assert_eq!(labels, vec!["Tools", "Coding CLIs", "Worktrees", "Sessions"]);
        // #EXP-2027: they LEFT Settings, but their links still resolve.
        for (id, _, section) in DEVICE_PAGES {
            assert!(!flat_nav_items().any(|item| item.section == *section), "{id}");
            assert_eq!(section_for_link(id).as_ref(), Some(section), "{id}");
        }
    }

    /// EXP-262/EXP-238: the Personal sections are never gated and never fall
    /// back — for any owner state the selection sticks. They sit LAST in
    /// the flat walk, so the fallback scan still lands on General/Members,
    /// never on a personal pane.
    #[test]
    fn personal_sections_never_gated_and_never_fall_back() {
        for section in [
            SettingsSection::Account,
            SettingsSection::Notifications,
            SettingsSection::Voice,
            SettingsSection::ApiKeys,
            SettingsSection::About,
        ] {
            for owner in [false, true] {
                assert!(section_visible(&section, owner));
                assert_eq!(
                    effective_selection(section.clone(), owner, any_board, any_board),
                    section
                );
            }
        }
    }

    #[test]
    fn ungated_selection_is_kept() {
        assert_eq!(
            effective_selection(SettingsSection::Labels, false, any_board, any_board),
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
            effective_selection(selected.clone(), true, |id| id == "b-1", any_board),
            selected
        );
        assert_eq!(
            effective_selection(selected, true, |_| false, any_board),
            SettingsSection::General
        );
    }

    /// EXP-1294: a selected agent page survives while the agent exists, falls
    /// back once it's gone (deleted / team switch), and — unlike a board page
    /// — is member-visible.
    #[test]
    fn stale_agent_selection_falls_back_and_members_see_agents() {
        let selected = SettingsSection::Agent("a-1".to_string());
        assert!(section_visible(&selected, false));
        assert_eq!(
            effective_selection(selected.clone(), false, any_board, |id| id == "a-1"),
            selected
        );
        assert_eq!(
            effective_selection(selected.clone(), false, any_board, |_| false),
            SettingsSection::Members
        );
        assert_eq!(
            effective_selection(selected, true, any_board, |_| false),
            SettingsSection::General
        );
    }

    /// #EXP-2027 THE SPLIT: THE desktop settings navs = `settings-nav.json`
    /// `nav.desktopNavs`: every `desktopNavCases` entry on platform desktop
    /// (owner / member) replays through [`desktop_nav_model`], whole
    /// {personal, team, device}; the group and page labels are the
    /// fixture's. (`navCases` + `groups` stay the phones' until their new
    /// UI: the desktop no longer replays them.)
    #[test]
    fn the_navs_replay_the_settings_nav_desktop_navs() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../packages/domain-contract/fixtures/settings-nav.json"
        ))
        .unwrap();
        let nav = &fixture["nav"];
        let ids = |value: &serde_json::Value| -> Vec<String> {
            value.as_array().unwrap().iter().map(|item| item.as_str().unwrap().to_string()).collect()
        };
        let mut replayed = 0;
        for case in nav["desktopNavCases"].as_array().unwrap() {
            if case["platform"] != "desktop" {
                continue;
            }
            replayed += 1;
            // No desktop case sits on a cloud instance: the cloud-only
            // pages (Plan & Billing) are web-only anyway.
            assert_eq!(case["cloud"], false, "{}", case["name"]);
            let (personal, team, device) = desktop_nav_model(case["role"] == "owner");
            let got = serde_json::json!({"personal": personal, "team": team, "device": device});
            assert_eq!(got, case["expected"], "{}", case["name"]);
        }
        assert_eq!(replayed, 2);
        // The navs ARE `desktopNavs` on desktop: the ids in order (minus the
        // web-only pages), the group ids and labels, the pages' labels
        // (`rows` / `pages`) and the children folded under Issues.
        let on_desktop = |id: &str| {
            let platforms = nav["pages"][id]["platforms"].as_array().or_else(|| nav["rowPlatforms"][id].as_array());
            platforms.is_none_or(|platforms| platforms.iter().any(|platform| platform == "desktop"))
        };
        let label_of = |id: &str| {
            fixture["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["id"] == id)
                .map(|row| row["label"].clone())
                .unwrap_or_else(|| nav["pages"][id]["label"].clone())
        };
        for (key, group) in [("personal", &PERSONAL_NAV), ("team", &TEAM_NAV)] {
            let groups = nav["desktopNavs"][key].as_array().unwrap();
            assert_eq!(groups.len(), 1, "{key}");
            assert_eq!(groups[0]["id"], group.id, "{key}");
            assert_eq!(groups[0]["label"], group.label, "{key}");
            let expected: Vec<String> = ids(&groups[0]["items"]).into_iter().filter(|id| on_desktop(id)).collect();
            assert_eq!(group.items.iter().map(|item| item.id).collect::<Vec<_>>(), expected, "{key}");
            for item in group.items.iter().flat_map(|item| std::iter::once(item).chain(item.children.iter())) {
                assert_eq!(label_of(item.id), item.label, "{}", item.id);
            }
        }
        assert_eq!(DEVICE_PAGES.iter().map(|(id, _, _)| id.to_string()).collect::<Vec<_>>(), ids(&nav["desktopNavs"]["device"]));
        for (id, label, _) in DEVICE_PAGES {
            assert_eq!(label_of(id), *label, "{id}");
        }
        let issues = TEAM_NAV.items.iter().find(|item| item.id == "issues").unwrap();
        let children: Vec<&str> = issues.children.iter().map(|item| item.id).collect();
        assert_eq!(serde_json::json!(children), nav["children"]["issues"]);
        // The Voice page left the nav (the Assistant page's Voice section):
        // #EXP-1987: only Personal's `assistant` page resolves through it.
        let voice: Vec<&str> = flat_nav_items().filter(|item| item.section == SettingsSection::Voice).map(|item| item.id).collect();
        assert_eq!(voice, vec!["assistant"]);
        assert_eq!(ASSISTANT_PAGE, domain::agent_settings::ASSISTANT.copy.title);
        assert_eq!(section_for_link("assistant"), Some(SettingsSection::Voice));
    }

    /// #EXP-2027: every page of a nav belongs to it (a personal page shows
    /// under no team, a team page under one), and the page a frame shows
    /// moves `settings_team` accordingly: a team page with none set opens
    /// for the fallback team, a personal page drops the team, a board's,
    /// agent's or app's page (and a This device page) keeps what is set.
    #[test]
    fn every_page_shows_under_its_own_nav() {
        for item in PERSONAL_NAV.items {
            assert_eq!(section_scope(&item.section), SectionScope::Personal, "{}", item.id);
        }
        for item in TEAM_NAV.items.iter().flat_map(|item| std::iter::once(item).chain(item.children.iter())) {
            assert_eq!(section_scope(&item.section), SectionScope::Team, "{}", item.id);
        }
        for (id, _, section) in DEVICE_PAGES {
            assert_eq!(section_scope(section), SectionScope::Either, "{id}");
        }
        let team = || Some("t1".to_string());
        let fallback = || Some("t0".to_string());
        assert_eq!(scoped_team(SectionScope::Team, None, fallback), Some("t0".into()));
        assert_eq!(scoped_team(SectionScope::Team, team(), fallback), team());
        assert_eq!(scoped_team(SectionScope::Personal, team(), fallback), None);
        assert_eq!(scoped_team(SectionScope::Either, team(), fallback), team());
        assert_eq!(scoped_team(SectionScope::Either, None, fallback), None);
        // `Voice` is personal before it resolves to the Assistant's agent
        // page; a stale board page falls back to a team page.
        let assistant = SettingsSection::Agent("a-0".into());
        assert_eq!(nav_scope(&SettingsSection::Voice, &assistant), SectionScope::Personal);
        assert_eq!(nav_scope(&assistant, &assistant), SectionScope::Either);
        assert_eq!(nav_scope(&SettingsSection::Board("b-1".into()), &SettingsSection::General), SectionScope::Team);
    }

    /// #EXP-2027: the header: `← Settings` in the personal nav, the team's
    /// mark (image, else the glyph) + name in a team's.
    #[test]
    fn the_header_names_the_team_or_settings() {
        let personal = shell::back_state(None);
        assert_eq!(personal["label"], "Settings");
        assert_eq!((personal["hasImage"].as_bool(), personal["glyph"].as_bool()), (Some(false), Some(false)));
        let acme = NavTeam { id: "t1".into(), name: "Acme".into(), image: None };
        let glyph = shell::back_state(Some(&acme));
        assert_eq!(glyph["label"], "Acme");
        assert_eq!((glyph["hasImage"].as_bool(), glyph["glyph"].as_bool()), (Some(false), Some(true)));
        let pictured = NavTeam { image: Some("/api/teams/t1/image".into()), ..acme };
        let image = shell::back_state(Some(&pictured));
        assert_eq!((image["hasImage"].as_bool(), image["glyph"].as_bool()), (Some(true), Some(false)));
        assert_eq!(image["src"], "/api/teams/t1/image");
    }

    /// EXP-771: web copy wins — the nav says "Statuses", not the longer
    /// desktop-only "Issue statuses".
    #[test]
    fn statuses_nav_label_matches_the_web() {
        let label = flat_nav_items()
            .find(|item| item.section == SettingsSection::Statuses)
            .map(|item| item.label);
        assert_eq!(label, Some("Statuses"));
    }

    /// EXP-630 (web parity): Issues follows Members in the Team group with
    /// Labels then Statuses as its sub-pages, which are no top-level rows any
    /// more; the flat walk lists the three in a row and every one of them
    /// is member-visible.
    #[test]
    fn issues_nests_labels_and_statuses_like_the_web() {
        let team = &TEAM_NAV;
        let members = team.items.iter().position(|item| item.label == "Members").unwrap();
        let issues = &team.items[members + 1];
        assert_eq!(issues.label, "Issues");
        assert_eq!(issues.section, SettingsSection::Issues);
        let children: Vec<(&str, &SettingsSection)> =
            issues.children.iter().map(|item| (item.label, &item.section)).collect();
        assert_eq!(
            children,
            vec![("Labels", &SettingsSection::Labels), ("Statuses", &SettingsSection::Statuses)]
        );
        assert!(!team.items.iter().any(|item| item.label == "Labels" || item.label == "Statuses"));
        let flat: Vec<&str> = flat_nav_items().map(|item| item.label).collect();
        let at = flat.iter().position(|label| *label == "Issues").unwrap();
        assert_eq!(&flat[at..at + 3], ["Issues", "Labels", "Statuses"]);
        for item in std::iter::once(issues).chain(issues.children.iter()) {
            assert!(section_visible(&item.section, false), "{} is member-visible", item.label);
        }
    }

    /// EXP-500: archiving the board whose settings pane is open drops it out
    /// of the synced collection, which is exactly the stale-`Board(id)` case
    /// above — the nav clamps to General instead of rendering a blank pane.
    /// The All boards pane it lands on stays reachable. #EXP-1797: every
    /// member reaches it (it carries the issue trash; settings-nav.json).
    #[test]
    fn all_boards_pane_is_member_visible_and_never_clamped() {
        assert!(section_visible(&SettingsSection::AllBoards, true));
        assert!(section_visible(&SettingsSection::AllBoards, false));
        assert_eq!(
            effective_selection(SettingsSection::AllBoards, true, |_| false, |_| false),
            SettingsSection::AllBoards
        );
    }

    /// EXP-1319: New agent is a PAGE (web `settings/agents/new`), owner-only
    /// like the web route: a member's selection clamps to the fallback.
    #[test]
    fn the_new_agent_page_is_owner_only() {
        assert!(section_visible(&SettingsSection::NewAgent, true));
        assert!(!section_visible(&SettingsSection::NewAgent, false));
        assert_eq!(
            effective_selection(SettingsSection::NewAgent, true, |_| false, |_| false),
            SettingsSection::NewAgent
        );
        assert_ne!(
            effective_selection(SettingsSection::NewAgent, false, |_| true, |_| true),
            SettingsSection::NewAgent
        );
    }

    /// NEW-6: the prompts list and a prompt's page are member-readable,
    /// New prompt owner-only, and the Prompts row stays lit on both
    /// sub-pages.
    #[test]
    fn prompts_are_member_readable_and_stay_lit() {
        assert!(section_visible(&SettingsSection::Prompts, false));
        assert!(section_visible(&SettingsSection::Prompt("p-1".into()), false));
        assert!(section_visible(&SettingsSection::NewPrompt, true));
        assert!(!section_visible(&SettingsSection::NewPrompt, false));
        assert_ne!(
            effective_selection(SettingsSection::NewPrompt, false, |_| true, |_| true),
            SettingsSection::NewPrompt
        );
        for lit in [
            SettingsSection::Prompts,
            SettingsSection::Prompt("p-1".into()),
            SettingsSection::NewPrompt,
        ] {
            assert!(nav_lit(&SettingsSection::Prompts, &lit), "{lit:?}");
        }
        assert!(!nav_lit(&SettingsSection::Prompts, &SettingsSection::Repositories));
    }
}

/// #EXP-1629: the field bench's (`settings_bench::field_variants`) read of
/// [`doc::Ink`]: the group radius and fill, the rem, the muted glyph and the
/// row hairline.
#[cfg(test)]
pub(crate) fn doc_ink(cx: &App) -> (f32, String, f32, String, String) {
    let ink = doc::Ink::of(cx);
    (ink.radius_lg, ink.fill_row.clone(), ink.rem, ink.muted.clone(), ink.stroke_row.clone())
}

/// #EXP-1629: [`doc::mount`] for the field bench.
#[cfg(test)]
pub(crate) fn doc_mount<S: 'static>(id: &str, tree: serde_json::Value, binding: &gpui::Entity<eui_gpui::binding::Binding<S>>, window: &mut gpui::Window, cx: &mut App) -> gpui::Entity<eui_gpui::view::SurfaceView> {
    doc::mount(id, tree, binding, window, cx)
}

/// #EXP-1629: [`doc::mount_with`] with the IDE plugin and window-level
/// popups, for the Select bench.
#[cfg(test)]
pub(crate) fn doc_mount_lifted<S: 'static>(id: &str, tree: serde_json::Value, binding: &gpui::Entity<eui_gpui::binding::Binding<S>>, window: &mut gpui::Window, cx: &mut App) -> gpui::Entity<eui_gpui::view::SurfaceView> {
    doc::mount_with(id, tree, binding, std::rc::Rc::new(doc::ide_plugin()), true, window, cx)
}
