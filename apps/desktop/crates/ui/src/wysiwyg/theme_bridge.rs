//! EXP-261: maps the app theme (gpui-component `cx.theme()`, driven by our
//! generated design tokens) onto the vendored WYSIWYG editor's
//! [`MarkdownEditorTheme`]. Starts from the vendored built-in matching the
//! current mode and overrides the load-bearing tokens; everything else keeps
//! the vendored defaults, which are close cousins of our zinc palette.

use std::sync::Arc;

use gpui::{App, Hsla};
use gpui_component::ActiveTheme as _;
use gpui_markdown_editor::{FontWeightDef, MarkdownEditorTheme};

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
    // EXP-322: mirrors web `.issue-ref-pill` (`background: var(--accent)`,
    // EXP-381: `1px solid var(--border)` — accent barely clears the surface
    // behind it, so the border carries the pill's legibility exactly like it
    // does on web). EXP-423 Linear look: the TITLE renders in the
    // foreground and the `#IDENT` token is muted (web `::after` foreground /
    // muted identifier).
    c.reference_bg = app.accent;
    c.reference_text = app.foreground;
    c.reference_token_text = app.muted_foreground;
    c.reference_border = app.border;
    c.cursor = app.caret;
    c.selection = app.selection;
    c.task_checkbox_border = app.border;
    c.task_checkbox_bg = Hsla::transparent_black();
    c.task_checkbox_checked_bg = app.primary;
    c.task_checkbox_check = app.primary_foreground;
    c.table_border = app.border;
    c.table_header_bg = app.muted;
    c.table_cell_bg = Hsla::transparent_black();
    // EXP-726: the table maintenance chrome. The vendored defaults are a fixed
    // blue/slate pair that reads as a foreign accent inside our zinc palette;
    // the axis bands are the app's hover/selected row fills and the append
    // buttons the secondary button triple.
    c.table_cell_active_outline = app.ring;
    c.table_axis_preview_bg = app.accent;
    c.table_axis_selected_bg = app.list_active;
    c.table_append_button_bg = app.secondary;
    c.table_append_button_hover = app.secondary_hover;
    c.table_append_button_text = app.secondary_foreground;
    c.image_placeholder_bg = app.muted;
    c.image_placeholder_border = app.border;
    c.image_placeholder_text = app.muted_foreground;
    c.image_caption_text = app.muted_foreground;
    c.scrollbar_thumb = app.scrollbar_thumb;

    theme.fonts.ui_family = app.font_family.to_string();
    theme.fonts.mono_family = app.mono_font_family.to_string();

    apply_typography(&mut theme, f32::from(app.font_size).max(1.0));

    Arc::new(theme)
}

/// Web-parity type scale (`.tiptap-content` in `apps/web/src/styles.css`),
/// expressed against the app's rem — which gpui-component pins to
/// `theme.font_size`, i.e. [`theme::FONT_SIZE_PX`] rather than web's 16px
/// root. The vendored defaults are absolute pixels tuned for a
/// full-window 17px editor; left alone they render the description half again
/// larger than every other surface in the app.
fn apply_typography(theme: &mut MarkdownEditorTheme, rem: f32) {
    let t = &mut theme.typography;
    t.text_size = rem * 0.875;
    // `text_line_height` is consumed as `rems(..)`, so fold web's
    // font-size-relative 1.625 into the body's own rem multiple.
    t.text_line_height = 0.875 * 1.625;
    t.h1_size = rem * 1.25;
    t.h2_size = rem * 1.1;
    t.h3_size = rem;
    t.h4_size = rem;
    t.h5_size = rem;
    t.h6_size = rem;
    // Web headings are all `font-weight: 600`.
    t.h1_weight = FontWeightDef::Semibold;
    t.h2_weight = FontWeightDef::Semibold;
    t.h3_weight = FontWeightDef::Semibold;
    t.h4_weight = FontWeightDef::Semibold;
    t.h5_weight = FontWeightDef::Semibold;
    t.h6_weight = FontWeightDef::Semibold;
    t.code_size = rem * 0.8;

    // Web draws no rule under h1/h2 — drop the vendored underline and the
    // padding that only existed to clear it.
    let d = &mut theme.dimensions;
    d.h1_border_width = 0.0;
    d.h1_padding_bottom = 0.0;
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
