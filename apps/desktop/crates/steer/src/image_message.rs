//! The steer composer's image-message template (EXP-511) — the desktop
//! viewer's mirror of `apps/web/src/lib/steer-image-message.ts`.
//!
//! A steered message carries attached images as markdown embeds. The HOST
//! device localizes each embed to a file path before the agent sees it
//! (`publisher::localize_image_embeds`) and restores the token on the way back
//! out (`publisher::restore_image_embeds`, which is what makes the echo dedupe
//! and the viewer's image rendering work), so this exact shape is load-bearing
//! across web, iOS (`SteerImageMessage.swift`), Android
//! (`SteerImageMessage.kt`) and now the desktop. The four builders are
//! BYTE-IDENTICAL — the test fixtures below are the web test's, verbatim.
//!
//! The embed the publisher recognizes is `![image](/api/attachments/<uuid>)`
//! (`publisher::image_embed_pattern`); nothing else in a message can be
//! mistaken for one.
//!
//! EXP-698 added the POSITIONAL half: `[Image #N]` markers. The composer drops
//! one at the caret when the N-th image is attached, so the agent reads
//! "crop [Image #2]" instead of guessing which embed a sentence means. The
//! marker is PLAIN TEXT on the wire — the embed block below the prose stays
//! the only image payload — and the viewer renders each marker as a chip.
//! [`build_steer_image_message`]'s wire shape is FROZEN; the markers ride
//! inside its text half.

use std::sync::OnceLock;

use regex::Regex;

/// How many images one steered message may carry (web/iOS/Android parity).
pub const MAX_STEER_IMAGES: usize = 4;

/// `\[Image #(\d+)\]` — the web `IMAGE_MARKER_PATTERN`, byte-identical.
fn image_marker_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"\[Image #(\d+)\]").expect("the image marker pattern is a valid regex")
    })
}

/// One embed line, exactly as [`build_steer_image_message`] writes it.
fn embed_line_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^!\[image\]\(/api/attachments/([^)\s]+)\)$")
            .expect("the embed line pattern is a valid regex")
    })
}

/// The 1-based positional reference to one of the message's images.
pub fn image_marker(index: u32) -> String {
    format!("[Image #{index}]")
}

/// Build the message a steerer's composer sends: the trimmed text, then a
/// BLANK line, then one embed per line. With no text the embeds go alone;
/// with no images the trimmed text goes alone.
pub fn build_steer_image_message(text: &str, attachment_ids: &[String]) -> String {
    let trimmed = text.trim();
    if attachment_ids.is_empty() {
        return trimmed.to_string();
    }
    let embeds = attachment_ids
        .iter()
        .map(|id| format!("![image](/api/attachments/{id})"))
        .collect::<Vec<_>>()
        .join("\n");
    if trimmed.is_empty() {
        return embeds;
    }
    format!("{trimmed}\n\n{embeds}")
}

/// The inverse of [`build_steer_image_message`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedSteerMessage {
    /// The message without its trailing embed block.
    pub text: String,
    /// Attachment ids, in embed order — image #1 is `attachment_ids[0]`.
    pub attachment_ids: Vec<String>,
    /// The `[Image #N]` numbers the text carries, 1-based, in text order,
    /// deduped. A number with no matching embed is still reported — the
    /// viewer decides what to do with a dangling reference.
    pub markers: Vec<u32>,
}

/// Split a composed steer message back into its prose and its embeds, and
/// report the positional markers the prose carries.
pub fn parse_steer_message(message: &str) -> ParsedSteerMessage {
    let lines: Vec<&str> = message.split('\n').collect();
    let mut end = lines.len();
    while end > 0 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    let mut attachment_ids: Vec<String> = Vec::new();
    while end > 0 {
        let Some(captures) = embed_line_pattern().captures(lines[end - 1].trim()) else {
            break;
        };
        attachment_ids.insert(0, captures[1].to_string());
        end -= 1;
    }
    let text = lines[..end].join("\n").trim_end().to_string();
    let mut markers: Vec<u32> = Vec::new();
    for captures in image_marker_pattern().captures_iter(&text) {
        let Ok(index) = captures[1].parse::<u32>() else {
            continue;
        };
        if !markers.contains(&index) {
            markers.push(index);
        }
    }
    ParsedSteerMessage {
        text,
        attachment_ids,
        markers,
    }
}

