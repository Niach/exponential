//! EXP-261: the static formatting toolbar above the WYSIWYG description.
//!
//! Button-for-button the web `StaticToolbar`
//! (`apps/web/src/components/issue-editor/markdown-editor.tsx`): H1–H3, bold /
//! italic / strikethrough / code, link, quote, bullet / ordered / task list,
//! clear formatting, insert image — always visible (discoverability), no
//! selection bubble. Every button drives the vendored editor's
//! [`FormatCommand`] surface, so the document mutates through the same tested
//! block machinery that typing markdown shortcuts uses.

use gpui::prelude::FluentBuilder as _;
use gpui::{div, App, Context, Entity, IntoElement, ParentElement as _, Styled as _, Window};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState},
    ActiveTheme as _, Icon, Selectable as _, Sizable as _,
};
use gpui_markdown_editor::{FormatCommand, FormatState};

use super::description::WysiwygDescription;
use crate::icons::registry;
use crate::ExpIcon;

/// EXP-285: horizontal toolbar inset — the editor's `block_padding_x` (12)
/// minus the xsmall icon-button's own glyph inset ((20px button − 12px icon)
/// / 2 = 4), so the first glyph lines up with the title/description text in
/// BOTH host slots (issue detail and the create-issue dialog).
/// EXP-288: +2px optical compensation — the lucide glyphs' strokes start a
/// hair inside their 12px SVG box while text glyphs sit on the pen origin,
/// so the box-perfect alignment still READ as slightly left of the text.
const TOOLBAR_GLYPH_ALIGN_PX: f32 = super::WYSIWYG_BLOCK_PADDING_X - 4. + 2.;

fn separator(cx: &App) -> impl IntoElement {
    div().w_px().h_4().bg(cx.theme().border)
}

