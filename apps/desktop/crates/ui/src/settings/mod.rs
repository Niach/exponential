//! The ONE settings screen (masterplan-v3 §4.2 "Settings", §7.9 integrations
//! surface; EXP-238 folded the old Account screen in).
//!
//! Web parity targets: the `routes/t/$teamSlug/settings/` pages and
//! their `components/team/*-section.tsx` + `components/account/*` cards. The
//! screen mirrors the web's grouped master-detail layout (EXP-146): a fixed
//! left nav with the groups — **Team** (General, Members, Issues ⟩ Labels, Statuses,
//! Storage — the EXP-297 owner-only attachment manager), **Boards**
//! (one entry PER board + New board — EXP-288 flattened the old flat Boards
//! list into per-board detail pages), **Agents** (EXP-1294: one entry per
//! agent + New agent, then Repositories — NEW-41, web's `SETTINGS_NAV` — and
//! Prompts),
//! **Features** (Widget — EXP-771, MCP servers — EXP-792), the desktop-only
//! **This device** group (Tools, Coding CLIs, Worktrees, Sessions), and
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
// VAPP-177: the app editor's Settings tab (rendered by the app screen).
pub(crate) mod app_detail;
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
    active_team_id, go_back, nav_for_window, navigate, set_screen, Navigation, Screen,
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
    /// ordinary Personal-group item in [`NAV_GROUPS`] — it sits LAST, so the
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
/// `flattenSettingsNav`) — what the fallback scan and the tests walk.
fn flat_nav_items() -> impl Iterator<Item = &'static NavItem> {
    NAV_GROUPS
        .iter()
        .flat_map(|group| group.items)
        .flat_map(|item| std::iter::once(item).chain(item.children.iter()))
}

/// #EXP-1674: the section a `settings/<section>` link names: a nav page's id
/// (`settings-nav.json`) or a web route the desktop folds into one. `None` =
/// no such page here (Billing, Import, the Boards index): the browser's.
pub(crate) fn section_for_link(section: &str) -> Option<SettingsSection> {
    match section {
        "agents" => Some(SettingsSection::Agents),
        // The Voice section resolves to the viewer's Assistant page.
        "assistant" | "voice" => Some(SettingsSection::Voice),
        "mcp-servers" => Some(SettingsSection::Mcp),
        _ => flat_nav_items()
            .find(|item| item.id == section)
            .map(|item| item.section.clone()),
    }
}

struct NavGroup {
    /// The fixture's group id (`team`, `boards`, `agents`, `apps`,
    /// `device`, `personal`).
    id: &'static str,
    label: &'static str,
    items: &'static [NavItem],
}

/// S-3 (owner 2026-10-28): THE settings nav ×4 = `settings-nav.json` `nav`
/// on platform `desktop` (the test replays its `navCases`). The injected
/// lists (`@boards` + `@new-board`, `@assistant` + `@agents` +
/// `@new-agent`, `@apps` + `@new-app`) are drawn at render AHEAD of their
/// group's static pages; Plan & Billing and Import are web-only. The
/// Features and Advanced groups are gone, and Voice left Personal: its block
/// is the Assistant page's Voice section. Order defines both the nav and the
/// fallback scan (first visible item).
const NAV_GROUPS: &[NavGroup] = &[
    NavGroup {
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
            NavItem::leaf("widget", "Widget", SettingsSection::Widget),
            NavItem::leaf("storage", "Storage", SettingsSection::Storage),
        ],
    },
    // The per-board rows + New board are injected at render.
    NavGroup {
        id: "boards",
        label: "Boards",
        items: &[NavItem::leaf("all-boards", "All boards", SettingsSection::AllBoards)],
    },
    // The viewer's Assistant, the team agents and New agent are injected at
    // render; MCP closes the group.
    NavGroup {
        id: "agents",
        label: AGENTS_GROUP,
        items: &[
            NavItem::leaf("mcp", mcp::SETTINGS_MCP.title, SettingsSection::Mcp),
            // #EXP-1660 (settings-nav.json): Skills right after MCP.
            NavItem::leaf("skills", domain::skills::TITLE, SettingsSection::Skills),
        ],
    },
    // VAPP-110: the per-app rows + New app are injected at render.
    NavGroup { id: "apps", label: APPS_GROUP, items: &[] },
    NavGroup {
        id: "device",
        label: "This device",
        items: &[
            NavItem::leaf("tools", "Tools", SettingsSection::Tools),
            NavItem::leaf("coding-clis", "Coding CLIs", SettingsSection::AgentClis),
            NavItem::leaf("worktrees", "Worktrees", SettingsSection::LocalRepos),
            NavItem::leaf("sessions", "Sessions", SettingsSection::Sessions),
        ],
    },
    NavGroup {
        id: "personal",
        label: "Personal",
        items: &[
            NavItem::leaf("account", "Account", SettingsSection::Account),
            NavItem::leaf("notifications", "Notifications", SettingsSection::Notifications),
            NavItem::leaf("security", "Security", SettingsSection::ApiKeys),
            NavItem::leaf("about", "About", SettingsSection::About),
        ],
    },
];

