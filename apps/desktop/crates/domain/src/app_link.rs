//! EXP-1188 — what a clicked markdown link in agent prose (narration, the
//! Guide, the thread) opens. Web `lib/app-link.ts`'s twin, byte-locked ×4 by
//! `packages/domain-contract/fixtures/app-link.json` (its `description` is the
//! rule). Hand-parsed on purpose: a URL crate's normalisation would drift
//! from the other three clients. #EXP-1674: one link system: the team
//! shorthand, the routes table and THE open rule ([`link_open_outcome`]).

/// `copy.noAccess`: what a link the viewer cannot reach says.
pub const NO_ACCESS: &str = "Not found or no access";

/// The classified target of one href.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppLink {
    /// An issue on THIS instance — open it in the app.
    Issue {
        team_slug: String,
        board_slug: String,
        identifier: String,
    },
    /// #EXP-1674: an issue by identifier alone (agents rarely know the board).
    IssueRef { team_slug: String, identifier: String },
    /// #EXP-1674: one board's issue list.
    Board { team_slug: String, board_slug: String },
    /// A run on THIS instance — open it in the app.
    Session { team_slug: String, session_id: String },
    /// #EXP-1674: THE plan page of one plan card of a session.
    Plan {
        team_slug: String,
        session_id: String,
        message_id: String,
    },
    /// EXP-1284: a thread on THIS instance — open it in the app.
    Thread { team_slug: String, thread_id: String },
    /// #EXP-1674: the chats list.
    Threads { team_slug: String },
    /// #EXP-1674: one agent's page.
    Agent { team_slug: String, agent_id: String },
    /// #EXP-1674: the team's agent page.
    AgentHome { team_slug: String },
    /// #EXP-1674: the inbox.
    Inbox { team_slug: String },
    /// #EXP-1674: the devices page (`devices` or `settings/devices`).
    Devices { team_slug: String },
    /// #EXP-1674: one settings page; `section` = the segment, empty for the
    /// Settings root.
    Settings { team_slug: String, section: String },
    /// #EXP-1674: the app's first screen in THE RUN VIEW.
    AppHome { team_slug: String, app_slug: String },
    /// VAPP-218: an app screen on THIS instance — THE RUN VIEW showing
    /// `screen` with `params` (the query, minus the editor's own keys
    /// apps.json `screens.reserved`).
    AppScreen {
        team_slug: String,
        app_slug: String,
        screen: String,
        params: std::collections::BTreeMap<String, String>,
    },
    /// Another page on THIS instance: the path as written (query + hash
    /// kept), resolved against the instance origin by the caller.
    App { path: String },
    /// Anything the OS browser/mail client should open, as given.
    External { url: String },
    /// Not openable (placeholder hosts, other schemes, `#x`, bare words).
    Ignore,
}

/// Classify `href` against the instance `origin` (`https://host[:port]`, a
/// trailing slash tolerated), with no surface team (`~/x` = ignore).
pub fn classify_app_link(href: &str, origin: &str) -> AppLink {
    classify_app_link_in_team(href, origin, None)
}

