//! #EXP-1674: THE link opener ([`domain::app_link`], `fixtures/app-link.json`):
//! every clicked link on this client lands here (agent prose, documents, an
//! eApp's `Link`). A link only navigates: nothing here writes.
//!
//! The rule is [`domain::app_link::link_open_outcome`]: not a member of the
//! link's team = home + `copy.noAccess`; a page with no screen here = the
//! browser at its absolute URL; missing or unreadable = the screen's own
//! not-found state, else `copy.noAccess`; found in another of the viewer's
//! teams = switch without asking, then open.

use std::rc::Rc;

use domain::app_link::{
    app_path_team, classify_app_link, classify_app_link_in_team, expand_team_link, link_open_outcome,
    AppLink, LinkOpenOutcome, NO_ACCESS,
};
use gpui::{App, Window};
use serde_json::Value;
use sync::Store;

use crate::navigation::{self, Screen};
use crate::settings::SettingsSection;

/// Where an in-app link lands.
enum Landing {
    Screen(Screen),
    Issue { issue_id: String, board_id: String },
    Board { board_id: String },
    Plan { session_id: String, message_id: String },
    AppScreen {
        app_id: String,
        screen: String,
        params: serde_json::Map<String, Value>,
    },
    /// `None` = the Settings root.
    Settings(Option<SettingsSection>),
}

/// What a link's target is on this client.
enum Target {
    Found(Landing),
    /// Missing or unreadable: the landing showing its not-found state, `None`
    /// when no screen has one (`copy.noAccess`).
    NotFound(Option<Landing>),
    /// No screen on this client.
    WebOnly,
}

/// A link with no surface team: `~/x` resolves against the window's active
/// team. The default of every markdown view.
pub(crate) fn open_agent_link(href: &str, window: &mut Window, cx: &mut App) {
    open_link(href, None, window, cx);
}

/// The `on_open_link` of a surface whose team is `team_id` (the team of the
/// message, issue or app holding the link); `None` = the active team.
pub(crate) fn link_opener(team_id: Option<String>) -> impl Fn(&str, &mut Window, &mut App) + 'static {
    move |href: &str, window: &mut Window, cx: &mut App| {
        open_link_for_team_id(href, team_id.as_deref(), window, cx);
    }
}

fn open_link_for_team_id(href: &str, team_id: Option<&str>, window: &mut Window, cx: &mut App) {
    let team_slug = team_id.and_then(|team_id| team_slug_by_id(team_id, cx));
    open_link(href, team_slug.as_deref(), window, cx);
}

/// `HostPolicy::open_url` of an eApp in `team_id`: the eUI URL policy already
/// allowed `url` and made it absolute. In-app goes through the opener (on the
/// active window: the host is mid-update here); anything else the policy
/// allowed goes to the OS opener, as it always did.
pub(crate) fn app_url_opener(team_id: Option<String>) -> Rc<dyn Fn(&str, &mut App)> {
    Rc::new(move |url: &str, cx: &mut App| {
        let origin = link_origin(cx);
        let href = team_shorthand_of(url, &origin).unwrap_or_else(|| url.to_string());
        let in_app = href.starts_with('~')
            || !matches!(classify_app_link(&href, &origin), AppLink::External { .. } | AppLink::Ignore);
        if in_app {
            let team_id = team_id.clone();
            navigation::on_active_window(cx, move |window, cx| {
                open_link_for_team_id(&href, team_id.as_deref(), window, cx);
            });
        } else {
            cx.open_url(url);
        }
    })
}

/// THE opener. `team_slug` = the surface's team; `None` = the active team's.
pub(crate) fn open_link(href: &str, team_slug: Option<&str>, window: &mut Window, cx: &mut App) {
    let origin = link_origin(cx);
    let team = team_slug.map(str::to_string).or_else(|| active_team_slug(window, cx));
    match classify_app_link_in_team(href, &origin, team.as_deref()) {
        AppLink::Ignore => log::info!("link ignored: {href}"),
        AppLink::External { url } => crate::markdown::open_link_href(&url),
        link => open_in_app(link, absolute_url(href, team.as_deref(), &origin), window, cx),
    }
}

