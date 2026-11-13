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
    app_path_team, card_link_opens, classify_app_link, classify_app_link_in_team, expand_team_link, link_open_outcome,
    AppLink, LinkOpenOutcome, NO_ACCESS,
};
use eui_core::host::TextLink;
use gpui::{App, Window};
use serde_json::Value;
use sync::Store;

use crate::navigation::{self, Screen};
use crate::settings::SettingsSection;

/// Where an in-app link lands.
enum Landing {
    Screen(Screen),
    Inbox,
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

/// `HostPolicy::open_url` of the eApp `source_app`: the eUI URL policy already
/// allowed `url` and made it absolute (`~/x` against the app's `link_base`).
/// In-app goes through the opener (on the active window: the host is mid-update
/// here), a link at another app's screen only through that app's `exposed`
/// contract; anything else the policy allowed goes to the OS opener, as it
/// always did.
pub(crate) fn app_url_opener(source_app: Option<String>) -> Rc<dyn Fn(&str, &mut App)> {
    Rc::new(move |url: &str, cx: &mut App| {
        let origin = link_origin(cx);
        let link = classify_app_link(url, &origin);
        if matches!(link, AppLink::External { .. } | AppLink::Ignore) {
            cx.open_url(url);
            return;
        }
        let url = url.to_string();
        let source_app = source_app.clone();
        navigation::on_active_window(cx, move |window, cx| {
            let opened = match source_app.as_deref() {
                Some(source) => cross_app_link(link, source, cx),
                None => Some(link),
            };
            match opened {
                Some(link) => open_in_app(link, absolute_url(&url, None, &origin), window, cx),
                None => no_access(window, cx),
            }
        });
    })
}

/// #EXP-1674 (eUI `cross_app_target`, host-router.json `crossApp`): a link an
/// app's surface opens: any target but another app's screen as it is; another
/// app's only through its `exposed` contract, with the params it declares.
/// `None` = `copy.noAccess`, the same for a missing app. A person's URL or an
/// agent's chat link never comes here.
fn cross_app_link(link: AppLink, source_app: &str, cx: &App) -> Option<AppLink> {
    if !matches!(link, AppLink::AppHome { .. } | AppLink::AppScreen { .. }) {
        return Some(link);
    }
    let (team_slug, app_slug, screen, params) = match &link {
        AppLink::AppHome { team_slug, app_slug } => (team_slug.clone(), app_slug.clone(), None, None),
        AppLink::AppScreen { team_slug, app_slug, screen, params } => (team_slug.clone(), app_slug.clone(), Some(screen.clone()), Some(params)),
        _ => return None,
    };
    let team_id = team_id_by_slug(&team_slug, cx)?;
    let app = crate::apps_view::find_app(&app_slug, Some(team_id.as_str()), cx)
        .filter(|app| app.row.as_ref().is_none_or(|row| row.team_id.as_deref() == Some(team_id.as_str())))?;
    if app.id == source_app {
        return Some(link.clone());
    }
    let package = (app.published_version > 0).then(|| app.row.as_ref().and_then(|row| row.package.clone())).flatten().unwrap_or(Value::Null);
    let asked = params.map(|params| Value::Object(params.iter().map(|(key, value)| (key.clone(), Value::String(value.clone()))).collect()));
    let target = eui_core::host::cross_app_target(&package, screen.as_deref(), asked.as_ref()).ok()?;
    let params = target.params.into_iter().filter_map(|(key, value)| value.as_str().map(|value| (key, value.to_string()))).collect();
    Some(AppLink::AppScreen { team_slug, app_slug, screen: target.screen, params })
}

/// #EXP-1674 (plan-card.json): `HostPlugin::open_url` of a chat card in
/// `team_id` (`None` = the active team): in-app and https only. `url` is
/// absolute: the URL policy expanded `~/x` against the card's `link_base`.
pub(crate) fn open_card_url(url: &str, team_id: Option<String>, cx: &mut App) {
    let origin = link_origin(cx);
    let href = url.to_string();
    navigation::on_active_window(cx, move |window, cx| {
        let team = team_id
            .and_then(|team_id| team_slug_by_id(&team_id, cx))
            .or_else(|| active_team_slug(window, cx));
        if card_link_opens(&href, &origin, team.as_deref()) {
            open_link(&href, team.as_deref(), window, cx);
        }
    });
}

/// #EXP-1674: the `link_base` (`/t/<team>/`) of a surface in `team_id` (`None` =
/// the active team): what `~/x` expands against in the URL policy.
pub(crate) fn team_link_base(team_id: Option<&str>, cx: &App) -> Option<String> {
    let team_id = match team_id {
        Some(team_id) => team_id.to_string(),
        None => {
            let window = cx.active_window()?;
            navigation::active_team_id(&navigation::nav_for_window_id(window.window_id(), cx)?, cx)?
        }
    };
    let team = team_slug_by_id(&team_id, cx)?;
    expand_team_link("~/", Some(&team))
}

/// #VAPP-258 (`catalog/host.json` markdown): a Markdown block image's
/// natural size when it is one of the team's synced attachments (the row's
/// probed width × height); `None` = the fixed box.
pub(crate) fn markdown_image_size(src: &str, cx: &App) -> Option<(f32, f32)> {
    crate::markdown::attachment_natural_size(src, cx)
}

/// #VAPP-258: a one-link Markdown paragraph pointing at a synced attachment
/// as a video / audio / file block (by its content type; the name = its
/// filename); `None` = the paragraph stays.
pub(crate) fn markdown_attachment(href: &str, cx: &App) -> Option<eui_gpui::host::MarkdownAttachment> {
    let id = crate::markdown::image_url::attachment_id_from_src(href)?;
    let row = Store::try_global(cx)?.collections().attachments.read(cx).get(id).cloned()?;
    let content_type = row.content_type.unwrap_or_default();
    let kind = if content_type.starts_with("video/") {
        "video"
    } else if content_type.starts_with("audio/") {
        "audio"
    } else if content_type.starts_with("image/") {
        return None;
    } else {
        "file"
    };
    Some(eui_gpui::host::MarkdownAttachment { kind: kind.into(), name: row.filename.filter(|n| !n.is_empty()).unwrap_or_else(|| "Attachment".into()) })
}

/// #EXP-1674: the `#IDENT` refs of `text` that resolve in `team_id` (`None` =
/// the active team) as the host's `text_links`; `None` = no link.
pub(crate) fn issue_text_links(text: &str, team_id: Option<&str>, cx: &App) -> Option<Vec<TextLink>> {
    if !text.contains('#') {
        return None;
    }
    let team_id = match team_id {
        Some(team_id) => team_id.to_string(),
        None => {
            let window = cx.active_window()?;
            navigation::active_team_id(&navigation::nav_for_window_id(window.window_id(), cx)?, cx)?
        }
    };
    let team = team_slug_by_id(&team_id, cx)?;
    let mut segments = Vec::new();
    let mut at = 0;
    for token in crate::markdown::scan_issue_refs(text) {
        let identifier = &text[token.start + 1..token.end];
        if issue_by_identifier(&team_id, identifier, cx).is_none() {
            continue;
        }
        let Some(href) = expand_team_link(&format!("~/issues/{identifier}"), Some(&team)) else { continue };
        if token.start > at {
            segments.push(TextLink { text: text[at..token.start].to_string(), href: None, chip: None });
        }
        segments.push(TextLink { text: text[token.clone()].to_string(), href: Some(href), chip: None });
        at = token.end;
    }
    if segments.is_empty() {
        return None;
    }
    if at < text.len() {
        segments.push(TextLink { text: text[at..].to_string(), href: None, chip: None });
    }
    Some(segments)
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
        | AppLink::Chats { team_slug }
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
    // #EXP-1804: a CI run (every check's `details_url` our CI writes) opens
    // its page here when the run is the team's synced one.
    if let AppLink::App { path } = link {
        if let Some(ci) = domain::ci_run::ci_run_link(path) {
            let synced = team_id.is_some_and(|team_id| {
                Store::try_global(cx).is_some_and(|store| {
                    store.collections().ci_runs.read(cx).get(&ci.run_id).is_some_and(|run| run.team_id.as_deref().is_none_or(|team| team == team_id))
                })
            });
            if synced {
                return found(Screen::CiRun { run_id: ci.run_id, job: ci.job_id });
            }
        }
    }
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
        AppLink::Chats { .. } => found(Screen::Chats),
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
        // #FEED-191: the Inbox is the header's popover.
        AppLink::Inbox { .. } => Target::Found(Landing::Inbox),
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
        Landing::Inbox => crate::inbox_popover::open(window, cx),
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

/// #EXP-1674 (prose-links.json): agent prose with its bare in-app URLs
/// rewritten to their chips, in `team_id`'s scope. Display text only.
pub(crate) fn chip_prose_links(text: &str, team_id: Option<&str>, cx: &App) -> String {
    use domain::prose_links::{chip_bare_app_links, ProseChipKind};
    if !text.contains("://") {
        return text.to_string();
    }
    let Some(team_id) = team_id else {
        return text.to_string();
    };
    if Store::try_global(cx).is_none() {
        return text.to_string();
    }
    let team = team_slug_by_id(team_id, cx);
    let chips = crate::markdown::RefResolver::from_store(team_id.to_string()).issue_chip;
    let resolve = |kind: ProseChipKind, id: &str| -> Option<String> {
        match kind {
            ProseChipKind::Issue => (chips)(&id.to_uppercase(), cx).map(|chip| chip.title),
            ProseChipKind::Board => {
                let store = Store::try_global(cx)?;
                let boards = store.collections().boards.read(cx);
                boards
                    .iter()
                    .find(|board| board.team_id == team_id && board.slug.as_deref().is_some_and(|slug| slug.eq_ignore_ascii_case(id)))
                    .map(|board| board.name.clone())
            }
            ProseChipKind::Session => crate::session_data::session(id, cx)
                .filter(|row| row.team_id.as_deref() == Some(team_id))
                .map(|row| row.title.filter(|title| !title.trim().is_empty()).unwrap_or_else(|| "Session".to_string())),
            ProseChipKind::Agent => crate::session_data::agent_def(id, cx)
                .filter(|row| row.team_id.as_deref() == Some(team_id))
                .and_then(|row| row.name),
        }
    };
    chip_bare_app_links(text, &link_origin(cx), team.as_deref(), &resolve)
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