/// #EXP-1674: [`classify_app_link`] with the SURFACE'S team slug (the team of
/// the message, issue or app holding the link), which `~/x` resolves against.
pub fn classify_app_link_in_team(href: &str, origin: &str, team: Option<&str>) -> AppLink {
    let href = href.trim();
    if href.is_empty() {
        return AppLink::Ignore;
    }
    if href.starts_with('~') {
        return match expand_team_link(href, team) {
            Some(path) => in_app(&path),
            None => AppLink::Ignore,
        };
    }
    if href.starts_with('/') {
        if href.starts_with("//") {
            return AppLink::Ignore;
        }
        return in_app(href);
    }
    if starts_with_ignore_case(href, "mailto:") {
        return AppLink::External { url: href.to_string() };
    }
    let scheme_len = if starts_with_ignore_case(href, "https://") {
        "https://".len()
    } else if starts_with_ignore_case(href, "http://") {
        "http://".len()
    } else {
        return AppLink::Ignore;
    };
    let rest = &href[scheme_len..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if !valid_authority(authority) {
        return AppLink::Ignore;
    }
    let base = &href[..scheme_len + authority_end];
    let origin = origin.trim().trim_end_matches('/');
    if !origin.is_empty() && base.eq_ignore_ascii_case(origin) {
        let path = &rest[authority_end..];
        if path.starts_with('/') {
            return in_app(path);
        }
        return in_app(&format!("/{path}"));
    }
    AppLink::External { url: href.to_string() }
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// `[A-Za-z0-9-]` labels joined by `.` (two or more, or `localhost`), an
/// optional `:port`, no userinfo.
fn valid_authority(authority: &str) -> bool {
    let (host, port) = match authority.split_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if let Some(port) = port {
        if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
    }
    // At least two labels unless `localhost` — browsers punycode a
    // placeholder like `https://…` into a one-label host (`xn--rvg`).
    !host.is_empty()
        && (host.contains('.') || host.eq_ignore_ascii_case("localhost"))
        && host.split('.').all(|label| {
            !label.is_empty() && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// A path's segments: query + hash dropped, empty segments dropped, each
/// percent-decoded.
fn path_segments(path: &str) -> Vec<String> {
    path[..path.find(['?', '#']).unwrap_or(path.len())]
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(strict_decode)
        .collect()
}

/// An in-app path (`/…`, query + hash allowed): a row of the routes table,
/// else `app` with the path as written.
fn in_app(path: &str) -> AppLink {
    let segments = path_segments(path);
    let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
    if let ["t", team, rest @ ..] = segments.as_slice() {
        if let Some(link) = route(team, rest, path) {
            return link;
        }
    }
    AppLink::App { path: path.to_string() }
}

/// `routes` (app-link.json), in matching order: `rest` = the segments after
/// `/t/<team>/`, exact counts.
fn route(team: &str, rest: &[&str], path: &str) -> Option<AppLink> {
    let team_slug = team.to_string();
    Some(match rest {
        ["issues", identifier] => AppLink::IssueRef {
            team_slug,
            identifier: identifier.to_string(),
        },
        ["boards", board, "issues", identifier] => AppLink::Issue {
            team_slug,
            board_slug: board.to_string(),
            identifier: identifier.to_string(),
        },
        ["boards", board] => AppLink::Board {
            team_slug,
            board_slug: board.to_string(),
        },
        ["sessions", session, "plan", message] => AppLink::Plan {
            team_slug,
            session_id: session.to_string(),
            message_id: message.to_string(),
        },
        ["sessions", session] => AppLink::Session {
            team_slug,
            session_id: session.to_string(),
        },
        ["threads", thread] => AppLink::Thread {
            team_slug,
            thread_id: thread.to_string(),
        },
        ["threads"] => AppLink::Threads { team_slug },
        ["agents", agent] => AppLink::Agent {
            team_slug,
            agent_id: agent.to_string(),
        },
        ["agent"] => AppLink::AgentHome { team_slug },
        ["inbox"] => AppLink::Inbox { team_slug },
        ["devices"] | ["settings", "devices"] => AppLink::Devices { team_slug },
        ["settings", section] => AppLink::Settings {
            team_slug,
            section: section.to_string(),
        },
        ["settings"] => AppLink::Settings {
            team_slug,
            section: String::new(),
        },
        ["apps", app, screen] => AppLink::AppScreen {
            team_slug,
            app_slug: app.to_string(),
            screen: screen.to_string(),
            params: screen_query_params(path),
        },
        ["apps", app] => AppLink::AppHome {
            team_slug,
            app_slug: app.to_string(),
        },
        _ => return None,
    })
}

/// #EXP-1674: `~/x` = `/t/<team>/x`, or `None` when it cannot be expanded (no
/// team, not `~/`, `~//`, a `.` or `..` segment even percent-encoded).
pub fn expand_team_link(href: &str, team: Option<&str>) -> Option<String> {
    let team = team.filter(|team| !team.is_empty())?;
    if !href.starts_with("~/") || href.starts_with("~//") {
        return None;
    }
    let rest = &href[2..];
    let bare = &rest[..rest.find(['?', '#']).unwrap_or(rest.len())];
    if bare.split('/').map(strict_decode).any(|segment| segment == "." || segment == "..") {
        return None;
    }
    Some(format!("/t/{}/{rest}", encode_uri_component(team)))
}

/// `encodeURIComponent`: everything but `A-Za-z0-9-_.!~*'()` as `%XX` over
/// the UTF-8 bytes.
fn encode_uri_component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// The team an `app` link's path names (`/t/<team>/…`), for THE open rule's
/// membership check; `None` = not a team page.
pub fn app_path_team(path: &str) -> Option<String> {
    let mut segments = path_segments(path).into_iter();
    if segments.next().as_deref() != Some("t") {
        return None;
    }
    segments.next()
}

/// #EXP-1674: what opening a resolved in-app link does (`openCases`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkOpenOutcome {
    /// Opens where it is.
    Open,
    /// Found in another team of the viewer's: switch without asking, then open.
    SwitchTeam,
    /// The viewer is not a member of the link's team: home + `copy.noAccess`.
    NoAccess,
    /// A page this client has no screen for: the browser.
    Browser,
    /// Missing or unreadable: the existing not-found state, else `copy.noAccess`.
    NotFound,
}

/// #EXP-1674: what a chat card's link may open (plan-card.json `linkCases`):
/// in-app, or https out.
pub fn card_link_opens(href: &str, origin: &str, team: Option<&str>) -> bool {
    match classify_app_link_in_team(href, origin, team) {
        AppLink::Ignore => false,
        AppLink::External { url } => url.get(..8).is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://")),
        _ => true,
    }
}

/// #EXP-1674: THE open rule ×4 (`openCases`), checked in this order.
/// Unreadable looks exactly like missing.
pub fn link_open_outcome(member: bool, active_team: bool, found: bool, web_only: bool) -> LinkOpenOutcome {
    if !member {
        LinkOpenOutcome::NoAccess
    } else if web_only {
        LinkOpenOutcome::Browser
    } else if !found {
        LinkOpenOutcome::NotFound
    } else if active_team {
        LinkOpenOutcome::Open
    } else {
        LinkOpenOutcome::SwitchTeam
    }
}

/// VAPP-218: the editor's own query keys, never screen params (apps.json
/// `screens.reserved`).
const SCREEN_RESERVED: [&str; 2] = ["tab", "frame"];

/// A path's query as screen params: `k=v` pairs split on `&` then the first
/// `=`, each side percent-decoded, empty keys dropped, the last repeat wins,
/// the editor's own keys dropped (sorted by key: a `BTreeMap`).
fn screen_query_params(path: &str) -> std::collections::BTreeMap<String, String> {
    let mut found = std::collections::BTreeMap::new();
    let Some(q) = path.find('?') else {
        return found;
    };
    let rest = &path[q + 1..];
    let query = &rest[..rest.find('#').unwrap_or(rest.len())];
    for pair in query.split('&') {
        let (key, value) = match pair.split_once('=') {
            Some((key, value)) => (strict_decode(key), strict_decode(value)),
            None => (strict_decode(pair), String::new()),
        };
        if key.is_empty() || SCREEN_RESERVED.contains(&key.as_str()) {
            continue;
        }
        found.insert(key, value);
    }
    found
}

/// `decodeURIComponent`, or the text as written when any escape is
/// malformed or the bytes are not UTF-8 (the TS reference keeps it whole).
fn strict_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = |b: u8| (b as char).to_digit(16);
            match (bytes.get(index + 1).copied().and_then(hex), bytes.get(index + 2).copied().and_then(hex)) {
                (Some(high), Some(low)) => {
                    out.push((high * 16 + low) as u8);
                    index += 3;
                }
                _ => return text.to_string(),
            }
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// The ONE contract fixture, replayed on every client (web
    /// `app-link.test.ts`, iOS `AppLinkTests`, Android `AppLinkTest`).
    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/app-link.json");

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FixtureLink {
        kind: String,
        url: Option<String>,
        path: Option<String>,
        team_slug: Option<String>,
        board_slug: Option<String>,
        identifier: Option<String>,
        session_id: Option<String>,
        message_id: Option<String>,
        thread_id: Option<String>,
        agent_id: Option<String>,
        section: Option<String>,
        app_slug: Option<String>,
        screen: Option<String>,
        params: Option<std::collections::BTreeMap<String, String>>,
    }

    impl FixtureLink {
        fn expected(self) -> AppLink {
            match self.kind.as_str() {
                "issue" => AppLink::Issue {
                    team_slug: self.team_slug.unwrap(),
                    board_slug: self.board_slug.unwrap(),
                    identifier: self.identifier.unwrap(),
                },
                "issueRef" => AppLink::IssueRef {
                    team_slug: self.team_slug.unwrap(),
                    identifier: self.identifier.unwrap(),
                },
                "board" => AppLink::Board {
                    team_slug: self.team_slug.unwrap(),
                    board_slug: self.board_slug.unwrap(),
                },
                "session" => AppLink::Session {
                    team_slug: self.team_slug.unwrap(),
                    session_id: self.session_id.unwrap(),
                },
                "plan" => AppLink::Plan {
                    team_slug: self.team_slug.unwrap(),
                    session_id: self.session_id.unwrap(),
                    message_id: self.message_id.unwrap(),
                },
                "thread" => AppLink::Thread {
                    team_slug: self.team_slug.unwrap(),
                    thread_id: self.thread_id.unwrap(),
                },
                "threads" => AppLink::Threads { team_slug: self.team_slug.unwrap() },
                "agent" => AppLink::Agent {
                    team_slug: self.team_slug.unwrap(),
                    agent_id: self.agent_id.unwrap(),
                },
                "agentHome" => AppLink::AgentHome { team_slug: self.team_slug.unwrap() },
                "inbox" => AppLink::Inbox { team_slug: self.team_slug.unwrap() },
                "devices" => AppLink::Devices { team_slug: self.team_slug.unwrap() },
                "settings" => AppLink::Settings {
                    team_slug: self.team_slug.unwrap(),
                    section: self.section.unwrap(),
                },
                "appHome" => AppLink::AppHome {
                    team_slug: self.team_slug.unwrap(),
                    app_slug: self.app_slug.unwrap(),
                },
                "appScreen" => AppLink::AppScreen {
                    team_slug: self.team_slug.unwrap(),
                    app_slug: self.app_slug.unwrap(),
                    screen: self.screen.unwrap(),
                    params: self.params.unwrap(),
                },
                "app" => AppLink::App { path: self.path.unwrap() },
                "external" => AppLink::External { url: self.url.unwrap() },
                "ignore" => AppLink::Ignore,
                other => panic!("unknown fixture kind {other}"),
            }
        }
    }

    /// A present field, `null` included: `Some(None)` = no team, an absent
    /// field stays `None` = the fixture's top-level team.
    fn present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<Option<String>>, D::Error> {
        Option::<String>::deserialize(deserializer).map(Some)
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        href: String,
        #[serde(default, deserialize_with = "present")]
        team: Option<Option<String>>,
        link: FixtureLink,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct OpenInput {
        member: bool,
        active_team: bool,
        found: bool,
        web_only: bool,
    }

    #[derive(Deserialize)]
    struct OpenCase {
        name: String,
        input: OpenInput,
        outcome: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct LinkCopy {
        no_access: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        origin: String,
        team: Option<String>,
        cases: Vec<Case>,
        open_cases: Vec<OpenCase>,
        copy: LinkCopy,
    }

    #[test]
    fn app_link_fixture_cases() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("app-link.json parses");
        assert!(!fixture.cases.is_empty());
        let default_team = fixture.team.clone();
        let mut failures = Vec::new();
        for case in fixture.cases {
            let team = case.team.unwrap_or_else(|| default_team.clone());
            let got = classify_app_link_in_team(&case.href, &fixture.origin, team.as_deref());
            let want = case.link.expected();
            if got != want {
                failures.push(format!("{}: got {got:?}, want {want:?}", case.name));
            }
        }
        assert!(failures.is_empty(), "app-link fixture failures:\n{}", failures.join("\n"));
    }

    #[test]
    fn card_link_fixture_cases() {
        #[derive(Deserialize)]
        struct LinkCase {
            name: String,
            href: String,
            #[serde(default, deserialize_with = "present")]
            team: Option<Option<String>>,
            opens: bool,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct CardFixture {
            link_origin: String,
            link_team: String,
            link_cases: Vec<LinkCase>,
        }
        // Absent = the fixture's team; null = no team.
        fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<String>>, D::Error> {
            Option::<String>::deserialize(d).map(Some)
        }
        let fixture: CardFixture = serde_json::from_str(include_str!("../../../../../packages/domain-contract/fixtures/plan-card.json"))
            .expect("plan-card.json parses");
        assert!(!fixture.link_cases.is_empty());
        for case in fixture.link_cases {
            let team = case.team.unwrap_or_else(|| Some(fixture.link_team.clone()));
            assert_eq!(card_link_opens(&case.href, &fixture.link_origin, team.as_deref()), case.opens, "{}", case.name);
        }
    }

    #[test]
    fn app_link_fixture_open_cases() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("app-link.json parses");
        assert!(!fixture.open_cases.is_empty());
        assert_eq!(fixture.copy.no_access, NO_ACCESS);
        for case in fixture.open_cases {
            let want = match case.outcome.as_str() {
                "open" => LinkOpenOutcome::Open,
                "switchTeam" => LinkOpenOutcome::SwitchTeam,
                "noAccess" => LinkOpenOutcome::NoAccess,
                "browser" => LinkOpenOutcome::Browser,
                "notFound" => LinkOpenOutcome::NotFound,
                other => panic!("unknown fixture outcome {other}"),
            };
            let input = &case.input;
            let got = link_open_outcome(input.member, input.active_team, input.found, input.web_only);
            assert_eq!(got, want, "{}", case.name);
        }
    }

    #[test]
    fn app_path_team_reads_a_team_page() {
        assert_eq!(app_path_team("/t/my%20team/reviews?tab=mine").as_deref(), Some("my team"));
        assert_eq!(app_path_team("/t/acme").as_deref(), Some("acme"));
        assert_eq!(app_path_team("/settings/account"), None);
        assert_eq!(app_path_team("/t"), None);
        assert_eq!(app_path_team("/"), None);
    }

    #[test]
    fn the_team_encodes_like_encode_uri_component() {
        assert_eq!(encode_uri_component("a b/ü-_.!~*'()"), "a%20b%2F%C3%BC-_.!~*'()");
        assert_eq!(expand_team_link("~/inbox", Some("")), None);
    }

    #[test]
    fn app_link_origin_trailing_slash_and_bare_origin() {
        assert_eq!(
            classify_app_link("https://app.exponential.at", "https://app.exponential.at/"),
            AppLink::App { path: "/".into() }
        );
        assert_eq!(
            classify_app_link("https://app.exponential.at?x=1", "https://app.exponential.at"),
            AppLink::App { path: "/?x=1".into() }
        );
        assert_eq!(strict_decode("a%2"), "a%2");
        assert_eq!(strict_decode("a%zz"), "a%zz");
        assert_eq!(strict_decode("a%2Fb%zz"), "a%2Fb%zz");
    }
}
