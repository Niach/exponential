//! EXP-261: maps the app theme (gpui-component `cx.theme()`, driven by our
//! generated design tokens) onto the vendored WYSIWYG editor's
//! [`MarkdownEditorTheme`]. Starts from the vendored built-in matching the
//! current mode and overrides the load-bearing tokens; everything else keeps
//! the vendored defaults, which are close cousins of our zinc palette.

use std::sync::Arc;

use gpui::{App, Hsla};
use gpui_component::ActiveTheme as _;
use gpui_markdown_editor::MarkdownEditorTheme;

/// Build the vendored editor theme from the current app theme. Call again on
/// theme changes (the wrapper observes the mode) — presentation only.
pub(crate) fn editor_theme(cx: &App) -> Arc<MarkdownEditorTheme> {
    let app = cx.theme();
    let mut theme = if app.mode.is_dark() {
        MarkdownEditorTheme::default_theme()
    } else {
        MarkdownEditorTheme::light_theme()
    };

    let c = &mut theme.colors;
    // The description slot is chrome-less (EXP-256): the editor paints no
    // background of its own — the detail column's background shows through.
    c.editor_background = Hsla::transparent_black();
    c.source_mode_block_bg = app.muted;
    c.text_default = app.foreground;
    c.text_link = app.link;
    c.text_placeholder = app.muted_foreground;
    c.text_h1 = app.foreground;
    c.text_h2 = app.foreground;
    c.text_h3 = app.foreground;
    c.text_h4 = app.foreground;
    c.text_h5 = app.foreground;
    c.text_h6 = app.foreground;
    c.text_quote = app.muted_foreground;
    c.border_quote = app.border;
    c.separator_color = app.border;
    c.code_bg = app.muted;
    c.code_text = app.foreground;
    c.cursor = app.caret;
    c.selection = app.selection;
    c.task_checkbox_border = app.border;
    c.task_checkbox_bg = Hsla::transparent_black();
    c.task_checkbox_checked_bg = app.primary;
    c.task_checkbox_check = app.primary_foreground;
    c.table_border = app.border;
    c.table_header_bg = app.muted;
    c.table_cell_bg = Hsla::transparent_black();
    c.image_placeholder_bg = app.muted;
    c.image_placeholder_border = app.border;
    c.image_placeholder_text = app.muted_foreground;
    c.image_caption_text = app.muted_foreground;
    c.scrollbar_thumb = app.scrollbar_thumb;

    theme.fonts.ui_family = app.font_family.to_string();
    theme.fonts.mono_family = app.mono_font_family.to_string();

    Arc::new(theme)
}

/// [`editor_theme`] with the per-instance empty-document placeholder text
/// (the vendored editor reads it off the theme's `placeholders`).
pub(crate) fn editor_theme_with_placeholder(
    cx: &App,
    placeholder: &str,
) -> Arc<MarkdownEditorTheme> {
    let mut theme = (*editor_theme(cx)).clone();
    theme.placeholders.empty_editing = placeholder.to_string();
    Arc::new(theme)
}
