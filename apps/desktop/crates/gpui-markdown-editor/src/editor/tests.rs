use std::sync::Arc;

use gpui::{AppContext, TestAppContext};

use super::Editor;
use crate::{
    FormatCommand, MarkdownEditorEnvironment, MarkdownEditorMode, MarkdownEditorOptions,
    MarkdownEditorTheme, SourceSelection,
};

#[gpui::test]
async fn rendered_source_round_trip_preserves_markdown(cx: &mut TestAppContext) {
    let markdown = "# Title\n\nParagraph with **bold**.\n\n| A | B |\n| --- | --- |\n| 1 | 2 |";
    let editor = cx.new(|cx| Editor::new(markdown, MarkdownEditorOptions::default(), cx));

    editor.update(cx, |editor, cx| {
        assert_eq!(editor.markdown(cx), markdown);
        editor.set_mode(MarkdownEditorMode::Source, cx);
        assert_eq!(editor.markdown(cx), markdown);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        assert_eq!(editor.markdown(cx), markdown);
    });
}

// EXP-261: GFM treats an ordered list's FIRST number as semantic (the list
// start). The importer keeps it and the serializer re-emits the run from it —
// `3. three` must never normalize to `1. three`.
#[gpui::test]
async fn ordered_list_start_number_round_trips(cx: &mut TestAppContext) {
    let markdown = "3. three\n4. four";
    let editor = cx.new(|cx| Editor::new(markdown, MarkdownEditorOptions::default(), cx));
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.markdown(cx), "3. three\n4. four");
    });

    // Followers renumber from the head (GFM: only the first number matters).
    let editor = cx.new(|cx| Editor::new("3. a\n9. b\n2. c", MarkdownEditorOptions::default(), cx));
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.markdown(cx), "3. a\n4. b\n5. c");
    });

    // Separate runs each keep their own start; lists from 1 stay unchanged.
    let editor = cx.new(|cx| {
        Editor::new(
            "1. one\n2. two\n\nbetween\n\n5. five\n6. six",
            MarkdownEditorOptions::default(),
            cx,
        )
    });
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.markdown(cx), "1. one\n2. two\n\nbetween\n\n5. five\n6. six");
    });

    // Nested runs seed independently of the outer run.
    let editor = cx.new(|cx| {
        Editor::new(
            "2. outer\n  4. inner\n  5. inner two\n3. outer two",
            MarkdownEditorOptions::default(),
            cx,
        )
    });
    editor.update(cx, |editor, cx| {
        assert_eq!(
            editor.markdown(cx),
            "2. outer\n  4. inner\n  5. inner two\n3. outer two"
        );
    });
}

#[gpui::test]
async fn replace_markdown_preserves_the_selected_mode(cx: &mut TestAppContext) {
    let options = MarkdownEditorOptions {
        mode: MarkdownEditorMode::Source,
        ..MarkdownEditorOptions::default()
    };
    let editor = cx.new(|cx| Editor::new("alpha", options, cx));

    editor.update(cx, |editor, cx| {
        editor.replace_markdown("beta\ngamma", cx);
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
        assert_eq!(editor.markdown(cx), "beta\ngamma");
    });
}

#[gpui::test]
async fn theme_and_strings_are_isolated_per_instance(cx: &mut TestAppContext) {
    let mut first_theme = MarkdownEditorTheme::default_theme();
    first_theme.name = "first".into();
    let mut second_theme = MarkdownEditorTheme::default_theme();
    second_theme.name = "second".into();

    let first = cx.new(|cx| {
        Editor::new(
            "one",
            MarkdownEditorOptions {
                environment: MarkdownEditorEnvironment {
                    theme: Arc::new(first_theme),
                    ..MarkdownEditorEnvironment::default()
                },
                ..MarkdownEditorOptions::default()
            },
            cx,
        )
    });
    let second = cx.new(|cx| {
        Editor::new(
            "two",
            MarkdownEditorOptions {
                environment: MarkdownEditorEnvironment {
                    theme: Arc::new(second_theme),
                    ..MarkdownEditorEnvironment::default()
                },
                ..MarkdownEditorOptions::default()
            },
            cx,
        )
    });

    assert_eq!(
        first.read_with(cx, |editor, _| editor.environment().theme.name.clone()),
        "first"
    );
    assert_eq!(
        second.read_with(cx, |editor, _| editor.environment().theme.name.clone()),
        "second"
    );
}

