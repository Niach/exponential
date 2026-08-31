//! EXP-261 byte-parity gate for the vendored WYSIWYG editor
//! ([`gpui_markdown_editor`]): every [`super::serialize::CONTRACT_FIXTURES`]
//! string must survive the vendored engine's own parse→serialize unchanged —
//! its serializer IS the canonical form on the save path (the WYSIWYG editor
//! deliberately does NOT run comrak [`super::canonicalize`] over its output,
//! which would destroy constructs like tables that the vendored engine
//! legitimately round-trips). This suite is what makes that boundary safe.

use std::sync::Arc;

use gpui::{AppContext as _, SharedString, TestAppContext};
use gpui_markdown_editor::{
    MarkdownEditor, MarkdownEditorEnvironment, MarkdownEditorMode, MarkdownEditorOptions,
    ReferenceDecorator, ReferenceKind, ReferenceSpan,
};

use super::serialize::CONTRACT_FIXTURES;
use super::{normalize_for_wysiwyg, scan_issue_refs, scan_mentions};

#[gpui::test]
async fn contract_fixtures_survive_wysiwyg_round_trip(cx: &mut TestAppContext) {
    for (name, md) in CONTRACT_FIXTURES {
        let editor = cx.new(|cx| MarkdownEditor::from_markdown(cx, (*md).to_string(), None));
        let first = editor.update(cx, |editor, cx| editor.markdown(cx));
        assert_eq!(&first, md, "wysiwyg round-trip diverged for fixture {name}");

        // Idempotency: re-importing the serialized form must be a fixpoint.
        let editor = cx.new(|cx| MarkdownEditor::from_markdown(cx, first.clone(), None));
        let second = editor.update(cx, |editor, cx| editor.markdown(cx));
        assert_eq!(
            second, first,
            "wysiwyg round-trip not idempotent for fixture {name}"
        );
    }
}

// Known, deliberate divergence (documented in the vendored crate's NOTICE):
// an intra-word `*` (e.g. `c*d`) IS emphasis-capable in GFM, so the vendored
// serializer escapes it (`c\*d`) — semantically identical on every client,
// one-time byte churn on first WYSIWYG save of such text.
#[gpui::test]
async fn intra_word_asterisk_escapes_once_then_stays_stable(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| MarkdownEditor::from_markdown(cx, "a_b and c*d".to_string(), None));
    let first = editor.update(cx, |editor, cx| editor.markdown(cx));
    assert_eq!(first, "a_b and c\\*d");
    let editor = cx.new(|cx| MarkdownEditor::from_markdown(cx, first.clone(), None));
    let second = editor.update(cx, |editor, cx| editor.markdown(cx));
    assert_eq!(second, first);
}

/// EXP-697: the `\`-at-end-of-line hard break web, iOS and Android write is
/// not in the vendored engine's escape table, so it used to reach the reader
/// as a LITERAL backslash and get written straight back out. The host pre-pass
/// (`normalize_for_wysiwyg`) hands the engine the two-space spelling it does
/// round-trip, so the break survives and the backslash never shows up.
#[gpui::test]
async fn backslash_hard_breaks_reach_the_engine_as_two_space_breaks(cx: &mut TestAppContext) {
    let round_trip = |markdown: &str, cx: &mut TestAppContext| {
        let editor = cx.new(|cx| {
            MarkdownEditor::from_markdown(cx, normalize_for_wysiwyg(markdown), None)
        });
        editor.update(cx, |editor, cx| editor.markdown(cx))
    };

    for (stored, expected) in [
        ("alpha\\\nbeta", "alpha  \nbeta"),
        ("> alpha\\\n> beta", "> alpha  \n> beta"),
        ("- alpha\\\n  beta", "- alpha  \n  beta"),
    ] {
        let saved = round_trip(stored, cx);
        assert_eq!(saved, expected, "hard break lost for {stored:?}");
        // …and the form it now stores is a fixpoint.
        assert_eq!(round_trip(&saved, cx), saved, "not a fixpoint: {saved:?}");
    }

    // An ESCAPED backslash at end of line is a literal character plus a soft
    // break, not a hard break — it must survive as the character.
    assert_eq!(round_trip("back\\\\\nmore", cx), "back\\\\\nmore");
    // Code is never touched.
    assert_eq!(
        round_trip("```\nalpha\\\nbeta\n```", cx),
        "```\nalpha\\\nbeta\n```"
    );
}

