//! #EXP-1841: THE attachment strip of EVERY composer ×4 (a new chat's, a
//! session's, an agent chat's, the Assistant's, an app editor's) and its
//! positional `[Image #N]` placeholders (EXP-698). Locked by
//! `packages/domain-contract/fixtures/composer-attachments.json` (its
//! `$comment` holds the rules); mirrors TS `src/composer-attachments.ts`, iOS
//! `ComposerAttachments.swift`, Android `ComposerAttachments.kt`. Pure: the
//! strip is a list of kinds; classifying a picked file (type, size) stays
//! the composer's (`ui::composer_images`), so a pick arrives classified.
//!
//! Offsets are BYTE offsets (gpui's text fields work in bytes); a caret out
//! of range or inside a multi-byte character clamps to the character
//! boundary at or before it.

use std::sync::OnceLock;

use regex::Regex;

use crate::image_message::image_marker;

/// What sits in the strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentKind {
    Image,
    File,
}

/// A picked file, classified: an image, a file, or refused (empty, over its
/// size cap).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickKind {
    Image,
    File,
    Rejected,
}

/// Images and files each cap at four per message.
pub const COMPOSER_MAX_IMAGES: usize = 4;
pub const COMPOSER_MAX_FILES: usize = 4;

/// The rejection toasts ×4.
pub const ATTACHMENT_REJECTED_TOAST: &str = "Images up to 10 MB and files up to 50 MB can be attached";
pub const IMAGE_CAP_TOAST: &str = "Up to 4 images per message";
pub const FILE_CAP_TOAST: &str = "Up to 4 files per message";

fn marker_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| Regex::new(r"\[Image #(\d+)\]").expect("the image marker pattern is a valid regex"))
}

/// The largest char boundary at or before `at` (and never past the end).
pub fn clamp_to_char_boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while at > 0 && !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Drops `[Image #index]` at `caret`, space-separated from whatever it lands
/// against. Returns the new draft and the caret BEHIND the insertion.
pub fn insert_image_marker(text: &str, caret: usize, index: u32) -> (String, usize) {
    let at = clamp_to_char_boundary(text, caret);
    let (before, after) = text.split_at(at);
    let marker = image_marker(index);
    let lead = if !before.is_empty() && !before.ends_with(char::is_whitespace) { " " } else { "" };
    let trail = if !after.is_empty() && !after.starts_with(char::is_whitespace) { " " } else { "" };
    (format!("{before}{lead}{marker}{trail}{after}"), at + lead.len() + marker.len() + trail.len())
}

/// Removing the `removed`-th image renumbers the draft: its own markers go,
/// and every higher one slides down one. Only a line that LOST a marker gets
/// the gap it left tidied; untouched lines keep their spacing.
pub fn renumber_image_markers(text: &str, removed: u32) -> String {
    renumber_with_caret(text, removed, 0).0
}

/// Collapses each run of 2+ spaces/tabs to one space, mapping `at`: a caret
/// inside a run (past its first character) lands behind the single space.
fn collapse_runs(line: &str, at: usize) -> (String, usize) {
    let blank = |b: u8| b == b' ' || b == b'\t';
    let bytes = line.as_bytes();
    let mut out = String::with_capacity(line.len());
    // Every in-range `at` sits on a char start the walk passes; past the
    // end = the new end.
    let mut mapped: Option<usize> = None;
    let mut i = 0;
    while i < line.len() {
        if blank(bytes[i]) {
            let mut j = i;
            while j < line.len() && blank(bytes[j]) {
                j += 1;
            }
            if j - i >= 2 {
                if at == i {
                    mapped = Some(out.len());
                } else if at > i && at <= j {
                    mapped = Some(out.len() + 1);
                }
                out.push(' ');
                i = j;
                continue;
            }
        }
        if at == i {
            mapped = Some(out.len());
        }
        let ch = line[i..].chars().next().expect("a char at a boundary");
        out.push(ch);
        i += ch.len_utf8();
    }
    let at = mapped.unwrap_or(out.len());
    (out, at)
}

