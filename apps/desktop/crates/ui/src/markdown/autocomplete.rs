//! Caret-anchored `@`-member / `#`-issue / `:`-emoji autocomplete
//! (masterplan-v3 §4.6; EXP-551 added the emoji trigger).
//!
//! gpui-component's built-in `completion_menu` is LSP-bound, so this is the
//! standalone reusable overlay: [`detect_trigger`] finds a pending `@query`
//! or `#query` token behind the caret, a [`CompletionSource`] resolves it
//! against the live synced collections, and the editor renders the popover
//! (keyboard: ↑/↓ select, Enter/Tab accept, Esc dismiss — wired in
//! `editor.rs`). Accepting inserts the canonical interchange form: `@<email>`
//! for mentions (`apps/web/src/lib/integrations/mentions.ts` resolves it
//! server-side on save), `#<IDENTIFIER>` for issue refs
//! (`apps/web/src/lib/issue-refs.ts`) or — for `:shortcode` — the emoji's
//! UNICODE, never the `:shortcode:` text (EXP-551: stored markdown is plain
//! GFM shared with clients that expand nothing).

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
    /// `:` — emoji shortcodes (EXP-551); inserts the emoji UNICODE.
    Emoji,
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
    /// Visual identity ahead of the label (EXP-426): the issue's resolved
    /// status glyph for `#`, the member's avatar for `@`.
    pub decoration: Option<CompletionDecoration>,
}

/// See [`CompletionItem::decoration`].
#[derive(Debug, Clone, PartialEq)]
pub enum CompletionDecoration {
    /// The issue's RESOLVED status (custom rows included) — resolution is
    /// per-issue, so the glyph/tint stay correct in cross-team composers.
    Status(domain::statuses::ResolvedStatus),
    /// The member's user id (the EXP-698 hue key) + profile image URL
    /// (`None` = the hue-hashed initials fallback).
    User {
        user_id: String,
        image_url: Option<String>,
    },
    /// EXP-551: the emoji glyph itself (it IS what the row inserts).
    Emoji { glyph: SharedString },
}

/// The shared row body every completion popover renders (EXP-426):
/// decoration · label · muted truncating detail. The hosts keep their own
/// container/hover/selection chrome and anchoring.
pub fn completion_row_content(item: &CompletionItem, cx: &mut App) -> gpui::AnyElement {
    use gpui::{div, IntoElement as _, ParentElement as _, Styled as _};
    use gpui_component::{ActiveTheme as _, Sizable as _};
    let mut row = gpui_component::h_flex().flex_1().min_w_0().gap_2().items_center();
    match &item.decoration {
        Some(CompletionDecoration::Status(status)) => {
            row = row.child(crate::icons::resolved_status_icon(status, cx).xsmall());
        }
        Some(CompletionDecoration::User { user_id, image_url }) => {
            row = row.child(crate::user_avatar::user_avatar(
                user_id,
                &item.label,
                image_url.as_deref(),
                gpui_component::Size::XSmall,
                cx,
            ));
        }
        Some(CompletionDecoration::Emoji { glyph }) => {
            // EXP-600: pin the COLOR emoji face — the default fallback chain
            // can reach a monochrome symbol font first.
            row = row.child(
                div()
                    .text_base()
                    .font_family(crate::emoji::EMOJI_FONT_FAMILY)
                    .child(glyph.clone()),
            );
        }
        None => {}
    }
    row.child(
        div()
            .text_sm()
            .whitespace_nowrap()
            .child(item.label.clone()),
    )
    .child(
        div()
            .flex_1()
            .min_w_0()
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .whitespace_nowrap()
            .overflow_hidden()
            .text_ellipsis()
            .child(item.detail.clone()),
    )
    .into_any_element()
}

/// A `@`/`#`/`:` token being typed behind the caret.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingToken {
    pub trigger: CompletionTrigger,
    /// Byte offset of the `@`/`#`/`:` character in the input text.
    pub start: usize,
    /// The query typed so far (without the trigger char).
    pub query: String,
    /// EXP-551, [`CompletionTrigger::Emoji`] only: the CLOSING colon has been
    /// typed (`:tada:`). An exact shortcode then auto-commits instead of
    /// leaving literal shortcode text behind. Always `false` for `@`/`#`.
    pub closed: bool,
}

impl PendingToken {
    /// Byte length of the whole token (trigger char + query + the emoji
    /// token's optional closing colon) — what acceptance replaces.
    pub fn token_len(&self) -> usize {
        1 + self.query.len() + usize::from(self.closed)
    }
}

/// Live item lookup for the popover — re-queried on every keystroke against
/// the synced collections (§4.6: "Both re-query on each keystroke against
/// the live store").
pub trait CompletionSource {
    fn query(&self, trigger: CompletionTrigger, query: &str, cx: &App) -> Vec<CompletionItem>;
}

const MAX_ITEMS: usize = 8;
const MAX_QUERY_LEN: usize = 64;

/// Minimum `:shortcode` query length (EXP-551 — web `EMOJI_AT_CARET`'s
/// `{2,}`). Two characters are what keep `12:30`, `note:` and `:)` from ever
/// opening a menu.
const MIN_EMOJI_QUERY_LEN: usize = 2;