fn open_in_app(link: AppLink, url: Option<String>, window: &mut Window, cx: &mut App) {
    // A link naming a team this viewer does not sync is not a member's. A
    // page outside any team (`/admin`) has no team to be a member of.
    let (member, team_id) = match link_team(&link) {
        Some(slug) => {
            let team_id = team_id_by_slug(&slug, cx);
            (team_id.is_some(), team_id)
        }
        None => (true, None),
    };
    let target = resolve(&link, team_id.as_deref(), cx);
    let (found, web_only) = match &target {
        Target::Found(_) => (true, false),
        Target::NotFound(_) => (false, false),
        Target::WebOnly => (true, true),
    };
    let active_team = match team_id.as_deref() {
        Some(team_id) => navigation::nav_for_window_id(window.window_handle().window_id(), cx)
            .is_none_or(|nav| navigation::active_team_id(&nav, cx).as_deref() == Some(team_id)),
        None => true,
    };
    match (link_open_outcome(member, active_team, found, web_only), target) {
        (LinkOpenOutcome::NoAccess, _) => {
            no_access(window, cx);
            navigation::navigate(window, cx, Screen::Home);
        }
        (LinkOpenOutcome::Browser, _) => match url {
            Some(url) => crate::markdown::open_link_href(&url),
            None => log::info!("link has no absolute url: {link:?}"),
        },
        (LinkOpenOutcome::NotFound, Target::NotFound(Some(landing))) => land_held(None, landing, window, cx),
        (LinkOpenOutcome::NotFound, _) => no_access(window, cx),
        (LinkOpenOutcome::Open, Target::Found(landing)) => land_held(None, landing, window, cx),
        (LinkOpenOutcome::SwitchTeam, Target::Found(landing)) => land_held(team_id, landing, window, cx),
        (LinkOpenOutcome::Open | LinkOpenOutcome::SwitchTeam, _) => {}
    }
}

fn no_access(window: &mut Window, cx: &mut App) {
    crate::toast::show(crate::toast::Toast::error(NO_ACCESS), window, cx);
}

/// The team a link names.
fn link_team(link: &AppLink) -> Option<String> {
    match link {
        AppLink::Issue { team_slug, .. }
        | AppLink::IssueRef { team_slug, .. }
        | AppLink::Board { team_slug, .. }
        | AppLink::Session { team_slug, .. }
        | AppLink::Plan { team_slug, .. }
        | AppLink::Thread { team_slug, .. }
        | AppLink::Threads { team_slug }
        | AppLink::Agent { team_slug, .. }
        | AppLink::AgentHome { team_slug }
        | AppLink::Inbox { team_slug }
        | AppLink::Devices { team_slug }
        | AppLink::Settings { team_slug, .. }
        | AppLink::AppHome { team_slug, .. }
        | AppLink::AppScreen { team_slug, .. } => Some(team_slug.clone()),
        AppLink::App { path } => app_path_team(path),
        AppLink::External { .. } | AppLink::Ignore => None,
    }
}

fn found(screen: Screen) -> Target {
    Target::Found(Landing::Screen(screen))
}

/// The link's target in `team_id` (`None` = no team of the viewer's).
fn resolve(link: &AppLink, team_id: Option<&str>, cx: &App) -> Target {
    // Every other page of this instance: this client has no screen for it.
    if matches!(link, AppLink::App { .. }) {
        return Target::WebOnly;
    }
    let Some(team_id) = team_id else {
        return Target::NotFound(None);
    };
    match link {
        AppLink::Issue { identifier, .. } | AppLink::IssueRef { identifier, .. } => {
            match issue_by_identifier(team_id, identifier, cx) {
                Some((issue_id, board_id)) => Target::Found(Landing::Issue { issue_id, board_id }),
                None => Target::NotFound(None),
            }
        }
        AppLink::Board { board_slug, .. } => match board_by_slug(team_id, board_slug, cx) {
            Some(board_id) => Target::Found(Landing::Board { board_id }),
            None => Target::NotFound(None),
        },
        AppLink::Session { session_id, .. } => session_target(
            session_id,
            team_id,
            Landing::Screen(navigation::session_screen_for(session_id, cx)),
            cx,
        ),
        // A plan page waits for its message and has no not-found state; the
        // session route's stands in.
        AppLink::Plan { session_id, message_id, .. } => {
            let plan = Landing::Plan {
                session_id: session_id.clone(),
                message_id: message_id.clone(),
            };
            match session_target(session_id, team_id, plan, cx) {
                Target::NotFound(Some(_)) => Target::NotFound(Some(Landing::Screen(Screen::Session {
                    session_id: session_id.clone(),
                }))),
                target => target,
            }
        }
        AppLink::Thread { thread_id, .. } => session_target(
            thread_id,
            team_id,
            Landing::Screen(Screen::Thread {
                session_id: thread_id.clone(),
            }),
            cx,
        ),
        AppLink::Threads { .. } => found(Screen::Threads),
        AppLink::Agent { agent_id, .. } => {
            let agent = crate::session_data::agent_def(&crate::agent_screen::resolve_agent_id(agent_id, cx), cx);
            let screen = Screen::Agent {
                agent_id: agent_id.clone(),
            };
            match agent {
                None => Target::NotFound(Some(Landing::Screen(screen))),
                Some(row) if row.team_id.as_deref().is_none_or(|team| team == team_id) => found(screen),
                Some(_) => Target::NotFound(None),
            }
        }
        // The team's agent page is HOME (EXP-1294).
        AppLink::AgentHome { .. } => found(Screen::Home),
        AppLink::Inbox { .. } => found(Screen::Inbox {
            tab: crate::sidebar::InboxTab::Inbox,
        }),
        AppLink::Devices { .. } => found(Screen::Devices),
        AppLink::Settings { section, .. } if section.is_empty() => Target::Found(Landing::Settings(None)),
        // `settings-nav.json` `visible: owner`: not an owner reads as missing.
        AppLink::Settings { section, .. }
            if matches!(section.as_str(), "general" | "widget")
                && !crate::settings::is_owner(cx, team_id) =>
        {
            Target::NotFound(None)
        }
        AppLink::Settings { section, .. } => match crate::settings::section_for_link(section) {
            Some(section) => Target::Found(Landing::Settings(Some(section))),
            None => Target::WebOnly,
        },
        AppLink::AppHome { app_slug, .. } => {
            app_target(app_slug, team_id, |app| Landing::Screen(Screen::AppRun { app }), cx)
        }
        AppLink::AppScreen { app_slug, screen, params, .. } => app_target(
            app_slug,
            team_id,
            |app_id| Landing::AppScreen {
                app_id,
                screen: screen.clone(),
                params: params
                    .iter()
                    .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                    .collect(),
            },
            cx,
        ),
        AppLink::App { .. } | AppLink::External { .. } | AppLink::Ignore => Target::NotFound(None),
    }
}

