//! Caret-anchored `@`-member / `#`-issue autocomplete (masterplan-v3 §4.6).
//!
//! gpui-component's built-in `completion_menu` is LSP-bound, so this is the
//! standalone reusable overlay: [`detect_trigger`] finds a pending `@query`
//! or `#query` token behind the caret, a [`CompletionSource`] resolves it
//! against the live synced collections, and the editor renders the popover
//! (keyboard: ↑/↓ select, Enter/Tab accept, Esc dismiss — wired in
//! `editor.rs`). Accepting inserts the canonical interchange form: `@<email>`
//! for mentions (`apps/web/src/lib/integrations/mentions.ts` resolves it
//! server-side on save) or `#<IDENTIFIER>` for issue refs
//! (`apps/web/src/lib/issue-refs.ts`).

use std::rc::Rc;

use domain::rows::Issue;
use gpui::{App, SharedString};
use sync::Store;

/// What opened the completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionTrigger {
    /// `@` — team members; inserts `@<email>`.
    Mention,
    /// `#` — issues; inserts `#<IDENTIFIER>`.
    IssueRef,
}

/// One row of the completion popover.
#[derive(Debug, Clone, PartialEq)]
pub struct CompletionItem {
    pub trigger: CompletionTrigger,
    /// The literal text to insert (including the leading `@`/`#`).
    pub insert: String,
    /// Primary label (member name / issue identifier).
    pub label: SharedString,
    /// Secondary muted label (member email / issue title).
    pub detail: SharedString,
}

/// A `@`/`#` token being typed behind the caret.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingToken {
    pub trigger: CompletionTrigger,
    /// Byte offset of the `@`/`#` character in the input text.
    pub start: usize,
    /// The query typed so far (without the trigger char).
    pub query: String,
}

/// Live item lookup for the popover — re-queried on every keystroke against
/// the synced collections (§4.6: "Both re-query on each keystroke against
/// the live store").
pub trait CompletionSource {
    fn query(&self, trigger: CompletionTrigger, query: &str, cx: &App) -> Vec<CompletionItem>;
}

const MAX_ITEMS: usize = 8;
const MAX_QUERY_LEN: usize = 64;

/// Find a pending completion token ending at `cursor` (a byte offset into
/// `text`). Mirrors the web trigger rules: the token must start at the
/// beginning of a line or after whitespace (so `foo#EXP-1` and mid-email `@`s
/// don't trigger — the web `#` regex demands `(?<![\w#])`), and the query may
/// only contain the token charset (`[A-Za-z0-9._%+-@]` for mentions —
/// emails — and `[A-Za-z0-9-]` for issue refs).
pub fn detect_trigger(text: &str, cursor: usize) -> Option<PendingToken> {
    if cursor > text.len() || !text.is_char_boundary(cursor) {
        return None;
    }
    let before = &text[..cursor];
    // Token start: after the last whitespace before the cursor.
    let token_start = before
        .rfind(|c: char| c.is_whitespace())
        .map(|i| i + before[i..].chars().next().map_or(1, char::len_utf8))
        .unwrap_or(0);
    let token = &before[token_start..];
    let mut chars = token.chars();
    let trigger = match chars.next() {
        Some('@') => CompletionTrigger::Mention,
        Some('#') => CompletionTrigger::IssueRef,
        _ => return None,
    };
    let query = chars.as_str();
    if query.len() > MAX_QUERY_LEN {
        return None;
    }
    let valid = match trigger {
        CompletionTrigger::Mention => query
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._%+-@".contains(c)),
        CompletionTrigger::IssueRef => query
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-'),
    };
    if !valid {
        return None;
    }
    Some(PendingToken {
        trigger,
        start: token_start,
        query: query.to_string(),
    })
}

/// The default [`CompletionSource`]: team members (⨝ users) for `@`,
/// the team's issues for `#`, both read live from the §05 collections.
pub fn store_completion_source(team_id: impl Into<String>) -> Rc<dyn CompletionSource> {
    Rc::new(StoreCompletionSource {
        team_id: team_id.into(),
    })
}

