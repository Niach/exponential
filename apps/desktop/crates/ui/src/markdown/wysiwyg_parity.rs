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
    MarkdownEditor, MarkdownEditorEnvironment, ReferenceDecorator, ReferenceKind, ReferenceSpan,
};

use super::serialize::CONTRACT_FIXTURES;
use super::{scan_issue_refs, scan_mentions};

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
            })
            .chain(scan_issue_refs(text).into_iter().map(|range| ReferenceSpan {
                range,
                kind: ReferenceKind::IssueRef,
                display_suffix: Some(SharedString::from(
                    "a deliberately long display-only chip title",
                )),
            }))
            .collect();
        spans.sort_by_key(|span| span.range.start);
        spans
    }
}

#[gpui::test]
async fn contract_fixtures_survive_a_chip_decorating_round_trip(cx: &mut TestAppContext) {
    for (name, md) in CONTRACT_FIXTURES {
        let editor = cx.new(|cx| {
            MarkdownEditor::with_environment(
                (*md).to_string(),
                MarkdownEditorEnvironment {
                    reference_decorator: Some(Arc::new(DecorateEverything)),
                    ..MarkdownEditorEnvironment::default()
                },
                cx,
            )
        });
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
    let editor = cx.new(|cx| {
        MarkdownEditor::with_environment(
            src.to_string(),
            MarkdownEditorEnvironment {
                reference_decorator: Some(Arc::new(DecorateEverything)),
                ..MarkdownEditorEnvironment::default()
            },
            cx,
        )
    });
    assert_eq!(editor.update(cx, |editor, cx| editor.markdown(cx)), src);
}