/// A `sessions` row of `team_id`. A row of ANOTHER team under this team's
/// slug is unreadable here: `copy.noAccess`, since its own screen would open it.
fn session_target(session_id: &str, team_id: &str, landing: Landing, cx: &App) -> Target {
    match crate::session_data::session(session_id, cx) {
        None => Target::NotFound(Some(landing)),
        Some(row) if row.team_id.as_deref().is_none_or(|team| team == team_id) => Target::Found(landing),
        Some(_) => Target::NotFound(None),
    }
}

/// An app of `team_id` by slug. A slug only ANOTHER team has must not reach
/// the run view (it resolves slugs against the active team).
fn app_target(app_slug: &str, team_id: &str, landing: impl FnOnce(String) -> Landing, cx: &App) -> Target {
    let app = crate::apps_view::find_app(app_slug, Some(team_id), cx)
        .filter(|app| app.row.as_ref().is_none_or(|row| row.team_id.as_deref() == Some(team_id)));
    match app {
        Some(app) => Target::Found(landing(app.id)),
        None if crate::apps_view::find_app(app_slug, None, cx).is_some() => Target::NotFound(None),
        None => Target::NotFound(Some(Landing::Screen(Screen::AppRun {
            app: app_slug.to_string(),
        }))),
    }
}

/// One held move on a New issue page (EXP-1212): the switch and the landing
/// together, or neither.
fn land_held(switch_to: Option<String>, landing: Landing, window: &mut Window, cx: &mut App) {
    navigation::leave_held(window, cx, None, move |window, cx| {
        if let Some(team_id) = switch_to {
            navigation::switch_team(window, cx, team_id);
        }
        land(landing, window, cx);
    });
}

fn land(landing: Landing, window: &mut Window, cx: &mut App) {
    match landing {
        Landing::Screen(screen) => navigation::navigate(window, cx, screen),
        Landing::Issue { issue_id, board_id } => {
            navigation::open_issue_scoped(window, cx, issue_id, board_id, None);
        }
        Landing::Board { board_id } => {
            navigation::set_active_board(window, cx, board_id.clone());
            navigation::navigate(window, cx, Screen::BoardIssues { board_id });
        }
        Landing::Plan { session_id, message_id } => crate::plan_page::open(&session_id, &message_id, window, cx),
        Landing::AppScreen { app_id, screen, params } => {
            crate::app_screen::open_app_screen(window, cx, app_id, screen, params);
        }
        Landing::Settings(section) => {
            navigation::navigate(window, cx, Screen::Settings);
            if let Some(section) = section {
                crate::sidebar::select_settings_section(window, cx, section);
            }
        }
    }
}

/// The instance origin, no trailing slash; empty when signed out.
fn link_origin(cx: &App) -> String {
    let origin = crate::queries::instance_origin(cx);
    origin.as_deref().unwrap_or("").trim_end_matches('/').to_string()
}

