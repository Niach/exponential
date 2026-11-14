//! EXP-1459: what this install could make of a sealed value (a message body,
//! a card, a session title; `packages/domain-contract/fixtures/sealed-content.json`).
//! `sync::sealed` opens rows at ingest and stamps this state; the renderers
//! branch on it (the locked row).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

/// #EXP-1683: a session upload this process made: what its line needs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentUpload {
    pub filename: String,
    pub content_type: String,
}

static UPLOADS: LazyLock<Mutex<HashMap<String, AttachmentUpload>>> = LazyLock::new(Default::default);

/// Recorded by the session upload routes for [`with_attachment_embeds`].
pub fn remember_upload(id: &str, filename: &str, content_type: &str) {
    UPLOADS
        .lock()
        .unwrap()
        .insert(id.to_string(), AttachmentUpload { filename: filename.to_string(), content_type: content_type.to_string() });
}

pub fn recorded_upload(id: &str) -> Option<AttachmentUpload> {
    UPLOADS.lock().unwrap().get(id).cloned()
}

/// `text` with a line per attachment it does not reference yet
/// (`sealed-content.json` `attachmentEmbedCases`): the server never appends
/// to a sealed body, so the writer does before sealing.
pub fn with_attachment_embeds(
    text: &str,
    attachment_ids: &[String],
    upload_of: impl Fn(&str) -> Option<AttachmentUpload>,
) -> String {
    const INLINE_IMAGE_TYPES: [&str; 5] = ["image/png", "image/jpeg", "image/webp", "image/gif", "image/avif"];
    let mut seen = Vec::new();
    let mut parts = vec![text.trim_end().to_string()];
    for id in attachment_ids {
        if seen.contains(&id) || text.contains(&format!("/api/attachments/{id}")) {
            continue;
        }
        seen.push(id);
        let upload = upload_of(id);
        let name = upload.as_ref().map_or("image", |upload| upload.filename.as_str()).replace(['[', ']'], "");
        let image = upload.as_ref().is_none_or(|upload| INLINE_IMAGE_TYPES.contains(&upload.content_type.as_str()));
        parts.push(format!("{}[{name}](/api/attachments/{id})", if image { "!" } else { "" }));
    }
    if seen.is_empty() {
        return text.to_string();
    }
    parts.retain(|part| !part.is_empty());
    parts.join("\n\n")
}

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

impl SealState {
    /// Why the content stays locked (`placeholder.reasons`); `None` = opened.
    pub fn locked_reason(self) -> Option<LockedReason> {
        match self {
            Self::Opened => None,
            Self::Locked => Some(LockedReason::NoKey),
            Self::Unreadable => Some(LockedReason::Unreadable),
        }
    }
}

/// `sealed-content.json` `placeholder`: why content stays locked here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LockedReason {
    /// Waiting for a holder's wrap: it opens by itself.
    NoKey,
    /// Unsupported, malformed or refused: it never opens here.
    Unreadable,
}

/// The locked state's icon concept and copy (`placeholder`).
#[derive(Debug, Clone, Deserialize)]
pub struct Placeholder {
    pub icon: String,
    pub copy: PlaceholderCopy,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceholderCopy {
    pub no_key: String,
    pub unreadable: String,
}

impl Placeholder {
    pub fn text(&self, reason: LockedReason) -> &str {
        match reason {
            LockedReason::NoKey => &self.copy.no_key,
            LockedReason::Unreadable => &self.copy.unreadable,
        }
    }
}

pub(crate) const SEALED_CONTENT_JSON: &str =
    include_str!("../../../../../packages/domain-contract/fixtures/sealed-content.json");

pub static PLACEHOLDER: LazyLock<Placeholder> = LazyLock::new(|| {
    let fixture: serde_json::Value = serde_json::from_str(SEALED_CONTENT_JSON).expect("sealed-content.json parses");
    serde_json::from_value(fixture["placeholder"].clone()).expect("sealed-content.json placeholder")
});

/// A row's locked reason (`None` = plaintext or opened).
pub fn locked_reason(state: Option<SealState>) -> Option<LockedReason> {
    state.and_then(SealState::locked_reason)
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

/// The `#IDENT` issue references in `text` (web `lib/issue-refs.ts`
/// HASH_REF_SOURCE + REF_TAIL: `#` not glued to a word or `#`, a letter-led
/// prefix, `-`, digits, then a token boundary), uppercased, unique.
pub fn issue_refs(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let glued = i > 0 && (word(chars[i - 1]) || chars[i - 1] == '#');
        if chars[i] != '#' || glued || !chars.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic()) {
            i += 1;
            continue;
        }
        let mut j = i + 2;
        while j < chars.len() && chars[j].is_ascii_alphanumeric() {
            j += 1;
        }
        let digits = j + 1;
        let mut k = digits;
        while k < chars.len() && chars[k].is_ascii_digit() {
            k += 1;
        }
        let tail_ok = !chars.get(k).is_some_and(|&c| word(c) || c == '-');
        if chars.get(j) == Some(&'-') && k > digits && tail_ok {
            let ident: String = chars[i + 1..k].iter().collect::<String>().to_uppercase();
            if !out.contains(&ident) {
                out.push(ident);
            }
        }
        i = k.max(i + 1);
    }
    out
}

