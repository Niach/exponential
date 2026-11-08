//! #EXP-1674: a bare in-app URL in agent prose reads as the chip of what it
//! names. Web `lib/prose-links.ts`'s twin, locked ×4 by
//! `packages/domain-contract/fixtures/prose-links.json` (its `description` is
//! the rule). Display text only: nothing here ever serializes.

use crate::app_link::{classify_app_link_in_team, AppLink};

const MARKER: &str = "#exp-chip-";
const STOPS: &[u8] = b" \t<>()[]`\"'";
const TRAILING: &[u8] = b".,;:!?*_~";

/// What a chip names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProseChipKind {
    Issue,
    Board,
    Session,
    Agent,
}

impl ProseChipKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProseChipKind::Issue => "issue",
            ProseChipKind::Board => "board",
            ProseChipKind::Session => "session",
            ProseChipKind::Agent => "agent",
        }
    }
}

fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The first three characters of a fence line (at most 3 leading spaces).
fn fence_mark(line: &str) -> Option<&str> {
    let spaces = line.bytes().take(3).take_while(|byte| *byte == b' ').count();
    let rest = &line[spaces..];
    (rest.starts_with("```") || rest.starts_with("~~~")).then(|| &rest[..3])
}

fn escape_title(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for ch in title.chars() {
        if matches!(ch, '\\' | '[' | ']' | '*' | '_' | '`' | '~') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn chip_of(url: &str, origin: &str, team: &str, resolve: &dyn Fn(ProseChipKind, &str) -> Option<String>) -> Option<String> {
    let (slug, kind, id) = match classify_app_link_in_team(url, origin, Some(team)) {
        AppLink::Issue { team_slug, identifier, .. } | AppLink::IssueRef { team_slug, identifier } => {
            (team_slug, ProseChipKind::Issue, identifier)
        }
        AppLink::Board { team_slug, board_slug } => (team_slug, ProseChipKind::Board, board_slug),
        AppLink::Session { team_slug, session_id } => (team_slug, ProseChipKind::Session, session_id),
        AppLink::Agent { team_slug, agent_id } => (team_slug, ProseChipKind::Agent, agent_id),
        _ => return None,
    };
    if !slug.eq_ignore_ascii_case(team) {
        return None;
    }
    let title = resolve(kind, &id)?;
    if kind == ProseChipKind::Issue {
        return Some(format!("#{id}"));
    }
    let base = url.split('#').next().unwrap_or(url);
    Some(format!("[{}]({base}{MARKER}{})", escape_title(&title), kind.as_str()))
}

fn chip_line(line: &str, origin: &str, team: &str, resolve: &dyn Fn(ProseChipKind, &str) -> Option<String>, out: &mut String) {
    let bytes = line.as_bytes();
    let mut at = 0;
    let mut copied = 0;
    while at < bytes.len() {
        if bytes[at] == b'`' {
            let mut run = at;
            while bytes.get(run) == Some(&b'`') {
                run += 1;
            }
            let width = run - at;
            let mut close = run;
            let mut end = None;
            while close < bytes.len() {
                if bytes[close] != b'`' {
                    close += 1;
                    continue;
                }
                let mut close_run = close;
                while bytes.get(close_run) == Some(&b'`') {
                    close_run += 1;
                }
                if close_run - close == width {
                    end = Some(close_run);
                    break;
                }
                close = close_run;
            }
            at = end.unwrap_or(run);
            continue;
        }
        let head = &bytes[at..bytes.len().min(at + 8)];
        let is_url = head.len() >= 7 && (head[..7].eq_ignore_ascii_case(b"http://") || (head.len() == 8 && head.eq_ignore_ascii_case(b"https://")));
        if is_url {
            let prev = at.checked_sub(1).map(|p| bytes[p]);
            let destination = prev == Some(b'(') && at > 1 && bytes[at - 2] == b']';
            if !prev.is_some_and(|byte| is_word(byte) || byte == b'[' || byte == b'<') && !destination {
                let mut end = at;
                while end < bytes.len() && !STOPS.contains(&bytes[end]) {
                    end += 1;
                }
                while end > at && TRAILING.contains(&bytes[end - 1]) {
                    end -= 1;
                }
                if let Some(chip) = chip_of(&line[at..end], origin, team, resolve) {
                    out.push_str(&line[copied..at]);
                    out.push_str(&chip);
                    copied = end;
                }
                at = end;
                continue;
            }
        }
        at += 1;
    }
    out.push_str(&line[copied..]);
}

/// `text` with its bare in-app URLs rewritten to their chips' sources.
/// `resolve` = the display title of a SYNCED row, `None` otherwise.
pub fn chip_bare_app_links(
    text: &str,
    origin: &str,
    team: Option<&str>,
    resolve: &dyn Fn(ProseChipKind, &str) -> Option<String>,
) -> String {
    let Some(team) = team.filter(|team| !team.is_empty()) else {
        return text.to_string();
    };
    let lowered = text.to_ascii_lowercase();
    if !lowered.contains("http://") && !lowered.contains("https://") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut fence: Option<String> = None;
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            out.push('\n');
        }
        if let Some(mark) = fence_mark(line) {
            match &fence {
                None => fence = Some(mark.to_string()),
                Some(open) if open == mark => fence = None,
                Some(_) => {}
            }
            out.push_str(line);
        } else if fence.is_none() {
            chip_line(line, origin, team, resolve, &mut out);
        } else {
            out.push_str(line);
        }
    }
    out
}

/// The chip kind a rewritten link's href carries (`None`: an ordinary link).
pub fn prose_chip_kind(href: &str) -> Option<ProseChipKind> {
    let at = href.rfind(MARKER)?;
    match &href[at + MARKER.len()..] {
        "board" => Some(ProseChipKind::Board),
        "session" => Some(ProseChipKind::Session),
        "agent" => Some(ProseChipKind::Agent),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Replayed ×4 (web `prose-links.test.ts`, iOS `ProseLinksTests`, Android
    /// `ProseLinksTest`).
    #[test]
    fn the_rule_replays_its_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/prose-links.json"
        ))
        .expect("prose-links.json parses");
        let origin = fixture["origin"].as_str().expect("origin");
        let team = fixture["team"].as_str().expect("team");
        let synced: HashMap<String, String> =
            serde_json::from_value(fixture["synced"].clone()).expect("synced");
        let resolve = |kind: ProseChipKind, id: &str| synced.get(&format!("{}:{id}", kind.as_str())).cloned();
        for case in fixture["cases"].as_array().expect("cases") {
            let text = case["text"].as_str().expect("text");
            assert_eq!(
                chip_bare_app_links(text, origin, Some(team), &resolve),
                case["out"].as_str().expect("out"),
                "{}",
                case["name"]
            );
        }
        for case in fixture["chipKindCases"].as_array().expect("chipKindCases") {
            assert_eq!(
                prose_chip_kind(case["href"].as_str().expect("href")).map(ProseChipKind::as_str),
                case["kind"].as_str(),
                "{}",
                case["name"]
            );
        }
    }
}
