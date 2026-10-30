//! Text the way the web lays it out, shared by the measurer and the painter
//! so the reserved box IS the painted box:
//!
//! - **Break opportunities** follow UAX #14 (`unicode-linebreak`): spaces,
//!   after hyphens, between CJK ideographs, never inside a URL at `/` — what
//!   a browser's `overflow-wrap: normal` does.
//! - **Min-content** = the widest unbreakable segment, **max-content** = the
//!   widest hard line; trailing spaces HANG (they never count), as in CSS.
//! - **Wrapping** is greedy over those segments; the painter draws the
//!   resulting lines verbatim (hard breaks, no re-wrapping by gpui).
//! - `text-transform` applies before both.
//!
//! Every function here is pure: widths come from a caller closure (gpui's
//! shaper in the painter, a fixed width per char in the tests).

use std::borrow::Cow;

use unicode_linebreak::{linebreaks, BreakOpportunity};

/// Round 2 §2: a text's bidi PARAGRAPH direction is its node's (`Hello!` in
/// an rtl box reads `!Hello`). gpui takes the direction of the first strong
/// character, so a zero-width mark (RLM / LRM) leads the text when that
/// differs from the node's.
pub fn with_paragraph_direction(text: &str, rtl: bool) -> Cow<'_, str> {
    use unicode_bidi::{bidi_class, BidiClass};
    let first = text.chars().find_map(|c| match bidi_class(c) {
        BidiClass::L => Some(false),
        BidiClass::R | BidiClass::AL => Some(true),
        _ => None,
    });
    match (rtl, first) {
        (true, Some(true)) | (false, None | Some(false)) => Cow::Borrowed(text),
        (true, _) => Cow::Owned(format!("\u{200F}{text}")),
        (false, Some(true)) => Cow::Owned(format!("\u{200E}{text}")),
    }
}

/// CSS `text-transform` (`uppercase | lowercase | capitalize`; anything else
/// leaves the text alone).
pub fn transform<'a>(text: &'a str, mode: Option<&str>) -> Cow<'a, str> {
    match mode {
        Some("uppercase") => Cow::Owned(text.to_uppercase()),
        Some("lowercase") => Cow::Owned(text.to_lowercase()),
        Some("capitalize") => {
            let mut out = String::with_capacity(text.len());
            let mut start = true;
            for c in text.chars() {
                if start && c.is_alphanumeric() {
                    out.extend(c.to_uppercase());
                    start = false;
                } else {
                    if c.is_whitespace() {
                        start = true;
                    }
                    out.push(c);
                }
            }
            Cow::Owned(out)
        }
        _ => Cow::Borrowed(text),
    }
}

/// `s` without the spaces that hang at a line end.
pub fn hang(s: &str) -> &str {
    s.trim_end_matches([' ', '\t'])
}

/// One unbreakable run of text: its slice (trailing spaces included, the
/// line-feed excluded) and whether a HARD break follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment<'a> {
    pub text: &'a str,
    pub hard: bool,
}

/// The text cut at every break opportunity.
pub fn segments(text: &str) -> Vec<Segment<'_>> {
    let mut out = Vec::new();
    let mut start = 0;
    for (at, op) in linebreaks(text) {
        let raw = &text[start..at];
        let end_of_text = at >= text.len();
        let body = raw.trim_end_matches(['\n', '\r', '\u{2028}', '\u{2029}', '\u{0b}', '\u{0c}']);
        let hard = op == BreakOpportunity::Mandatory && (!end_of_text || body.len() != raw.len());
        out.push(Segment { text: body, hard });
        start = at;
    }
    if out.is_empty() {
        out.push(Segment { text: "", hard: false });
    }
    // Tailoring: like the browsers' URL handling, no break after a slash
    // that a word follows (`exponential.at/docs` stays whole).
    let mut merged: Vec<Segment<'_>> = Vec::with_capacity(out.len());
    for seg in out {
        match merged.last_mut() {
            Some(prev) if !prev.hard && prev.text.ends_with('/') && seg.text.chars().next().is_some_and(|c| !c.is_whitespace()) => {
                let start = prev.text.as_ptr() as usize - text.as_ptr() as usize;
                let end = seg.text.as_ptr() as usize - text.as_ptr() as usize + seg.text.len();
                *prev = Segment { text: &text[start..end], hard: seg.hard };
            }
            _ => merged.push(seg),
        }
    }
    merged
}

/// The widest unbreakable segment (CSS min-content).
pub fn min_content(text: &str, width_of: &mut dyn FnMut(&str) -> f32) -> f32 {
    segments(text).iter().map(|s| width_of(hang(s.text))).fold(0.0, f32::max)
}

/// The widest hard line (CSS max-content).
pub fn max_content(text: &str, width_of: &mut dyn FnMut(&str) -> f32) -> f32 {
    hard_lines(text).iter().map(|l| width_of(hang(l))).fold(0.0, f32::max)
}