#[cfg(test)]
/// The injected `@lists` of a group, in the fixture's order, for a viewer
/// (`newEntries`: the New rows are the owners').
fn injected_items(group_id: &str, owner: bool) -> Vec<&'static str> {
    match group_id {
        "boards" if owner => vec!["@boards", "@new-board"],
        "boards" => vec!["@boards"],
        "agents" if owner => vec!["@assistant", "@agents", "@new-agent"],
        "agents" => vec!["@assistant", "@agents"],
        "apps" if owner => vec!["@apps", "@new-app"],
        "apps" => vec!["@apps"],
        _ => Vec::new(),
    }
}

#[cfg(test)]
/// The nav's groups for a viewer: group id → item ids (the injected lists
/// first, then the visible static pages; children folded under their
/// parent). Locked by `settings-nav.json` `navCases` (platform desktop).
fn nav_model(owner: bool) -> Vec<(&'static str, Vec<&'static str>)> {
    NAV_GROUPS
        .iter()
        .map(|group| {
            let mut items = injected_items(group.id, owner);
            items.extend(
                group
                    .items
                    .iter()
                    .filter(|item| section_visible(&item.section, owner))
                    .map(|item| item.id),
            );
            (group.id, items)
        })
        .filter(|(_, items)| !items.is_empty())
        .collect()
}

/// EXP-1294: the team Agents group's label — `agent-settings.json`
/// `copy.title` (a test locks the two together).
const AGENTS_GROUP: &str = "Agents";