impl WysiwygDescription {
    fn format_button(
        id: &'static str,
        icon: ExpIcon,
        tooltip: &'static str,
        active: bool,
        command: FormatCommand,
        cx: &mut Context<Self>,
    ) -> Button {
        Button::new(id)
            .ghost()
            .xsmall()
            .icon(Icon::from(icon))
            .tooltip(tooltip)
            .selected(active)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.apply_format(command.clone(), window, cx);
            }))
    }

    pub(super) fn render_toolbar(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let state = self.format_state(window, cx);
        let has_link = state.link.is_some();
        let link_input = self.link_input.as_ref().map(|(input, _)| input.clone());
        Some(
            h_flex()
                .flex_wrap()
                .gap_0p5()
                .py_0p5()
                .px(gpui::px(TOOLBAR_GLYPH_ALIGN_PX))
                .child(Self::format_button(
                    "wysiwyg-h1",
                    registry::EDITOR_HEADING_1,
                    "Heading 1",
                    state.heading == Some(1),
                    FormatCommand::Heading(1),
                    cx,
                ))
                .child(Self::format_button(
                    "wysiwyg-h2",
                    registry::EDITOR_HEADING_2,
                    "Heading 2",
                    state.heading == Some(2),
                    FormatCommand::Heading(2),
                    cx,
                ))
                .child(Self::format_button(
                    "wysiwyg-h3",
                    registry::EDITOR_HEADING_3,
                    "Heading 3",
                    state.heading == Some(3),
                    FormatCommand::Heading(3),
                    cx,
                ))
                .child(separator(cx))
                .child(Self::format_button(
                    "wysiwyg-bold",
                    registry::EDITOR_BOLD,
                    "Bold",
                    state.bold,
                    FormatCommand::Bold,
                    cx,
                ))
                .child(Self::format_button(
                    "wysiwyg-italic",
                    registry::EDITOR_ITALIC,
                    "Italic",
                    state.italic,
                    FormatCommand::Italic,
                    cx,
                ))
                .child(Self::format_button(
                    "wysiwyg-strike",
                    registry::EDITOR_STRIKETHROUGH,
                    "Strikethrough",
                    state.strikethrough,
                    FormatCommand::Strikethrough,
                    cx,
                ))
                .child(Self::format_button(
                    "wysiwyg-code",
                    registry::EDITOR_CODE,
                    "Code",
                    state.code,
                    FormatCommand::Code,
                    cx,
                ))
                .child(separator(cx))
                // Link: an inline URL field, never a modal prompt. The
                // selection (or the link under the caret) supplies the anchor
                // text, so one field is enough — web parity with `LinkControl`.
                .map(|row| match link_input {
                    Some(input) => row.child(render_link_editor(input, has_link, cx)),
                    None => row.child(
                        Button::new("wysiwyg-link")
                            .ghost()
                            .xsmall()
                            .icon(Icon::from(registry::EDITOR_LINK))
                            .tooltip("Link")
                            .selected(has_link)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_link_editor(window, cx);
                            })),
                    ),
                })
                .child(Self::format_button(
                    "wysiwyg-quote",
                    registry::EDITOR_QUOTE,
                    "Quote",
                    state.quote,
                    FormatCommand::Quote,
                    cx,
                ))
                .child(separator(cx))
                .child(Self::format_button(
                    "wysiwyg-ul",
                    registry::EDITOR_LIST,
                    "Bullet list",
                    state.bullet_list,
                    FormatCommand::BulletList,
                    cx,
                ))
                .child(Self::format_button(
                    "wysiwyg-ol",
                    registry::EDITOR_LIST_ORDERED,
                    "Numbered list",
                    state.ordered_list,
                    FormatCommand::OrderedList,
                    cx,
                ))
                .child(Self::format_button(
                    "wysiwyg-task",
                    registry::EDITOR_LIST_TODO,
                    "Task list",
                    state.task_list,
                    FormatCommand::TaskList,
                    cx,
                ))
                .child(separator(cx))
                .child(Self::format_button(
                    "wysiwyg-clear",
                    registry::EDITOR_CLEAR_FORMATTING,
                    "Clear formatting",
                    false,
                    FormatCommand::ClearFormatting,
                    cx,
                ))
                .child(separator(cx))
                .child(
                    Button::new("wysiwyg-image")
                        .ghost()
                        .xsmall()
                        .icon(Icon::from(registry::EDITOR_IMAGE))
                        .tooltip("Insert image")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.pick_image(window, cx);
                        })),
                ),
        )
    }
}

fn render_link_editor(
    url_input: Entity<InputState>,
    has_link: bool,
    cx: &mut Context<WysiwygDescription>,
) -> impl IntoElement {
    h_flex()
        .gap_1()
        .items_center()
        .child(div().w_48().child(Input::new(&url_input).xsmall()))
        .child(
            Button::new("wysiwyg-link-apply")
                .ghost()
                .xsmall()
                .icon(Icon::from(registry::UI_CHECK))
                .tooltip("Apply link")
                .on_click(cx.listener(|this, _, window, cx| this.apply_link(window, cx))),
        )
        // Web only offers Remove on an existing link.
        .when(has_link, |row| {
            row.child(
                Button::new("wysiwyg-link-remove")
                    .ghost()
                    .xsmall()
                    .icon(Icon::from(registry::EDITOR_UNLINK))
                    .tooltip("Remove link")
                    .on_click(cx.listener(|this, _, window, cx| this.remove_link(window, cx))),
            )
        })
}

/// Toolbar-driven state, kept on the view so `render_toolbar` stays a pure
/// projection of it.
impl WysiwygDescription {
    fn apply_format(
        &mut self,
        command: FormatCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor
            .update(cx, |editor, cx| editor.apply_format(command, window, cx));
        // Toolbar commands are real edits with no blur to ride on for the
        // active-state refresh; the editor's own Changed event handles saving.
        cx.notify();
    }

    fn format_state(&self, window: &Window, cx: &App) -> FormatState {
        self.editor.read(cx).format_state(window, cx)
    }
}