/// The text split at its hard breaks only.
pub fn hard_lines(text: &str) -> Vec<String> {
    let mut lines = vec![String::new()];
    for s in segments(text) {
        lines.last_mut().expect("one line").push_str(s.text);
        if s.hard {
            lines.push(String::new());
        }
    }
    if lines.len() > 1 && lines.last().is_some_and(String::is_empty) && !text.ends_with("\n\n") {
        // A trailing line feed ends the last line; it opens none (pre-line).
        lines.pop();
    }
    lines
}

/// Greedy line breaking at `width` (`None` = only the hard breaks). Each
/// line is returned with its hanging spaces removed. A segment wider than
/// `width` overflows on its own line (`overflow-wrap: normal`).
pub fn wrap(text: &str, width: Option<f32>, width_of: &mut dyn FnMut(&str) -> f32) -> Vec<String> {
    let Some(width) = width else {
        return hard_lines(text).iter().map(|l| hang(l).to_string()).collect();
    };
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    // Width of `line` including its trailing spaces (the next segment
    // starts after them).
    let mut line_full = 0.0f32;
    let mut open = false;
    for s in segments(text) {
        let full = width_of(s.text);
        let trimmed = if hang(s.text).len() == s.text.len() { full } else { width_of(hang(s.text)) };
        if open && !line.is_empty() && line_full + trimmed > width + 0.5 {
            lines.push(hang(&line).to_string());
            line.clear();
            line_full = 0.0;
        }
        line.push_str(s.text);
        line_full += full;
        open = true;
        if s.hard {
            lines.push(hang(&line).to_string());
            line.clear();
            line_full = 0.0;
            open = false;
        }
    }
    if open || lines.is_empty() {
        lines.push(hang(&line).to_string());
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_paragraph_direction_is_the_nodes() {
        assert_eq!(with_paragraph_direction("Hello!", true), "\u{200F}Hello!");
        assert_eq!(with_paragraph_direction("Hello!", false), "Hello!");
        assert_eq!(with_paragraph_direction("שלום!", true), "שלום!");
        assert_eq!(with_paragraph_direction("שלום!", false), "\u{200E}שלום!");
        assert_eq!(with_paragraph_direction("42", true), "\u{200F}42");
        assert_eq!(with_paragraph_direction("42", false), "42");
    }

    /// 8 px per char, like the core's fixed measure.
    fn w8(s: &str) -> f32 {
        s.chars().count() as f32 * 8.0
    }

    #[test]
    fn break_opportunities_follow_uax14() {
        let words: Vec<&str> = segments("Hello brave new-world").iter().map(|s| s.text).collect();
        assert_eq!(words, ["Hello ", "brave ", "new-", "world"], "a break after the hyphen");
        let url: Vec<&str> = segments("see https://exponential.at/docs/start now").iter().map(|s| s.text).collect();
        assert!(url.contains(&"https://exponential.at/docs/start "), "a URL stays whole: {url:?}");
        let cjk = segments("日本語のテキスト");
        assert!(cjk.len() >= 6, "CJK breaks between ideographs: {cjk:?}");
        let hard: Vec<(&str, bool)> = segments("one\ntwo").iter().map(|s| (s.text, s.hard)).collect();
        assert_eq!(hard, [("one", true), ("two", false)]);
    }

    #[test]
    fn min_and_max_content_hang_trailing_spaces() {
        assert_eq!(min_content("Hello brave world", &mut w8), 40.0);
        assert_eq!(max_content("Hello brave world  ", &mut w8), 17.0 * 8.0);
        assert_eq!(max_content("ab\nabcd", &mut w8), 32.0);
        assert_eq!(min_content("日本語", &mut w8), 8.0, "one ideograph");
        assert_eq!(min_content("", &mut w8), 0.0);
    }

    #[test]
    fn wrapping_is_greedy_and_overflows_long_words() {
        assert_eq!(wrap("Hello brave new world", Some(96.0), &mut w8), ["Hello brave", "new world"]);
        assert_eq!(wrap("Hello brave new world", Some(40.0), &mut w8), ["Hello", "brave", "new", "world"]);
        assert_eq!(wrap("Supercalifragilistic ok", Some(40.0), &mut w8), ["Supercalifragilistic", "ok"]);
        assert_eq!(wrap("one\ntwo three", Some(400.0), &mut w8), ["one", "two three"]);
        assert_eq!(wrap("one\n\ntwo", None, &mut w8), ["one", "", "two"]);
        assert_eq!(wrap("", Some(40.0), &mut w8), [""]);
        assert_eq!(wrap("trailing\n", None, &mut w8), ["trailing"]);
    }

    #[test]
    fn text_transform_matches_css() {
        assert_eq!(transform("hello world", Some("uppercase")), "HELLO WORLD");
        assert_eq!(transform("Hello World", Some("lowercase")), "hello world");
        assert_eq!(transform("hello wide world", Some("capitalize")), "Hello Wide World");
        assert_eq!(transform("as is", Some("none")), "as is");
        assert_eq!(transform("as is", None), "as is");
    }
}