/// [`renumber_image_markers`] with the caret carried along (the X keeps it
/// where it was): it moves left by what the removal took out before it, so
/// a caret right behind the removed placeholder lands where it began. A
/// caret INSIDE a marker counts as at that marker's start. TS
/// `renumberWithCaret`, in byte offsets.
pub fn renumber_with_caret(text: &str, removed: u32, caret: usize) -> (String, usize) {
    let mut at = clamp_to_char_boundary(text, caret);
    for found in marker_pattern().find_iter(text) {
        if at > found.start() && at < found.end() {
            at = found.start();
        }
    }
    let mut out: Vec<String> = Vec::new();
    let mut line_start = 0;
    let mut mapped_caret = 0;
    let mut out_offset = 0;
    for line in text.split('\n') {
        let local = (at >= line_start && at <= line_start + line.len()).then(|| at - line_start);
        let mut dropped = false;
        let mut next = String::with_capacity(line.len());
        let mut cursor = 0;
        let mut mapped: Option<usize> = None;
        for captures in marker_pattern().captures_iter(line) {
            let whole = captures.get(0).expect("the whole match");
            let start = whole.start();
            if let Some(local) = local {
                if local >= cursor && local <= start && mapped.is_none() {
                    mapped = Some(next.len() + (local - cursor));
                }
            }
            next.push_str(&line[cursor..start]);
            match captures[1].parse::<u32>() {
                Ok(index) if index == removed => dropped = true,
                Ok(index) if index > removed => next.push_str(&image_marker(index - 1)),
                _ => next.push_str(whole.as_str()),
            }
            cursor = whole.end();
            if local == Some(cursor) && mapped.is_none() {
                mapped = Some(next.len());
            }
        }
        if let Some(local) = local {
            if local >= cursor && mapped.is_none() {
                mapped = Some(next.len() + (local - cursor));
            }
        }
        next.push_str(&line[cursor..]);
        if dropped {
            // TS passes -1 (no caret on this line) through as "past the end".
            let (collapsed, at) = collapse_runs(&next, mapped.unwrap_or(usize::MAX));
            next = collapsed.trim_end_matches([' ', '\t']).to_string();
            let at = at.min(next.len());
            mapped = mapped.map(|_| at);
            if line.starts_with(&image_marker(removed)) {
                let lead = next.len() - next.trim_start_matches([' ', '\t']).len();
                next = next[lead..].to_string();
                mapped = mapped.map(|at| at.saturating_sub(lead));
            }
        }
        if let (Some(_), Some(mapped)) = (local, mapped) {
            mapped_caret = out_offset + mapped;
        }
        out_offset += next.len() + 1;
        out.push(next);
        line_start += line.len() + 1;
    }
    (out.join("\n"), mapped_caret)
}

/// How many images the strip holds.
pub fn image_count(strip: &[AttachmentKind]) -> usize {
    strip.iter().filter(|kind| **kind == AttachmentKind::Image).count()
}

/// A pick staged onto the strip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staged {
    /// The strip after the add.
    pub strip: Vec<AttachmentKind>,
    /// For each pick: true when it joined the strip.
    pub taken: Vec<bool>,
    /// The draft with one marker per TAKEN image at the caret.
    pub text: String,
    /// The caret behind the last inserted marker (the clamped caret without one).
    pub caret: usize,
    /// Picks refused for size or type.
    pub rejected: usize,
    /// Images over the image cap.
    pub overflow: usize,
    /// Files over the file cap.
    pub file_overflow: usize,
}

impl Staged {
    /// The toasts this stage owes, in order.
    pub fn toasts(&self) -> Vec<&'static str> {
        let mut toasts = Vec::new();
        if self.rejected > 0 {
            toasts.push(ATTACHMENT_REJECTED_TOAST);
        }
        if self.overflow > 0 {
            toasts.push(IMAGE_CAP_TOAST);
        }
        if self.file_overflow > 0 {
            toasts.push(FILE_CAP_TOAST);
        }
        toasts
    }
}

