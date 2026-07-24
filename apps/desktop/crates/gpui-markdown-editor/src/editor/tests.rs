use std::sync::Arc;

use gpui::{AppContext, TestAppContext};

use super::Editor;
use crate::{
    MarkdownEditorEnvironment, MarkdownEditorMode, MarkdownEditorOptions, MarkdownEditorTheme,
    SourceSelection,
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