struct StoreCompletionSource {
    team_id: String,
}

impl CompletionSource for StoreCompletionSource {
    fn query(&self, trigger: CompletionTrigger, query: &str, cx: &App) -> Vec<CompletionItem> {
        let collections = Store::global(cx).collections();
        match trigger {
            CompletionTrigger::Mention => {
                let needle = query.to_lowercase();
                let members = collections.team_members.read(cx);
                let users = collections.users.read(cx);
                let mut items: Vec<CompletionItem> = members
                    .iter()
                    .filter(|m| m.team_id == self.team_id)
                    .filter_map(|m| users.get(&m.user_id))
                    .filter_map(|user| {
                        let email = user.email.clone()?;
                        let name = user.name.clone().unwrap_or_else(|| email.clone());
                        let matches = needle.is_empty()
                            || email.to_lowercase().starts_with(&needle)
                            || name
                                .to_lowercase()
                                .split_whitespace()
                                .any(|word| word.starts_with(&needle));
                        matches.then(|| CompletionItem {
                            trigger,
                            insert: format!("@{email}"),
                            label: name.into(),
                            detail: email.into(),
                        })
                    })
                    .collect();
                items.sort_by(|a, b| a.label.cmp(&b.label));
                items.dedup_by(|a, b| a.insert == b.insert);
                items.truncate(MAX_ITEMS);
                items
            }
            CompletionTrigger::IssueRef => {
                let mut issues = collections.issues_in_team(&self.team_id, cx);
                filter_and_rank_issue_refs(&mut issues, query);
                issues.truncate(MAX_ITEMS);
                issues
                    .into_iter()
                    .map(|issue| CompletionItem {
                        trigger,
                        insert: format!("#{}", issue.identifier),
                        label: issue.identifier.clone().into(),
                        detail: issue.title.clone().into(),
                    })
                    .collect()
            }
        }
    }
}