/// EXP-1458 `refIssueIds`: the referenced issues the writer can see
/// (`issues`: `(id, identifier)`), unresolved refs dropped.
pub fn ref_issue_ids<'a>(text: &str, issues: impl IntoIterator<Item = (&'a str, &'a str)> + Clone) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for ident in issue_refs(text) {
        if let Some((id, _)) = issues.clone().into_iter().find(|(_, known)| known.eq_ignore_ascii_case(&ident)) {
            if !ids.iter().any(|seen| seen == id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

/// #FEED-248 (`sealed-content.json` `staleTree.refusal`): the server's
/// CONFLICT for a key write (an inline `seal` or `sessionKeys.create`) into
/// a tree that already moved on. A writer that gets it resolves the tree
/// from the server and re-seals once by itself.
pub const STALE_TREE_REFUSAL: &str = "This chat's encryption key changed. Try again in a moment.";

/// [`STALE_TREE_REFUSAL`], or what older servers said (`The tree is at epoch N`).
pub fn is_stale_tree_refusal(message: &str) -> bool {
    static OLDER: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"^The tree is at epoch \d+$").unwrap());
    message == STALE_TREE_REFUSAL || OLDER.is_match(message)
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
    fn issue_refs_follow_the_web_token() {
        let text = "see #EXP-12, #exp-12 and (#APP-3). not a#EXP-4, ##EXP-5, #EXP-6abc, #EXP-7-2, EXP-8";
        assert_eq!(issue_refs(text), vec!["EXP-12", "APP-3"]);
        let issues = [("i1", "EXP-12"), ("i2", "OTHER-1")];
        assert_eq!(ref_issue_ids(text, issues.iter().map(|(a, b)| (*a, *b))), vec!["i1"]);
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

#[cfg(test)]
mod attachment_embed_tests {
    use super::*;

    #[test]
    fn attachment_lines_replay_the_fixture() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/sealed-content.json"
        ))
        .unwrap();
        let types: Vec<&str> = fixture["inlineImageTypes"].as_array().unwrap().iter().map(|t| t.as_str().unwrap()).collect();
        assert_eq!(types, ["image/png", "image/jpeg", "image/webp", "image/gif", "image/avif"]);
        for case in fixture["attachmentEmbedCases"].as_array().unwrap() {
            let uploads: HashMap<String, AttachmentUpload> = serde_json::from_value(case["uploads"].clone()).unwrap();
            let ids: Vec<String> = serde_json::from_value(case["ids"].clone()).unwrap();
            let got = with_attachment_embeds(case["text"].as_str().unwrap(), &ids, |id| uploads.get(id).cloned());
            assert_eq!(got, case["expected"].as_str().unwrap(), "{}", case["name"]);
        }
    }

    #[test]
    fn the_stale_tree_refusal_is_the_fixtures() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../packages/domain-contract/fixtures/sealed-content.json"
        ))
        .unwrap();
        assert_eq!(fixture["staleTree"]["refusal"], STALE_TREE_REFUSAL);
        assert!(is_stale_tree_refusal(STALE_TREE_REFUSAL));
        assert!(is_stale_tree_refusal("The tree is at epoch 3"));
        assert!(!is_stale_tree_refusal("The tree is at epoch 3, really"));
        assert!(!is_stale_tree_refusal("Session not found"));
    }
}