/// Find a pending completion token ending at `cursor` (a byte offset into
/// `text`). Mirrors the web trigger rules: the token must start at the
/// beginning of a line or after whitespace (so `foo#EXP-1` and mid-email `@`s
/// don't trigger — the web `#` regex demands `(?<![\w#])`), and the query may
/// only contain the token charset (`[A-Za-z0-9._%+-@]` for mentions —
/// emails — `[A-Za-z0-9-]` for issue refs, and `[a-z0-9_+-]{2,}` plus an
/// optional closing colon for emoji, web `EMOJI_AT_CARET`).
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
        Some(':') => CompletionTrigger::Emoji,
        _ => return None,
    };
    let mut query = chars.as_str();
    // The emoji token may carry the CLOSING colon (`:tada:`); everything
    // after it is no longer part of the query.
    let mut closed = false;
    if trigger == CompletionTrigger::Emoji {
        if let Some(stripped) = query.strip_suffix(':') {
            closed = true;
            query = stripped;
        }
    }
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
        CompletionTrigger::Emoji => {
            query.len() >= MIN_EMOJI_QUERY_LEN
                && query
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_+-".contains(c))
        }
    };
    if !valid {
        return None;
    }
    Some(PendingToken {
        trigger,
        start: token_start,
        query: query.to_string(),
        closed,
    })
}

/// The default [`CompletionSource`]: team members (⨝ users) for `@`,
/// the team's issues for `#`, both read live from the §05 collections, plus
/// the team-independent emoji catalog for `:` (EXP-551).
pub fn store_completion_source(team_id: impl Into<String>) -> Rc<dyn CompletionSource> {
    Rc::new(StoreCompletionSource {
        team_id: Some(team_id.into()),
    })
}

/// EXP-551: the `:`-only source for hosts with no team in hand — emoji need
/// no synced data, so a team-less composer still gets the typeahead.
pub fn emoji_completion_source() -> Rc<dyn CompletionSource> {
    Rc::new(StoreCompletionSource { team_id: None })
}

struct StoreCompletionSource {
    /// `None` = emoji only (no team scope for members/issues).
    team_id: Option<String>,
}

impl CompletionSource for StoreCompletionSource {
    fn query(&self, trigger: CompletionTrigger, query: &str, cx: &App) -> Vec<CompletionItem> {
        let Some(team_id) = self.team_id.as_deref() else {
            return match trigger {
                CompletionTrigger::Emoji => emoji_items(query, cx),
                _ => Vec::new(),
            };
        };
        let collections = Store::global(cx).collections();
        match trigger {
            CompletionTrigger::Emoji => emoji_items(query, cx),
            CompletionTrigger::Mention => {
                let needle = query.to_lowercase();
                let members = collections.team_members.read(cx);
                let users = collections.users.read(cx);
                let mut items: Vec<CompletionItem> = members
                    .iter()
                    .filter(|m| m.team_id == team_id)
                    .filter_map(|m| users.get(&m.user_id))
                    .filter_map(|user| {
                        let email = user.email.clone()?;
                        let name = user.name.clone().unwrap_or_else(|| email.clone());
                        mention_matches(&name, &email, &needle).then(|| CompletionItem {
                            trigger,
                            insert: format!("@{email}"),
                            label: name.into(),
                            detail: email.into(),
                            decoration: Some(CompletionDecoration::User {
                                user_id: user.id.clone(),
                                image_url: user.image.clone(),
                            }),
                        })
                    })
                    .collect();
                items.sort_by(|a, b| a.label.cmp(&b.label));
                items.dedup_by(|a, b| a.insert == b.insert);
                items.truncate(MAX_ITEMS);
                items
            }
            CompletionTrigger::IssueRef => {
                let mut issues = collections.issues_in_team(team_id, cx);
                filter_and_rank_issue_refs(&mut issues, query);
                issues.truncate(MAX_ITEMS);
                issues
                    .into_iter()
                    .map(|issue| {
                        let resolved = crate::queries::resolve_issue_status(cx, &issue);
                        CompletionItem {
                            trigger,
                            insert: format!("#{}", issue.identifier),
                            label: issue.identifier.clone().into(),
                            detail: issue.title.clone().into(),
                            decoration: Some(CompletionDecoration::Status(resolved)),
                        }
                    })
                    .collect()
            }
        }
    }
}

/// EXP-551: the `:shortcode` candidates — the shared catalog's ranking. The
/// row reads glyph · `:shortcode:` · label, so the shortcode a user
/// half-remembers is always visible.
fn emoji_items(query: &str, _cx: &App) -> Vec<CompletionItem> {
    let catalog = crate::emoji::catalog();
    catalog
        .search(query, MAX_ITEMS)
        .into_iter()
        .filter_map(|index| catalog.get(index))
        .map(|emoji| {
            let glyph = emoji.unicode.clone();
            let shortcode = emoji
                .shortcodes
                .first()
                .cloned()
                .unwrap_or_else(|| emoji.label.clone());
            CompletionItem {
                trigger: CompletionTrigger::Emoji,
                insert: glyph.clone(),
                label: format!(":{shortcode}:").into(),
                detail: emoji.label.clone().into(),
                decoration: Some(CompletionDecoration::Emoji {
                    glyph: glyph.into(),
                }),
            }
        })
        .collect()
}