/// Filter + rank `#` candidates the way the web `IssueRefProvider.search`
/// does (iOS and Android mirror it): case-insensitive SUBSTRING match on
/// identifier or title, newest-created first — so the empty query surfaces
/// the most recent work. Ties (equal or missing `created_at`) fall back to
/// the natural identifier order, highest number first, keeping the ranking
/// deterministic.
fn filter_and_rank_issue_refs(issues: &mut Vec<Issue>, query: &str) {
    let needle = query.to_lowercase();
    issues.retain(|issue| {
        needle.is_empty()
            || issue.identifier.to_lowercase().contains(&needle)
            || issue.title.to_lowercase().contains(&needle)
    });
    // `Option<String>` on ISO-8601 timestamps: lexicographic == chronological,
    // and `None` (no created_at) sorts before every `Some` — reversed here so
    // undated rows land last.
    issues.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| sync::cmp_identifiers(&b.identifier, &a.identifier))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_mention_at_start() {
        let token = detect_trigger("@ja", 3).expect("trigger");
        assert_eq!(token.trigger, CompletionTrigger::Mention);
        assert_eq!(token.start, 0);
        assert_eq!(token.query, "ja");
    }

    #[test]
    fn detects_mention_after_whitespace() {
        let token = detect_trigger("hello @jane", 11).expect("trigger");
        assert_eq!(token.start, 6);
        assert_eq!(token.query, "jane");
    }

    #[test]
    fn mention_query_may_contain_full_email() {
        let token = detect_trigger("cc @jane@example.com", 20).expect("trigger");
        assert_eq!(token.query, "jane@example.com");
    }

    #[test]
    fn detects_issue_ref() {
        let token = detect_trigger("see #EXP-1", 10).expect("trigger");
        assert_eq!(token.trigger, CompletionTrigger::IssueRef);
        assert_eq!(token.start, 4);
        assert_eq!(token.query, "EXP-1");
    }

    #[test]
    fn hash_glued_to_word_does_not_trigger() {
        assert_eq!(detect_trigger("foo#EXP-1", 9), None);
    }

    #[test]
    fn whitespace_dismisses() {
        assert_eq!(detect_trigger("@jane done", 10), None);
    }

    #[test]
    fn newline_bounds_the_token() {
        let token = detect_trigger("line1\n@bo", 9).expect("trigger");
        assert_eq!(token.start, 6);
        assert_eq!(token.query, "bo");
    }

    #[test]
    fn invalid_chars_dismiss() {
        assert_eq!(detect_trigger("#EXP_1", 6), None);
        assert_eq!(detect_trigger("@ja!ne", 6), None);
    }

    #[test]
    fn empty_query_triggers() {
        let token = detect_trigger("@", 1).expect("trigger");
        assert_eq!(token.query, "");
    }

    #[test]
    fn cursor_mid_multibyte_char_is_safe() {
        // "é" is 2 bytes; offset 2 is inside it.
        assert_eq!(detect_trigger("@é", 2), None);
    }

    // -- filter_and_rank_issue_refs (web IssueRefProvider.search parity) ----

    fn issue(identifier: &str, title: &str, created_at: Option<&str>) -> Issue {
        serde_json::from_value(serde_json::json!({
            "id": identifier,
            "board_id": "p1",
            "number": 1,
            "identifier": identifier,
            "title": title,
            "status": "todo",
            "created_at": created_at,
        }))
        .expect("issue fixture")
    }

    fn identifiers(issues: &[Issue]) -> Vec<&str> {
        issues.iter().map(|i| i.identifier.as_str()).collect()
    }

    #[test]
    fn issue_refs_match_title_substring() {
        let mut issues = vec![
            issue("EXP-1", "Fix login flow", Some("2026-07-01T00:00:00Z")),
            issue("EXP-2", "Broken image upload", Some("2026-07-02T00:00:00Z")),
        ];
        filter_and_rank_issue_refs(&mut issues, "login");
        assert_eq!(identifiers(&issues), vec!["EXP-1"]);
    }

    #[test]
    fn issue_refs_match_identifier_substring_case_insensitively() {
        let mut issues = vec![
            issue("EXP-1", "a", Some("2026-07-01T00:00:00Z")),
            issue("EXP-12", "b", Some("2026-07-02T00:00:00Z")),
            issue("EXP-3", "c", Some("2026-07-03T00:00:00Z")),
        ];
        filter_and_rank_issue_refs(&mut issues, "xp-1");
        assert_eq!(identifiers(&issues), vec!["EXP-12", "EXP-1"]);
    }

    #[test]
    fn issue_refs_rank_newest_first_and_empty_query_keeps_all() {
        let mut issues = vec![
            issue("EXP-1", "oldest", Some("2026-06-01T00:00:00Z")),
            issue("EXP-2", "undated", None),
            issue("EXP-3", "newest", Some("2026-07-06T00:00:00Z")),
        ];
        filter_and_rank_issue_refs(&mut issues, "");
        assert_eq!(identifiers(&issues), vec!["EXP-3", "EXP-1", "EXP-2"]);
    }

    #[test]
    fn issue_refs_tie_break_on_identifier_number_desc() {
        let same = Some("2026-07-01T00:00:00Z");
        let mut issues = vec![
            issue("EXP-2", "a", same),
            issue("EXP-10", "b", same),
            issue("EXP-9", "c", same),
        ];
        filter_and_rank_issue_refs(&mut issues, "exp");
        assert_eq!(identifiers(&issues), vec!["EXP-10", "EXP-9", "EXP-2"]);
    }

    #[test]
    fn issue_refs_without_match_are_dropped() {
        let mut issues = vec![issue("EXP-1", "Fix login flow", None)];
        filter_and_rank_issue_refs(&mut issues, "zzz");
        assert!(issues.is_empty());
    }
}