/// Drop `[Image #index]` at `caret`, space-separated from whatever it lands
/// against. Returns the new draft and the caret BEHIND the insertion.
///
/// `caret` is a BYTE offset (gpui's textarea works in bytes); an offset that
/// is out of range — or lands inside a multi-byte character — is clamped to
/// the nearest character boundary at or before it, so the split can never
/// panic on a UTF-8 boundary.
pub fn insert_image_marker(text: &str, caret: usize, index: u32) -> (String, usize) {
    let at = clamp_to_char_boundary(text, caret);
    let (before, after) = text.split_at(at);
    let marker = image_marker(index);
    let lead = if !before.is_empty() && !before.ends_with(char::is_whitespace) {
        " "
    } else {
        ""
    };
    let trail = if !after.is_empty() && !after.starts_with(char::is_whitespace) {
        " "
    } else {
        ""
    };
    (
        format!("{before}{lead}{marker}{trail}{after}"),
        at + lead.len() + marker.len() + trail.len(),
    )
}

/// Removing the `removed_index`-th pending image renumbers the draft: its own
/// markers go, and every higher one slides down one. Only a line that LOST a
/// marker gets the gap it left tidied — untouched lines keep their spacing.
pub fn renumber_image_markers(text: &str, removed_index: u32) -> String {
    text.split('\n')
        .map(|line| {
            let mut dropped = false;
            let next = image_marker_pattern().replace_all(line, |captures: &regex::Captures| {
                let Ok(index) = captures[1].parse::<u32>() else {
                    return captures[0].to_string();
                };
                if index == removed_index {
                    dropped = true;
                    return String::new();
                }
                if index > removed_index {
                    image_marker(index - 1)
                } else {
                    captures[0].to_string()
                }
            });
            if !dropped {
                return next.into_owned();
            }
            let tidied = collapse_runs(next.as_ref());
            if line.starts_with(&image_marker(removed_index)) {
                tidied.trim_start_matches([' ', '\t']).to_string()
            } else {
                tidied
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The web tidy pass: runs of 2+ spaces/tabs collapse to one, trailing
/// spaces/tabs go.
fn collapse_runs(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut run = 0usize;
    for ch in line.chars() {
        if ch == ' ' || ch == '\t' {
            run += 1;
            continue;
        }
        if run > 0 {
            out.push(' ');
            run = 0;
        }
        out.push(ch);
    }
    // A trailing run is dropped entirely — the web `replace(/[ \t]+$/, '')`.
    // The line reached this pass only because it LOST a marker, so a
    // trailing gap is always one this removal opened.
    let _ = run;
    out
}

/// The largest char boundary at or before `at` (and never past the end).
fn clamp_to_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures mirrored byte-for-byte from steer-image-message.test.ts (web),
    // SteerImageMessageTests.swift (iOS) and SteerImageMessageTest.kt
    // (Android) — change all four together.
    const A: &str = "11111111-1111-4111-8111-111111111111";
    const B: &str = "22222222-2222-4222-8222-222222222222";

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn appends_embeds_after_a_blank_line_one_per_line() {
        assert_eq!(
            build_steer_image_message("fix the header", &ids(&[A, B])),
            format!(
                "fix the header\n\n![image](/api/attachments/{A})\n![image](/api/attachments/{B})"
            )
        );
    }

    #[test]
    fn sends_embeds_alone_when_the_text_is_whitespace() {
        assert_eq!(
            build_steer_image_message("  \n ", &ids(&[A])),
            format!("![image](/api/attachments/{A})")
        );
    }

    #[test]
    fn returns_trimmed_text_unchanged_without_images() {
        assert_eq!(build_steer_image_message("  hello  ", &[]), "hello");
    }

    #[test]
    fn returns_the_empty_string_for_no_text_and_no_images() {
        assert_eq!(build_steer_image_message("", &[]), "");
    }

    #[test]
    fn caps_at_four_images() {
        assert_eq!(MAX_STEER_IMAGES, 4);
    }

    #[test]
    fn the_embed_matches_what_the_publisher_localizes() {
        // The publisher's reverse rewrite keys on this exact token shape —
        // a drift here would ship local file paths into the published feed.
        let message = build_steer_image_message("look", &ids(&[A]));
        assert!(message.contains(&format!("![image](/api/attachments/{A})")));
    }

    // -- EXP-698 positional markers (web fixtures, verbatim) ----------------

    #[test]
    fn spaces_the_marker_off_the_text_it_lands_against() {
        assert_eq!(
            insert_image_marker("crop", 4, 1),
            ("crop [Image #1]".to_string(), 15)
        );
    }

    #[test]
    fn inserts_mid_text_with_one_space_on_each_side() {
        let (text, caret) = insert_image_marker("crop this", 5, 2);
        assert_eq!(text, "crop [Image #2] this");
        // Behind the trailing space, ready for more typing.
        assert_eq!(caret, 16);
    }

    #[test]
    fn adds_no_space_where_one_is_already_there() {
        assert_eq!(insert_image_marker("crop ", 5, 1).0, "crop [Image #1]");
        assert_eq!(insert_image_marker(" this", 0, 1).0, "[Image #1] this");
    }

    #[test]
    fn stands_alone_in_an_empty_draft() {
        assert_eq!(
            insert_image_marker("", 0, 1),
            ("[Image #1]".to_string(), 10)
        );
    }

    #[test]
    fn clamps_an_out_of_range_caret_to_the_end() {
        assert_eq!(insert_image_marker("crop", 99, 1).0, "crop [Image #1]");
    }

    #[test]
    fn clamps_a_caret_inside_a_multi_byte_character() {
        // Rust-only: the web's caret is a UTF-16 index into a JS string and
        // cannot split a scalar; ours is a byte offset and must not panic.
        let (text, _) = insert_image_marker("é", 1, 1);
        assert_eq!(text, "[Image #1] é");
    }

    #[test]
    fn drops_the_removed_marker_and_slides_the_higher_ones_down() {
        assert_eq!(
            renumber_image_markers("crop [Image #1] and [Image #2] and [Image #3]", 2),
            "crop [Image #1] and and [Image #2]"
        );
    }

    #[test]
    fn tidies_the_gap_the_dropped_marker_left() {
        assert_eq!(renumber_image_markers("crop [Image #1] please", 1), "crop please");
        assert_eq!(renumber_image_markers("crop [Image #1]", 1), "crop");
        assert_eq!(renumber_image_markers("[Image #1] crop", 1), "crop");
    }

    #[test]
    fn leaves_lower_markers_and_untouched_lines_alone() {
        assert_eq!(
            renumber_image_markers("[Image #1]  keep\ncrop [Image #3]", 2),
            "[Image #1]  keep\ncrop [Image #2]"
        );
    }

    #[test]
    fn removes_every_occurrence_of_the_same_marker() {
        assert_eq!(renumber_image_markers("a [Image #2] b [Image #2] c", 2), "a b c");
    }

    #[test]
    fn splits_the_prose_from_the_trailing_embeds() {
        assert_eq!(
            parse_steer_message(&build_steer_image_message("fix [Image #1]", &ids(&[A, B]))),
            ParsedSteerMessage {
                text: "fix [Image #1]".to_string(),
                attachment_ids: ids(&[A, B]),
                markers: vec![1],
            }
        );
    }

    #[test]
    fn reads_embeds_sent_without_text() {
        assert_eq!(
            parse_steer_message(&build_steer_image_message("", &ids(&[A]))),
            ParsedSteerMessage {
                text: String::new(),
                attachment_ids: ids(&[A]),
                markers: Vec::new(),
            }
        );
    }

    #[test]
    fn leaves_a_plain_message_untouched() {
        assert_eq!(
            parse_steer_message("just words"),
            ParsedSteerMessage {
                text: "just words".to_string(),
                attachment_ids: Vec::new(),
                markers: Vec::new(),
            }
        );
    }

    #[test]
    fn reports_markers_in_text_order_deduped() {
        assert_eq!(
            parse_steer_message("[Image #2] then [Image #1] then [Image #2]").markers,
            vec![2, 1]
        );
    }

    #[test]
    fn builds_the_marker_the_pattern_matches() {
        assert_eq!(image_marker(3), "[Image #3]");
        assert_eq!(parse_steer_message(&image_marker(3)).markers, vec![3]);
    }
}