/// EXP-322: chip TITLES are display-only. They are injected into the string
/// the editor shapes, never into the document — so a decorator that decorates
/// EVERY token with a long title must leave the serialized bytes untouched.
/// This is the gate that proves the GFM contract survived the offset-map
/// surgery; if any display text ever leaks into serialization, it goes red.
struct DecorateEverything;

impl ReferenceDecorator for DecorateEverything {
    fn scan(&self, text: &str) -> Vec<ReferenceSpan> {
        let mut spans: Vec<ReferenceSpan> = scan_mentions(text)
            .into_iter()
            .map(|range| ReferenceSpan {
                range,
                kind: ReferenceKind::Mention,
                display_suffix: None,
                icon: None,
            })
            .chain(scan_issue_refs(text).into_iter().map(|range| ReferenceSpan {
                range,
                kind: ReferenceKind::IssueRef,
                display_suffix: Some(SharedString::from(
                    "a deliberately long display-only chip title",
                )),
                // EXP-423: the icon triggers the leading NBSP gutter
                // injection — this gate proves the gutter (like the title)
                // never leaks into serialization.
                icon: Some(gpui_markdown_editor::ChipIcon {
                    svg_path: SharedString::from("icons/circle.svg"),
                    color: gpui::Hsla::default(),
                }),
            }))
            .collect();
        spans.sort_by_key(|span| span.range.start);
        spans
    }
}

/// The injection lives in `BlockTextElement::request_layout`, so a decorated
/// editor that is never laid out never injects anything — asserting on such an
/// editor's serialization proves nothing. Every chip assertion below therefore
/// PAINTS the editor in a real window first, and only then serializes.
fn painted_chip_editor<'a>(
    markdown: &str,
    mode: MarkdownEditorMode,
    cx: &'a mut TestAppContext,
) -> (
    gpui::Entity<MarkdownEditor>,
    &'a mut gpui::VisualTestContext,
) {
    let markdown = markdown.to_string();
    let (editor, cx) = cx.add_window_view(|_window, cx| {
        MarkdownEditor::new(
            markdown,
            MarkdownEditorOptions {
                mode,
                environment: MarkdownEditorEnvironment {
                    reference_decorator: Some(Arc::new(DecorateEverything)),
                    ..MarkdownEditorEnvironment::default()
                },
                ..MarkdownEditorOptions::default()
            },
            cx,
        )
    });
    // Two frames: the first applies the pending focus handshake, the second
    // lays out (and therefore injects into) the focused block as well.
    for _ in 0..2 {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    (editor, cx)
}

#[gpui::test]
async fn contract_fixtures_survive_a_chip_decorating_round_trip(cx: &mut TestAppContext) {
    for (name, md) in CONTRACT_FIXTURES {
        let (editor, cx) = painted_chip_editor(md, MarkdownEditorMode::Rendered, cx);
        let round_tripped = editor.update(cx, |editor, cx| editor.markdown(cx));
        assert_eq!(
            &round_tripped, md,
            "chip titles leaked into serialization for fixture {name}"
        );
    }
}

/// The tokens the chips actually decorate, exercised end to end.
#[gpui::test]
async fn issue_ref_and_mention_tokens_stay_plain_gfm_text(cx: &mut TestAppContext) {
    let src = "Ping @ada@example.com about #EXP-42 and #EXP-7";
    let (editor, cx) = painted_chip_editor(src, MarkdownEditorMode::Rendered, cx);
    assert_eq!(editor.update(cx, |editor, cx| editor.markdown(cx)), src);
}

/// EXP-322 D1/D2: the display-only title must never reach a surface whose job
/// is showing literal bytes — a fenced code block, an inline code span, or the
/// raw-source view. This paints the real thing and reads the source back.
#[gpui::test]
async fn code_and_raw_source_keep_their_literal_issue_refs(cx: &mut TestAppContext) {
    let src = "```ts\n// see #EXP-42 for context\n```\n\nprose `#EXP-42` and #EXP-42";
    {
        let (editor, cx) = painted_chip_editor(src, MarkdownEditorMode::Rendered, cx);
        assert_eq!(editor.update(cx, |editor, cx| editor.markdown(cx)), src);
    }
    // The raw-source view is the same document with every byte shown literally
    // — a chip title there would be the editor lying about the file.
    let (editor, cx) = painted_chip_editor(src, MarkdownEditorMode::Source, cx);
    assert_eq!(editor.update(cx, |editor, cx| editor.markdown(cx)), src);
}