/// Stages a pick onto the strip. The image count BEFORE the add is the
/// numbering base: image #1 is the strip's first IMAGE (files carry no
/// marker and never count).
pub fn stage_attachments(strip: &[AttachmentKind], add: &[PickKind], text: &str, caret: usize) -> Staged {
    let base = image_count(strip);
    let mut image_room = COMPOSER_MAX_IMAGES.saturating_sub(base);
    let mut file_room = COMPOSER_MAX_FILES.saturating_sub(strip.len() - base);
    let mut staged = Staged {
        strip: strip.to_vec(),
        taken: Vec::with_capacity(add.len()),
        text: text.to_string(),
        caret: clamp_to_char_boundary(text, caret),
        rejected: 0,
        overflow: 0,
        file_overflow: 0,
    };
    let mut number = base as u32;
    for kind in add {
        let took = match kind {
            PickKind::Rejected => {
                staged.rejected += 1;
                false
            }
            PickKind::Image if image_room == 0 => {
                staged.overflow += 1;
                false
            }
            PickKind::Image => {
                image_room -= 1;
                staged.strip.push(AttachmentKind::Image);
                number += 1;
                let (next, after) = insert_image_marker(&staged.text, staged.caret, number);
                staged.text = next;
                staged.caret = after;
                true
            }
            PickKind::File if file_room == 0 => {
                staged.file_overflow += 1;
                false
            }
            PickKind::File => {
                file_room -= 1;
                staged.strip.push(AttachmentKind::File);
                true
            }
        };
        staged.taken.push(took);
    }
    staged
}

