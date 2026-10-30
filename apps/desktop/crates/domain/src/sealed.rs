//! EXP-1459: what this install could make of a sealed value (a message body,
//! a card, a session title; `packages/domain-contract/fixtures/sealed-content.json`).
//! `sync::sealed` opens rows at ingest and stamps this state; the renderers
//! branch on it (the locked row).

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SealState {
    /// Opened with a tree key this install holds: the row reads as plaintext.
    Opened,
    /// `noKey`: sealed for an epoch this install holds no key for yet; a
    /// holder wraps it automatically, the row re-opens then.
    Locked,
    /// `unsupported` / `malformed` / `refused`: never shown as text.
    Unreadable,
}

impl SealState {
    /// The contract's open status (`plain` is no state at all).
    pub fn from_status(status: &str) -> Option<Self> {
        match status {
            "plain" => None,
            "opened" => Some(Self::Opened),
            "noKey" => Some(Self::Locked),
            _ => Some(Self::Unreadable),
        }
    }

    /// The content is not available on this install.
    pub fn is_closed(self) -> bool {
        self != Self::Opened
    }
}

/// `true` when a row's sealed content could not be opened here.
pub fn is_closed(state: Option<SealState>) -> bool {
    state.is_some_and(SealState::is_closed)
}

/// The `@<email>` mention token (web `lib/mention-refs.ts` MENTION_SOURCE).
const MENTION_SOURCE: &str = r"@([A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,})";

/// Unique, lowercase emails mentioned in `text`, in order of appearance.
pub fn mention_emails(text: &str) -> Vec<String> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(MENTION_SOURCE).expect("mention regex"));
    let mut out: Vec<String> = Vec::new();
    for caps in re.captures_iter(text) {
        let email = caps[1].to_lowercase();
        if !out.contains(&email) {
            out.push(email);
        }
    }
    out
}

/// EXP-1458: the server cannot read a sealed body, so the writer sends
/// `mentionUserIds`: every mentioned email that resolves to a user it can
/// see (`users`: `(id, email)`), unresolved tokens dropped.
pub fn mention_user_ids<'a>(text: &str, users: impl IntoIterator<Item = (&'a str, &'a str)> + Clone) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for email in mention_emails(text) {
        let found = users.clone().into_iter().find(|(_, user_email)| user_email.eq_ignore_ascii_case(&email));
        if let Some((id, _)) = found {
            if !ids.iter().any(|known| known == id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_resolve_to_visible_users_once() {
        let users = [("u1", "Ann@Example.com"), ("u2", "bob@example.com")];
        let text = "ping @ann@example.com and @ANN@example.com, @nobody@example.com, mail bob@example.com";
        assert_eq!(mention_emails(text), vec!["ann@example.com", "nobody@example.com"]);
        assert_eq!(mention_user_ids(text, users.iter().map(|(a, b)| (*a, *b))), vec!["u1"]);
    }

    #[test]
    fn statuses_map_onto_states() {
        assert_eq!(SealState::from_status("plain"), None);
        assert_eq!(SealState::from_status("opened"), Some(SealState::Opened));
        assert_eq!(SealState::from_status("noKey"), Some(SealState::Locked));
        for status in ["unsupported", "malformed", "refused"] {
            assert_eq!(SealState::from_status(status), Some(SealState::Unreadable));
        }
    }
}