#[gpui::test]
async fn set_theme_changes_only_presentation_state(cx: &mut TestAppContext) {
    let editor = cx.new(|cx| {
        Editor::new(
            "alpha\n\nbeta",
            MarkdownEditorOptions {
                mode: MarkdownEditorMode::Source,
                ..MarkdownEditorOptions::default()
            },
            cx,
        )
    });

    editor.update(cx, |editor, cx| {
        editor.set_source_selection(
            SourceSelection {
                range: 1..4,
                reversed: true,
            },
            cx,
        );
        let markdown = editor.markdown(cx);
        let revision = editor.revision();
        let selection = editor.source_selection(cx);
        let strings = editor.environment().strings.clone();
        let can_undo = editor.can_undo();
        let can_redo = editor.can_redo();
        let theme = Arc::new(MarkdownEditorTheme::light_theme());

        editor.set_theme(theme.clone(), cx);

        assert!(Arc::ptr_eq(&editor.theme(), &theme));
        assert!(Arc::ptr_eq(&editor.environment().strings, &strings));
        assert_eq!(editor.markdown(cx), markdown);
        assert_eq!(editor.revision(), revision);
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
        assert_eq!(editor.source_selection(cx), selection);
        assert_eq!(editor.can_undo(), can_undo);
        assert_eq!(editor.can_redo(), can_redo);
        assert!(
            editor
                .document
                .visible_blocks()
                .iter()
                .all(|visible| { Arc::ptr_eq(&visible.entity.read(cx).environment.theme, &theme) })
        );
    });
}

/// EXP-568 vendoring: the floating rail's "Text" entry. `Paragraph` demotes
/// any toolbar-reachable block kind and — alone among the block commands — is
/// idempotent: pressing Text on a paragraph must not toggle it into anything.
#[gpui::test]
async fn paragraph_format_demotes_and_never_toggles(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(crate::actions::default_key_bindings());
    });
    let (editor, cx) =
        cx.add_window_view(|_window, cx| Editor::from_markdown(cx, "Title".to_string(), None));

    cx.update(|window, app| {
        editor.update(app, |editor, cx| {
            editor.focus_first_block(cx);

            editor.apply_format(FormatCommand::Heading(2), window, cx);
            assert_eq!(editor.markdown(cx), "## Title");
            editor.apply_format(FormatCommand::Paragraph, window, cx);
            assert_eq!(editor.markdown(cx), "Title");
            // Text on a paragraph is a no-op, not a toggle.
            editor.apply_format(FormatCommand::Paragraph, window, cx);
            assert_eq!(editor.markdown(cx), "Title");

            editor.apply_format(FormatCommand::BulletList, window, cx);
            assert_eq!(editor.markdown(cx), "- Title");
            editor.apply_format(FormatCommand::Paragraph, window, cx);
            assert_eq!(editor.markdown(cx), "Title");

            editor.apply_format(FormatCommand::Quote, window, cx);
            assert_eq!(editor.markdown(cx), "> Title");
            editor.apply_format(FormatCommand::Paragraph, window, cx);
            assert_eq!(editor.markdown(cx), "Title");
        });
    });
}

/// EXP-568 vendoring: a selection the USER moved (drag, shift-arrow, a click
/// that collapses it) reaches the host as `SelectionChanged` — the floating
/// rail's visibility hangs on it. The diff guard is the load-bearing half:
/// the host repaints on the event, so an unguarded per-frame emit would spin.
#[gpui::test]
async fn user_selection_moves_emit_selection_changed_once(cx: &mut TestAppContext) {
    use std::cell::Cell;
    use std::rc::Rc;

    cx.update(|cx| {
        cx.bind_keys(crate::actions::default_key_bindings());
    });
    let (editor, cx) = cx
        .add_window_view(|_window, cx| Editor::from_markdown(cx, "alpha bravo".to_string(), None));

    let seen = Rc::new(Cell::new(0usize));
    let _subscription = cx.update(|_window, app| {
        let seen = seen.clone();
        app.subscribe(
            &editor,
            move |_editor, event: &crate::MarkdownEditorEvent, _cx| {
                if matches!(event, crate::MarkdownEditorEvent::SelectionChanged(_)) {
                    seen.set(seen.get() + 1);
                }
            },
        )
    });

    let redraw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    };

    // The construction-time (empty) selection IS the reported baseline, so
    // the first paint says nothing.
    redraw(cx);
    assert_eq!(seen.get(), 0);

    let block = editor.read_with(cx, |editor, _cx| {
        editor.document.visible_blocks()[0].entity.clone()
    });
    cx.update(|_window, app| {
        block.update(app, |block, cx| {
            block.selected_range = 0..5;
            cx.notify();
        });
    });
    redraw(cx);
    assert_eq!(seen.get(), 1);

    // Steady state: repaints with an unchanged selection stay silent.
    redraw(cx);
    redraw(cx);
    assert_eq!(seen.get(), 1);

    // Collapsing it back is a change too — that is what dismisses the rail.
    cx.update(|_window, app| {
        block.update(app, |block, cx| {
            block.selected_range = 0..0;
            cx.notify();
        });
    });
    redraw(cx);
    assert_eq!(seen.get(), 2);
}