/// VAPP-110: the team Apps group's label — `apps.json` `copy.title`.
const APPS_GROUP: &str = "Apps";

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
    let Some(team_id) = active_team_id(nav, cx) else {
        return Some(Default::default());
    };
    Some(
        crate::session_data::agents_in_team(&team_id, cx)
            .into_iter()
            .map(|agent| agent.agent_id)
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
    let Some(team_id) = active_team_id(nav, cx) else {
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
        SettingsSection::Agent(id) if id.is_empty() => active_team_id(nav, cx)
            .and_then(|team_id| crate::session_data::agents_in_team(&team_id, cx).into_iter().next())
            .map(|agent| SettingsSection::Agent(agent.agent_id))
            .unwrap_or(SettingsSection::Agent(id)),
        // S-3: the Voice page left the nav; its block is the Assistant
        // page's Voice section (the tray / meeting links land there).
        SettingsSection::Voice => active_team_id(nav, cx)
            .and_then(|team_id| crate::session_data::my_assistant(&team_id, cx))
            .map(|assistant| SettingsSection::Agent(assistant.id))
            .unwrap_or(SettingsSection::Voice),
        // VAPP-110: `App("")` = the team's first app; a slug (the dev
        // `app:<slug>`) names its app.
        SettingsSection::App(id) => {
            let apps = active_team_id(nav, cx)
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
        let owner = active_team_id(&self.nav, cx)
            .map(|ws| is_owner(cx, &ws))
            .unwrap_or(false);
        // Web settings layout route (EXP-146): grouped left nav + one
        // selected section pane in the detail column. EXP-282: the nav is
        // the window's left column now ([`SettingsNavPanel`]) — this view is
        // the detail column alone.
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
                let team_id = active_team_id(&self.nav, cx).unwrap_or_default();
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
                let team_id = active_team_id(&self.nav, cx);
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
                let team_id = active_team_id(&self.nav, cx).unwrap_or_default();
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
                let team_id = active_team_id(&self.nav, cx).unwrap_or_default();
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
    /// What each row's key does and the team it acts in (this frame's).
    routes: std::rc::Rc<std::cell::RefCell<NavRoutes>>,
    list_binding: Entity<eui_gpui::binding::Binding<serde_json::Value>>,
    list_view: Entity<eui_gpui::view::SurfaceView>,
    back_view: Entity<eui_gpui::view::SurfaceView>,
    _back_binding: Entity<eui_gpui::binding::Binding<serde_json::Value>>,
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
            // EXP-1294: the Agents group lists the live agent roots.
            cx.observe(&collections.sessions, |_, _, cx| cx.notify()),
            cx.observe(&collections.agents, |_, _, cx| cx.notify()),
            // VAPP-110: the Apps group lists the synced apps.
            cx.observe(&collections.apps, |_, _, cx| cx.notify()),
        ];
        let routes: std::rc::Rc<std::cell::RefCell<NavRoutes>> = Default::default();
        let handle = window.window_handle();
        let panel = cx.weak_entity();
        let list_binding = cx.new(|_| {
            let mut binding = eui_gpui::binding::Binding::new("/nav", serde_json::json!({"items": []}), |state: &serde_json::Value, _| state.clone());
            let routes = routes.clone();
            binding.on_action("pick", move |_, event, cx| {
                let key = event.context.get("key").and_then(serde_json::Value::as_str).unwrap_or_default();
                let (action, team_id) = {
                    let routes = routes.borrow();
                    (routes.actions.get(key).cloned()?, routes.team_id.clone())
                };
                let panel = panel.clone();
                // Deferred: the binding is mid-update.
                cx.defer(move |cx| {
                    let _ = handle.update(cx, |_, window, cx| nav_dispatch(action, team_id, &panel, window, cx));
                });
                None
            });
            binding
        });
        let back_nav = nav.clone();
        let back_binding = cx.new(|_| {
            let mut binding = eui_gpui::binding::Binding::new("/back", serde_json::Value::Null, |state: &serde_json::Value, _| state.clone());
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
            _back_binding: back_binding,
            folded: Default::default(),
            #[cfg(test)]
            shot: None,
            _subscriptions: subscriptions,
        }
    }

}

/// The data one nav frame draws: who looks (owner gating), the team, the
/// lit section and the injected lists.
pub(crate) struct NavInputs {
    pub(crate) owner: bool,
    pub(crate) team_id: Option<String>,
    pub(crate) effective: SettingsSection,
    pub(crate) boards: Vec<domain::rows::Board>,
    pub(crate) agents: Vec<domain::session_tree::AgentRow>,
    pub(crate) apps: Vec<crate::apps_view::AppEntry>,
}

/// The bench's nav (#EXP-1629): the inputs pinned instead of read off the
/// store (`selected` still clamps through [`effective_selection`]).
#[cfg(test)]
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
            return NavInputs { owner: shot.owner, team_id: shot.team_id.clone(), effective, boards: shot.boards.clone(), agents: shot.agents.clone(), apps: shot.apps.clone() };
        }
        let owner = active_team_id(&self.nav, cx)
            .map(|ws| is_owner(cx, &ws))
            .unwrap_or(false);
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
        let team_id = active_team_id(&self.nav, cx);
        let boards = team_id
            .as_deref()
            .map(|team_id| Store::global(cx).collections().boards_in_team(team_id, cx))
            .unwrap_or_default();
        // S-3: the Agents group mounting creates the viewer's Assistant on
        // first use.
        if let Some(team_id) = team_id.as_deref() {
            crate::session_data::ensure_assistant(team_id, cx);
        }
        let agents = team_id
            .as_deref()
            .map(|team_id| crate::session_data::agents_in_team(team_id, cx))
            .unwrap_or_default();
        // VAPP-110: the Apps group's rows = the apps the viewer manages.
        let apps = team_id
            .as_deref()
            .map(|team_id| crate::apps_view::app_entries(team_id, cx))
            .unwrap_or_default();
        NavInputs { owner, team_id, effective, boards, agents, apps }
    }
}

impl Render for SettingsNavPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let _span = tracing::trace_span!("render", view = "SettingsNavPanel").entered();
        let inputs = self.inputs(cx);
        let (items, actions) = shell::nav_entries(&inputs, &self.folded, &self.paint, cx);
        *self.routes.borrow_mut() = NavRoutes { actions, team_id: inputs.team_id };
        self.list_binding.update(cx, |binding, cx| binding.update_state(|state| *state = serde_json::json!({"items": items}), cx));
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

/// The nav's routing for one frame: each row key's action, the team.
#[derive(Default)]
struct NavRoutes {
    actions: std::collections::HashMap<String, shell::NavAction>,
    team_id: Option<String>,
}