fn active_team_slug(window: &Window, cx: &App) -> Option<String> {
    Store::try_global(cx)?;
    let nav = navigation::nav_for_window_id(window.window_handle().window_id(), cx)?;
    team_slug_by_id(&navigation::active_team_id(&nav, cx)?, cx)
}

fn team_slug_by_id(team_id: &str, cx: &App) -> Option<String> {
    let store = Store::try_global(cx)?;
    let teams = store.collections().teams.read(cx);
    teams.get(team_id)?.slug.clone().filter(|slug| !slug.is_empty())
}

/// The synced team a link's slug names (case-insensitive).
fn team_id_by_slug(team_slug: &str, cx: &App) -> Option<String> {
    let store = Store::try_global(cx)?;
    let teams = store.collections().teams.read(cx);
    teams
        .iter()
        .find(|team| team.slug.as_deref().is_some_and(|slug| slug.eq_ignore_ascii_case(team_slug)))
        .map(|team| team.id.clone())
}

/// `(issue id, board id)` of a synced issue of `team_id`.
fn issue_by_identifier(team_id: &str, identifier: &str, cx: &App) -> Option<(String, String)> {
    let store = Store::try_global(cx)?;
    store
        .collections()
        .issue_refs_in_team(team_id, cx)
        .into_iter()
        .find(|issue| issue.identifier.eq_ignore_ascii_case(identifier))
        .map(|issue| (issue.id.clone(), issue.board_id.clone()))
}

fn board_by_slug(team_id: &str, board_slug: &str, cx: &App) -> Option<String> {
    let store = Store::try_global(cx)?;
    let boards = store.collections().boards.read(cx);
    boards
        .iter()
        .find(|board| {
            board.team_id == team_id
                && board.slug.as_deref().is_some_and(|slug| slug.eq_ignore_ascii_case(board_slug))
        })
        .map(|board| board.id.clone())
}

/// An in-app href as an absolute URL, for the browser: the shorthand
/// expanded, a path against the origin, an absolute URL as written.
fn absolute_url(href: &str, team: Option<&str>, origin: &str) -> Option<String> {
    let href = href.trim();
    let path = if href.starts_with('~') {
        expand_team_link(href, team)?
    } else if href.starts_with('/') {
        href.to_string()
    } else {
        return Some(href.to_string());
    };
    (!origin.is_empty()).then(|| format!("{origin}{path}"))
}

/// The eUI URL policy resolves a relative `~/x` against its base into
/// `<origin>/~/x`: the shorthand back, `None` for any other URL.
fn team_shorthand_of(url: &str, origin: &str) -> Option<String> {
    if origin.is_empty() || !url.get(..origin.len())?.eq_ignore_ascii_case(origin) {
        return None;
    }
    url[origin.len()..].strip_prefix("/~/").map(|rest| format!("~/{rest}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = "https://app.exponential.at";

    #[test]
    fn the_browser_gets_an_absolute_url() {
        assert_eq!(
            absolute_url("/t/acme/settings/billing?x=1", None, ORIGIN).as_deref(),
            Some("https://app.exponential.at/t/acme/settings/billing?x=1")
        );
        assert_eq!(
            absolute_url(" ~/settings/billing ", Some("my team"), ORIGIN).as_deref(),
            Some("https://app.exponential.at/t/my%20team/settings/billing")
        );
        assert_eq!(
            absolute_url("https://app.exponential.at/admin", None, ORIGIN).as_deref(),
            Some("https://app.exponential.at/admin")
        );
        assert_eq!(absolute_url("/admin", None, ""), None);
    }

    #[test]
    fn the_shorthand_survives_the_url_policy() {
        assert_eq!(
            team_shorthand_of("https://App.Exponential.at/~/issues/EXP-1?x=1", ORIGIN).as_deref(),
            Some("~/issues/EXP-1?x=1")
        );
        assert_eq!(team_shorthand_of("https://app.exponential.at/t/acme/inbox", ORIGIN), None);
        assert_eq!(team_shorthand_of("https://other.example/~/inbox", ORIGIN), None);
        assert_eq!(team_shorthand_of("/~/inbox", ""), None);
    }

    #[test]
    fn settings_links_name_their_section() {
        use crate::settings::section_for_link;
        assert_eq!(section_for_link("members"), Some(SettingsSection::Members));
        assert_eq!(section_for_link("labels"), Some(SettingsSection::Labels));
        assert_eq!(section_for_link("security"), Some(SettingsSection::ApiKeys));
        assert_eq!(section_for_link("agents"), Some(SettingsSection::Agents));
        assert_eq!(section_for_link("assistant"), Some(SettingsSection::Voice));
        assert_eq!(section_for_link("billing"), None);
        assert_eq!(section_for_link("import"), None);
    }
}
