//! The composer's caret typeahead rules (masterplan-v3 §4.6, EXP-551):
//! which `@`-member / `#`-issue / `:`-emoji token is being typed behind the
//! caret, gpui-free so every host opens the same menu (the IDE's markdown
//! editor and mention input, #EDE-65 Exponential Desktop's chat). Accepting
//! inserts the interchange form: `@<email>`, `#<IDENTIFIER>`, the emoji's
//! unicode.

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

pub const MAX_ITEMS: usize = 8;
pub const MAX_QUERY_LEN: usize = 64;

/// Minimum `:shortcode` query length (EXP-551 — web `EMOJI_AT_CARET`'s
/// `{2,}`). Two characters are what keep `12:30`, `note:` and `:)` from ever
/// opening a menu.
pub const MIN_EMOJI_QUERY_LEN: usize = 2;

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

/// A member matches a `@` query by name or email (case-insensitive, contains).
pub fn mention_matches(name: &str, email: &str, needle: &str) -> bool {
    needle.is_empty()
        || name.to_lowercase().contains(needle)
        || email.to_lowercase().contains(needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_behind_the_caret_opens_its_menu() {
        let at = detect_trigger("hi @da", 6).expect("@");
        assert_eq!((at.trigger, at.start, at.query.as_str()), (CompletionTrigger::Mention, 3, "da"));
        assert_eq!(detect_trigger("see #EXP-1", 10).map(|t| t.trigger), Some(CompletionTrigger::IssueRef));
        assert!(detect_trigger("mail@x", 6).is_none(), "mid-word never opens");
        assert!(detect_trigger("12:30", 5).is_none(), "an emoji query needs two characters");
        assert!(mention_matches("Dana Reyes", "dana@x.com", "rey"));
    }
}