/// A nav row's press.
fn nav_dispatch(action: shell::NavAction, team_id: Option<String>, panel: &gpui::WeakEntity<SettingsNavPanel>, window: &mut Window, cx: &mut App) {
    match action {
        shell::NavAction::Section(section) => {
            select_settings_section(window, cx, section);
            navigate(window, cx, Screen::Settings);
        }
        shell::NavAction::App(app_id) => {
            crate::app_screen::open_app_tab(window, cx, app_id, domain::apps::EditorTab::Settings, false);
        }
        shell::NavAction::NewApp => {
            if let Some(team_id) = team_id {
                crate::apps_view::new_app(team_id, window, cx);
            }
        }
        shell::NavAction::NewBoard => {
            if let Some(team_id) = team_id {
                crate::create_board_dialog::open(window, cx, team_id);
            }
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

/// Web `isMember` gate (`useTeamPermissions`: the signed-in user has a
/// `team_members` row of the team — v7's one capability gate, every
/// mutation affordance rides it). The chat strip's `+` and the Guide
/// menu read it (web chat route `permissions.isMember`).
pub(crate) fn is_member(cx: &App, team_id: &str) -> bool {
    my_membership(cx, team_id).is_some()
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
        // EXP-886: the run-history pane closes the This device group
        // (SLOP-5: labelled "Runs").
        let device = NAV_GROUPS
            .iter()
            .find(|group| group.label == "This device")
            .expect("This device group");
        let labels: Vec<&str> = device.items.iter().map(|item| item.label).collect();
        // EXP-1294: the per-device CLI pane is "Coding CLIs" — "Agents" are
        // the team's agents.
        assert_eq!(labels, vec!["Tools", "Coding CLIs", "Worktrees", "Sessions"]);
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
        assert_eq!(AGENTS_GROUP, domain::agent_settings::AGENT_SETTINGS.copy.title);
    }

    /// S-3: THE settings nav = `settings-nav.json` `nav` on platform
    /// desktop: every `navCases` entry (owner / member) replays through
    /// [`nav_model`], the group labels and the pages' labels are the
    /// fixture's.
    #[test]
    fn the_nav_replays_the_settings_nav_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../../packages/domain-contract/fixtures/settings-nav.json"
        ))
        .unwrap();
        let nav = &fixture["nav"];
        let mut replayed = 0;
        for case in nav["navCases"].as_array().unwrap() {
            if case["platform"] != "desktop" {
                continue;
            }
            replayed += 1;
            let owner = case["role"] == "owner";
            let expected: Vec<(String, Vec<String>)> = case["expected"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(group, items)| {
                    (
                        group.clone(),
                        items.as_array().unwrap().iter().map(|item| item.as_str().unwrap().to_string()).collect(),
                    )
                })
                .collect();
            let got: Vec<(String, Vec<String>)> = nav_model(owner)
                .into_iter()
                .map(|(group, items)| (group.to_string(), items.into_iter().map(str::to_string).collect()))
                .collect();
            // serde_json keeps object keys sorted: compare as maps, then the
            // group ORDER against `nav.groups`.
            let sorted = |mut list: Vec<(String, Vec<String>)>| {
                list.sort();
                list
            };
            assert_eq!(sorted(got.clone()), sorted(expected), "{}", case["name"]);
            let order: Vec<&str> = nav["groups"]
                .as_array()
                .unwrap()
                .iter()
                .map(|group| group["id"].as_str().unwrap())
                .filter(|id| got.iter().any(|(group, _)| group == id))
                .collect();
            assert_eq!(got.iter().map(|(group, _)| group.as_str()).collect::<Vec<_>>(), order);
        }
        assert_eq!(replayed, 2);
        // The labels: the groups', the pages' (`rows` / `pages`), and the
        // children folded under Issues.
        for group in NAV_GROUPS {
            let fixture_group = nav["groups"]
                .as_array()
                .unwrap()
                .iter()
                .find(|candidate| candidate["id"] == group.id)
                .expect("a fixture group");
            assert_eq!(fixture_group["label"], group.label, "{}", group.id);
            for item in group.items.iter().flat_map(|item| std::iter::once(item).chain(item.children.iter())) {
                let label = fixture["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|row| row["id"] == item.id)
                    .map(|row| row["label"].clone())
                    .unwrap_or_else(|| nav["pages"][item.id]["label"].clone());
                assert_eq!(label, item.label, "{}", item.id);
            }
        }
        let children: Vec<&str> = NAV_GROUPS[0].items[2].children.iter().map(|item| item.id).collect();
        assert_eq!(serde_json::json!(children), nav["children"]["issues"]);
        assert_eq!(APPS_GROUP, domain::apps::APPS.copy.title);
        // The Voice page left the nav (the Assistant page's Voice section).
        assert!(!flat_nav_items().any(|item| item.section == SettingsSection::Voice));
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
        let team = NAV_GROUPS.iter().find(|group| group.label == "Team").expect("Team group");
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
