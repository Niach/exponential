//! EXP-261 byte-parity gate for the vendored WYSIWYG editor
//! ([`gpui_markdown_editor`]): every [`super::serialize::CONTRACT_FIXTURES`]
//! string must survive the vendored engine's own parse→serialize unchanged —
//! its serializer IS the canonical form on the save path (the WYSIWYG editor
//! deliberately does NOT run comrak [`super::canonicalize`] over its output,
//! which would destroy constructs like tables that the vendored engine
//! legitimately round-trips). This suite is what makes that boundary safe.

use gpui::{AppContext as _, TestAppContext};
use gpui_markdown_editor::MarkdownEditor;

use super::serialize::CONTRACT_FIXTURES;

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