/// The strip's X on the entry at `at`. An image's markers leave the draft
/// and every higher one slides down, so the numbers keep matching the strip;
/// a file has none. The caret (bytes) stays where it was
/// ([`renumber_with_caret`]). Returns the strip, the draft and the caret.
pub fn drop_attachment(strip: &[AttachmentKind], at: usize, text: &str, caret: usize) -> (Vec<AttachmentKind>, String, usize) {
    let caret = clamp_to_char_boundary(text, caret);
    let Some(kind) = strip.get(at) else {
        return (strip.to_vec(), text.to_string(), caret);
    };
    let rest: Vec<AttachmentKind> = strip.iter().enumerate().filter(|(index, _)| *index != at).map(|(_, kind)| *kind).collect();
    if *kind != AttachmentKind::Image {
        return (rest, text.to_string(), caret);
    }
    let number = image_count(&strip[..at]) as u32 + 1;
    let (text, caret) = renumber_with_caret(text, number, caret);
    (rest, text, caret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    const COMPOSER_ATTACHMENTS_JSON: &str = include_str!("../../../../../packages/domain-contract/fixtures/composer-attachments.json");

    #[derive(Deserialize)]
    struct InsertInput {
        text: String,
        caret: usize,
        index: u32,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct TextCaret {
        text: String,
        caret: usize,
    }

    #[derive(Deserialize)]
    struct RenumberInput {
        text: String,
        removed: u32,
    }

    #[derive(Deserialize)]
    struct StageInput {
        strip: Vec<String>,
        add: Vec<String>,
        text: String,
        caret: usize,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct StageExpected {
        strip: Vec<String>,
        taken: Vec<bool>,
        text: String,
        caret: usize,
        rejected: usize,
        overflow: usize,
        file_overflow: usize,
        toasts: Vec<String>,
    }

    #[derive(Deserialize)]
    struct DropInput {
        strip: Vec<String>,
        at: usize,
        text: String,
        caret: usize,
    }

    #[derive(Deserialize, Debug, PartialEq)]
    struct DropExpected {
        strip: Vec<String>,
        text: String,
        caret: usize,
    }

    /// The fixture's offsets are UTF-16 code units (JS string indices).
    fn utf16_to_byte(text: &str, at: usize) -> usize {
        let mut units = 0;
        for (byte, ch) in text.char_indices() {
            if units >= at {
                return byte;
            }
            units += ch.len_utf16();
        }
        text.len()
    }

    fn byte_to_utf16(text: &str, at: usize) -> usize {
        text[..at].encode_utf16().count()
    }

    #[derive(Deserialize)]
    struct Case<I, E> {
        name: String,
        input: I,
        expected: E,
    }

    #[derive(Deserialize)]
    struct Fixture {
        insert: Vec<Case<InsertInput, TextCaret>>,
        renumber: Vec<Case<RenumberInput, String>>,
        stage: Vec<Case<StageInput, StageExpected>>,
        drop: Vec<Case<DropInput, DropExpected>>,
    }

    fn kind(name: &str) -> AttachmentKind {
        match name {
            "image" => AttachmentKind::Image,
            "file" => AttachmentKind::File,
            other => panic!("unknown kind {other}"),
        }
    }

    fn pick(name: &str) -> PickKind {
        match name {
            "image" => PickKind::Image,
            "file" => PickKind::File,
            "rejected" => PickKind::Rejected,
            other => panic!("unknown pick {other}"),
        }
    }

    fn names(strip: &[AttachmentKind]) -> Vec<String> {
        strip.iter().map(|kind| if *kind == AttachmentKind::Image { "image" } else { "file" }.to_string()).collect()
    }

    fn fixture() -> Fixture {
        serde_json::from_str(COMPOSER_ATTACHMENTS_JSON).expect("composer-attachments.json parses")
    }

    #[test]
    fn replays_every_insert_case() {
        let cases = fixture().insert;
        assert!(!cases.is_empty());
        for case in cases {
            let (text, caret) = insert_image_marker(&case.input.text, case.input.caret, case.input.index);
            assert_eq!(TextCaret { text, caret }, case.expected, "{}", case.name);
        }
    }

    #[test]
    fn replays_every_renumber_case() {
        let cases = fixture().renumber;
        assert!(!cases.is_empty());
        for case in cases {
            assert_eq!(renumber_image_markers(&case.input.text, case.input.removed), case.expected, "{}", case.name);
        }
    }

    #[test]
    fn replays_every_stage_case() {
        let cases = fixture().stage;
        assert!(!cases.is_empty());
        for case in cases {
            let strip: Vec<AttachmentKind> = case.input.strip.iter().map(|name| kind(name)).collect();
            let add: Vec<PickKind> = case.input.add.iter().map(|name| pick(name)).collect();
            let staged = stage_attachments(&strip, &add, &case.input.text, case.input.caret);
            let got = StageExpected {
                strip: names(&staged.strip),
                taken: staged.taken.clone(),
                text: staged.text.clone(),
                caret: staged.caret,
                rejected: staged.rejected,
                overflow: staged.overflow,
                file_overflow: staged.file_overflow,
                toasts: staged.toasts().iter().map(|toast| toast.to_string()).collect(),
            };
            assert_eq!(got, case.expected, "{}", case.name);
        }
    }

    #[test]
    fn replays_every_drop_case() {
        let cases = fixture().drop;
        assert!(!cases.is_empty());
        for case in cases {
            let strip: Vec<AttachmentKind> = case.input.strip.iter().map(|name| kind(name)).collect();
            let caret = utf16_to_byte(&case.input.text, case.input.caret);
            let (rest, text, caret) = drop_attachment(&strip, case.input.at, &case.input.text, caret);
            let caret = byte_to_utf16(&text, caret);
            assert_eq!(DropExpected { strip: names(&rest), text, caret }, case.expected, "{}", case.name);
        }
    }

    #[test]
    fn a_caret_inside_a_multi_byte_character_clamps() {
        assert_eq!(insert_image_marker("é", 1, 1).0, "[Image #1] é");
    }

    #[test]
    fn the_x_keeps_a_byte_caret_past_multi_byte_text() {
        // "é" is two bytes: the caret behind the marker lands where it began.
        assert_eq!(renumber_with_caret("é [Image #1] ok", 1, 13), ("é ok".to_string(), 3));
    }
}
