//! EXP-1188 — what a clicked markdown link in agent prose (narration, the
//! Guide, the thread) opens. Web `lib/app-link.ts`'s twin, byte-locked ×4 by
//! `packages/domain-contract/fixtures/app-link.json` (its `description` is the
//! rule). Hand-parsed on purpose: a URL crate's normalisation would drift
//! from the other three clients.

/// The classified target of one href.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppLink {
    /// An issue on THIS instance — open it in the app.
    Issue {
        team_slug: String,
        board_slug: String,
        identifier: String,
    },
    /// A run on THIS instance — open it in the app.
    Session { team_slug: String, session_id: String },
    /// Another page on THIS instance: the path as written (query + hash
    /// kept), resolved against the instance origin by the caller.
    App { path: String },
    /// Anything the OS browser/mail client should open, as given.
    External { url: String },
    /// Not openable (placeholder hosts, other schemes, `#x`, bare words).
    Ignore,
}

/// Classify `href` against the instance `origin` (`https://host[:port]`, a
/// trailing slash tolerated).
pub fn classify_app_link(href: &str, origin: &str) -> AppLink {
    let href = href.trim();
    if href.is_empty() {
        return AppLink::Ignore;
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

/// An in-app path (`/…`, query + hash allowed) → issue | session | app.
fn in_app(path: &str) -> AppLink {
    let bare = &path[..path.find(['?', '#']).unwrap_or(path.len())];
    let segments: Vec<String> = bare
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(percent_decode)
        .collect();
    match segments.as_slice() {
        [t, team, boards, board, issues, identifier]
            if t == "t" && boards == "boards" && issues == "issues" =>
        {
            AppLink::Issue {
                team_slug: team.clone(),
                board_slug: board.clone(),
                identifier: identifier.clone(),
            }
        }
        [t, team, sessions, id] if t == "t" && sessions == "sessions" => AppLink::Session {
            team_slug: team.clone(),
            session_id: id.clone(),
        },
        _ => AppLink::App { path: path.to_string() },
    }
}

/// `%XX` → byte; a malformed escape stays literal; invalid UTF-8 is lossy.
fn percent_decode(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = |b: u8| (b as char).to_digit(16);
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                out.push((high * 16 + low) as u8);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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
    }

    impl FixtureLink {
        fn expected(self) -> AppLink {
            match self.kind.as_str() {
                "issue" => AppLink::Issue {
                    team_slug: self.team_slug.unwrap(),
                    board_slug: self.board_slug.unwrap(),
                    identifier: self.identifier.unwrap(),
                },
                "session" => AppLink::Session {
                    team_slug: self.team_slug.unwrap(),
                    session_id: self.session_id.unwrap(),
                },
                "app" => AppLink::App { path: self.path.unwrap() },
                "external" => AppLink::External { url: self.url.unwrap() },
                "ignore" => AppLink::Ignore,
                other => panic!("unknown fixture kind {other}"),
            }
        }
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        href: String,
        link: FixtureLink,
    }

    #[derive(Deserialize)]
    struct Fixture {
        origin: String,
        cases: Vec<Case>,
    }

    #[test]
    fn app_link_fixture_cases() {
        let fixture: Fixture = serde_json::from_str(FIXTURE).expect("app-link.json parses");
        assert!(!fixture.cases.is_empty());
        let mut failures = Vec::new();
        for case in fixture.cases {
            let got = classify_app_link(&case.href, &fixture.origin);
            let want = case.link.expected();
            if got != want {
                failures.push(format!("{}: got {got:?}, want {want:?}", case.name));
            }
        }
        assert!(failures.is_empty(), "app-link fixture failures:\n{}", failures.join("\n"));
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
        assert_eq!(percent_decode("a%2"), "a%2");
        assert_eq!(percent_decode("a%zz"), "a%zz");
    }
}