/// EXP-551: the unicode for an EXACT shortcode, or `None`. Both hosts call
/// this on a closed `:tada:` token to auto-commit; nothing else expands
/// shortcodes (stored markdown keeps literal `:text:` literal).
pub fn exact_emoji(shortcode: &str, _cx: &App) -> Option<String> {
    let emoji = crate::emoji::catalog().find_shortcode(shortcode)?;
    Some(emoji.unicode.clone())
}

/// Match a `@` candidate the way the web `MentionProvider.search` does (iOS
/// and Android mirror it): case-insensitive SUBSTRING match on the full name
/// or the full email, so `@huber` finds `Dennis Straehhuber
/// <dennis@straehhuber.com>` on every client. `needle` must already be
/// lowercased.
fn mention_matches(name: &str, email: &str, needle: &str) -> bool {
    needle.is_empty()
        || name.to_lowercase().contains(needle)
        || email.to_lowercase().contains(needle)
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

    // -- `:shortcode` emoji trigger (EXP-551, web EMOJI_AT_CARET parity) ---

    #[test]
    fn detects_emoji_shortcode() {
        let token = detect_trigger("ship :sm", 8).expect("trigger");
        assert_eq!(token.trigger, CompletionTrigger::Emoji);
        assert_eq!(token.start, 5);
        assert_eq!(token.query, "sm");
        assert!(!token.closed);
        assert_eq!(token.token_len(), 3);
    }

    #[test]
    fn detects_a_closed_emoji_shortcode() {
        let token = detect_trigger(":smile:", 7).expect("trigger");
        assert_eq!(token.query, "smile");
        assert!(token.closed);
        // The closing colon is part of what acceptance replaces.
        assert_eq!(token.token_len(), 7);
    }

    #[test]
    fn emoji_queries_are_case_insensitive_and_take_the_shortcode_charset() {
        assert_eq!(detect_trigger(":SM", 3).expect("trigger").query, "SM");
        assert_eq!(detect_trigger(":+1", 3).expect("trigger").query, "+1");
        assert_eq!(
            detect_trigger(":thumbs_up", 10).expect("trigger").query,
            "thumbs_up"
        );
        assert_eq!(
            detect_trigger(":e-mail", 7).expect("trigger").query,
            "e-mail"
        );
    }

    #[test]
    fn clock_times_urls_and_smileys_never_trigger() {
        // A colon glued to a word is not a trigger (the web regex demands
        // start-of-text or whitespace before it).
        assert_eq!(detect_trigger("12:30", 5), None);
        assert_eq!(detect_trigger("note:", 5), None);
        assert_eq!(detect_trigger("http://x", 8), None);
        // `:)` — one non-shortcode char, below the 2-char floor.
        assert_eq!(detect_trigger(":)", 2), None);
        // A single character is below the floor even when it IS in the charset.
        assert_eq!(detect_trigger(":a", 2), None);
        // Double colons are not a token either.
        assert_eq!(detect_trigger("::sm", 4), None);
        assert_eq!(detect_trigger(":smile::", 8), None);
    }

    #[test]
    fn emoji_cursor_mid_multibyte_char_is_safe() {
        // "é" is 2 bytes; offset 2 is inside it.
        assert_eq!(detect_trigger(":é", 2), None);
        // A token behind an emoji already in the text still resolves.
        let token = detect_trigger("🎉 :ta", 8).expect("trigger");
        assert_eq!(token.query, "ta");
    }

    #[test]
    fn mention_and_issue_ref_tokens_are_never_closed() {
        assert!(!detect_trigger("@ja", 3).expect("trigger").closed);
        assert!(!detect_trigger("#EXP-1", 6).expect("trigger").closed);
        assert_eq!(detect_trigger("@ja", 3).expect("trigger").token_len(), 3);
    }

    // -- mention_matches (web MentionProvider.search parity) ----------------

    const NAME: &str = "Dennis Straehhuber";
    const EMAIL: &str = "dennis@straehhuber.com";

    #[test]
    fn mentions_match_name_substring_not_just_word_prefix() {
        assert!(mention_matches(NAME, EMAIL, "huber"));
        assert!(mention_matches(NAME, EMAIL, "nnis stra"));
    }

    #[test]
    fn mentions_match_email_substring() {
        assert!(mention_matches(NAME, EMAIL, "ehhub"));
        assert!(mention_matches(NAME, EMAIL, "@straehhuber.com"));
    }

    #[test]
    fn mentions_match_case_insensitively() {
        assert!(mention_matches("Jane Doe", "JANE@EXAMPLE.COM", "example"));
    }

    #[test]
    fn mentions_without_match_are_dropped() {
        assert!(!mention_matches(NAME, EMAIL, "zzz"));
    }

    #[test]
    fn mentions_empty_query_matches_everyone() {
        assert!(mention_matches(NAME, EMAIL, ""));
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
