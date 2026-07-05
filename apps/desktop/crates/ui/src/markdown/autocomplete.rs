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

use gpui::{App, SharedString};
use sync::Store;

/// What opened the completion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionTrigger {
    /// `@` — workspace members; inserts `@<email>`.
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

/// The default [`CompletionSource`]: workspace members (⨝ users) for `@`,
/// the workspace's issues for `#`, both read live from the §05 collections.
pub fn store_completion_source(workspace_id: impl Into<String>) -> Rc<dyn CompletionSource> {
    Rc::new(StoreCompletionSource {
        workspace_id: workspace_id.into(),
    })
}

struct StoreCompletionSource {
    workspace_id: String,
}

impl CompletionSource for StoreCompletionSource {
    fn query(&self, trigger: CompletionTrigger, query: &str, cx: &App) -> Vec<CompletionItem> {
        let collections = Store::global(cx).collections();
        match trigger {
            CompletionTrigger::Mention => {
                let needle = query.to_lowercase();
                let members = collections.workspace_members.read(cx);
                let users = collections.users.read(cx);
                let mut items: Vec<CompletionItem> = members
                    .iter()
                    .filter(|m| m.workspace_id == self.workspace_id)
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
                let ident_needle = query.to_uppercase();
                let title_needle = query.to_lowercase();
                let mut issues = collections.issues_in_workspace(&self.workspace_id, cx);
                issues.retain(|issue| {
                    query.is_empty()
                        || issue.identifier.to_uppercase().starts_with(&ident_needle)
                        || issue.title.to_lowercase().starts_with(&title_needle)
                });
                issues.sort_by(|a, b| sync::cmp_identifiers(&a.identifier, &b.identifier));
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
}
