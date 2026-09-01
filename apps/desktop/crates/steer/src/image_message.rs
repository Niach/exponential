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

/// How many images one steered message may carry (web/iOS/Android parity).
pub const MAX_STEER_IMAGES: usize = 4;

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
}
