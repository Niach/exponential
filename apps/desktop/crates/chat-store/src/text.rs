//! The query semantics (chat-store.json QUERY SEMANTICS = steer-enet.json
//! SEARCH, #EDE-96): a case- and diacritics-insensitive literal over
//! whitespace-collapsed text, one snippet per match.

use unicode_normalization::char::{decompose_canonical, is_combining_mark};

/// Query code points kept (after the whitespace collapse).
pub const QUERY_MAX_CHARS: usize = 200;
/// The most hits (find in chat) or chats (global) one answer carries.
pub const HITS_MAX: usize = 50;
/// A snippet's width in code points (more only for a longer match).
pub const SNIPPET_CHARS: usize = 120;
/// Code points of context a snippet keeps before the match.
pub const SNIPPET_LEAD_CHARS: usize = 40;

fn is_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{0B}' | '\u{0C}')
}

/// Every run of ASCII whitespace = one space, trimmed.
pub fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for ch in text.chars() {
        if is_space(ch) {
            space = !out.is_empty();
            continue;
        }
        if space {
            out.push(' ');
        }
        space = false;
        out.push(ch);
    }
    out
}

/// SYSTEM INPUTS (#FEED-300, chat-store.json `search.systemCases`, TS
/// `systemInput`): is this input or message text a line the device or the
/// server worded (a tagged system line, a firing, another speaker's line),
/// not a person's own words?
pub fn system_input(text: &str) -> bool {
    system_tag(text) || system_firing(text) || system_speaker(text)
}

/// `[` + one or more of a-z and `_` + `] `.
fn system_tag(text: &str) -> bool {
    let Some(rest) = text.strip_prefix('[') else { return false };
    let tag = rest.chars().take_while(|ch| ch.is_ascii_lowercase() || *ch == '_').count();
    tag > 0 && rest[tag..].starts_with("] ")
}

/// The text after a backticked name (one or more characters, no backtick or
/// newline) that follows `prefix`, starting at its closing backtick.
fn after_name<'t>(text: &'t str, prefix: &str) -> Option<&'t str> {
    let rest = text.strip_prefix(prefix)?;
    let end = rest.find(['`', '\n'])?;
    (end > 0 && rest[end..].starts_with('`')).then(|| &rest[end..])
}

/// `Schedule \`…\`` + ` fired` on the same line, or `Event \`…\` (from `.
fn system_firing(text: &str) -> bool {
    if let Some(rest) = after_name(text, "Schedule `") {
        if rest[1..].split('\n').next().is_some_and(|line| line.contains(" fired")) {
            return true;
        }
    }
    after_name(text, "Event `").is_some_and(|rest| rest.starts_with("` (from "))
}

/// `Report from ` / `Message from ` + a name + ` (session <id>)`,
/// ` (session <id>, ended)` or ` (member <id>)` + `: `, on the first line.
fn system_speaker(text: &str) -> bool {
    let Some(rest) = text.strip_prefix("Report from ").or_else(|| text.strip_prefix("Message from ")) else { return false };
    let line = rest.split('\n').next().unwrap_or_default();
    [" (session ", " (member "].into_iter().any(|open| {
        line.match_indices(open).any(|(at, _)| {
            if at == 0 {
                return false;
            }
            let after = &line[at + open.len()..];
            let id = after.find(|ch: char| ch == ',' || ch == ')' || is_space(ch)).unwrap_or(after.len());
            let tail = &after[id..];
            id > 0 && tail.strip_prefix(", ended").unwrap_or(tail).starts_with("): ")
        })
    })
}

/// The fold, per code point: NFD, marks dropped, lowercased. Returns the
/// folded chars and, per folded char, the code point of `text` it came from.
pub fn fold(text: &str) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::with_capacity(text.len());
    let mut source = Vec::with_capacity(text.len());
    for (index, ch) in text.chars().enumerate() {
        decompose_canonical(ch, |part| {
            if is_combining_mark(part) {
                return;
            }
            for lower in part.to_lowercase() {
                chars.push(lower);
                source.push(index);
            }
        });
    }
    (chars, source)
}

/// The folded text as a string (what the index stores and matches).
pub fn folded(text: &str) -> String {
    fold(&collapse(text)).0.into_iter().collect()
}

/// The folded query: collapsed, cut to [`QUERY_MAX_CHARS`]; empty = no search.
pub fn fold_query(query: &str) -> Vec<char> {
    let cut: String = collapse(query).chars().take(QUERY_MAX_CHARS).collect();
    fold(&cut).0
}

/// One match's body: the snippet and the match in its code points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub snippet: String,
    pub match_start: usize,
    pub match_len: usize,
}

/// The first match of the folded `query` in `text` (collapsed here).
pub fn first_match(text: &str, query: &[char]) -> Option<Match> {
    let collapsed = collapse(text);
    let (folded, source) = fold(&collapsed);
    if query.is_empty() || folded.len() < query.len() {
        return None;
    }
    let at = (0..=folded.len() - query.len()).find(|&i| folded[i..i + query.len()] == *query)?;
    let start = source[at];
    let end = source[at + query.len() - 1] + 1;
    let chars: Vec<char> = collapsed.chars().collect();
    let from = start.saturating_sub(SNIPPET_LEAD_CHARS);
    let to = chars.len().min((from + SNIPPET_CHARS).max(end));
    let mut snippet = String::new();
    let mut lead = 0;
    if from > 0 {
        snippet.push('…');
        lead = 1;
    }
    snippet.extend(&chars[from..to]);
    if to < chars.len() {
        snippet.push('…');
    }
    Some(Match { snippet, match_start: start - from + lead, match_len: end - start })
}

/// A limit clamped to [1, [`HITS_MAX`]].
pub fn clamp_limit(limit: usize) -> usize {
    limit.clamp(1, HITS_MAX)
}
