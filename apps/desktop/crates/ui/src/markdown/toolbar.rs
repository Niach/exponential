//! The static formatting toolbar pinned above the editor (masterplan-v3
//! §4.5: bold/italic/strike/code, H1–H3, bullet/ordered/task, quote, link,
//! image — **no selection-bubble / floating popover**, the EXP-3 decision the
//! desktop inherits). Button layout mirrors the web `StaticToolbar`
//! (`apps/web/src/components/issue-editor/markdown-editor.tsx`).
//!
//! The pure source transformations live here (unit-tested); the buttons call
//! into [`MarkdownEditor`] methods that apply them to the focused block's
//! `InputState`.

use gpui::prelude::FluentBuilder as _;
use gpui::{div, App, IntoElement, ParentElement as _, Styled as _};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    h_flex, ActiveTheme as _, Icon, Sizable as _,
};
use std::ops::Range;

use crate::ExpIcon;

use super::editor::MarkdownEditor;

/// A line-level formatting toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LinePrefix {
    Heading(u8),
    Bullet,
    Ordered,
    Task,
    Quote,
}

// ---------------------------------------------------------------------------
// Pure source transformations
// ---------------------------------------------------------------------------

/// Wrap/unwrap `range` of `value` with an inline delimiter (`**`, `*`, `~~`,
/// `` ` ``). Returns the new value and the caret byte offset.
pub(super) fn toggle_wrap(value: &str, range: Range<usize>, delim: &str) -> (String, usize) {
    let (start, end) = clamp_range(value, range);
    let selected = &value[start..end];
    let d = delim.len();

    // Selection already wrapped → unwrap.
    if selected.len() >= 2 * d && selected.starts_with(delim) && selected.ends_with(delim) {
        let inner = &selected[d..selected.len() - d];
        let new_value = format!("{}{}{}", &value[..start], inner, &value[end..]);
        return (new_value, start + inner.len());
    }
    // Delimiters just outside the selection → unwrap those.
    if start >= d
        && value[..start].ends_with(delim)
        && value[end..].starts_with(delim)
    {
        let new_value = format!(
            "{}{}{}",
            &value[..start - d],
            selected,
            &value[end + d..]
        );
        return (new_value, start - d + selected.len());
    }
    // Wrap.
    let new_value = format!("{}{delim}{selected}{delim}{}", &value[..start], &value[end..]);
    let caret = if selected.is_empty() {
        start + d // between the delimiters, ready to type
    } else {
        start + d + selected.len() + d
    };
    (new_value, caret)
}

/// Known line prefixes, longest-match first.
fn strip_known_prefix(line: &str) -> (Option<LinePrefix>, &str) {
    for (marker, kind) in [
        ("- [ ] ", LinePrefix::Task),
        ("- [x] ", LinePrefix::Task),
        ("- [X] ", LinePrefix::Task),
        ("### ", LinePrefix::Heading(3)),
        ("## ", LinePrefix::Heading(2)),
        ("# ", LinePrefix::Heading(1)),
        ("- ", LinePrefix::Bullet),
        ("> ", LinePrefix::Quote),
    ] {
        if let Some(rest) = line.strip_prefix(marker) {
            return (Some(kind), rest);
        }
    }
    // Ordered `N. `
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 {
        if let Some(rest) = line[digits..].strip_prefix(". ") {
            return (Some(LinePrefix::Ordered), rest);
        }
    }
    (None, line)
}

/// Toggle a line prefix over every line the `range` touches. Returns the new
/// value and the caret byte offset (end of the last touched line).
pub(super) fn toggle_line_prefix(
    value: &str,
    range: Range<usize>,
    prefix: LinePrefix,
) -> (String, usize) {
    let (start, end) = clamp_range(value, range);
    let line_start = value[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = value[end..]
        .find('\n')
        .map(|i| end + i)
        .unwrap_or(value.len());

    let region = &value[line_start..line_end];
    let mut out_lines: Vec<String> = Vec::new();
    let mut ordinal = 1u32;
    for line in region.split('\n') {
        // Keep list indentation for nested items.
        let indent_len = line.len() - line.trim_start_matches(' ').len();
        let (indent, body) = line.split_at(indent_len);
        let (existing, rest) = strip_known_prefix(body);
        let new_line = if existing == Some(prefix) {
            // Toggle off.
            format!("{indent}{rest}")
        } else {
            let marker = match prefix {
                LinePrefix::Heading(level) => {
                    format!("{} ", "#".repeat(level.clamp(1, 3) as usize))
                }
                LinePrefix::Bullet => "- ".to_string(),
                LinePrefix::Ordered => {
                    let m = format!("{ordinal}. ");
                    ordinal += 1;
                    m
                }
                LinePrefix::Task => "- [ ] ".to_string(),
                LinePrefix::Quote => "> ".to_string(),
            };
            format!("{indent}{marker}{rest}")
        };
        out_lines.push(new_line);
    }
    let replacement = out_lines.join("\n");
    let new_value = format!(
        "{}{replacement}{}",
        &value[..line_start],
        &value[line_end..]
    );
    (new_value, line_start + replacement.len())
}

/// Strip all inline formatting (bold/italic/strike/code/link syntax) from
/// the selected range — the web toolbar's "Clear formatting". Uses the
/// parser so nesting/edge cases match the contract instead of ad-hoc string
/// surgery.
pub(super) fn strip_inline_formatting(value: &str, range: Range<usize>) -> (String, usize) {
    let (start, end) = clamp_range(value, range);
    if start == end {
        return (value.to_string(), end);
    }
    let fragment = &value[start..end];
    let blocks = super::parse::markdown_to_blocks(fragment);
    let mut plain_parts: Vec<String> = Vec::new();
    for block in &blocks {
        if let super::blocks::ContentBlock::Text { content, .. } = block {
            if !content.text.is_empty() {
                plain_parts.push(content.text.clone());
            }
        }
    }
    let plain = plain_parts.join("\n");
    let new_value = format!("{}{plain}{}", &value[..start], &value[end..]);
    (new_value, start + plain.len())
}

fn clamp_range(value: &str, range: Range<usize>) -> (usize, usize) {
    let mut start = range.start.min(value.len());
    let mut end = range.end.min(value.len());
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }
    while start > 0 && !value.is_char_boundary(start) {
        start -= 1;
    }
    while end < value.len() && !value.is_char_boundary(end) {
        end += 1;
    }
    (start, end)
}

// ---------------------------------------------------------------------------
// The toolbar element
// ---------------------------------------------------------------------------

fn toolbar_button(
    id: &'static str,
    icon: ExpIcon,
    tooltip: &'static str,
    cx: &mut gpui::Context<MarkdownEditor>,
    on_click: impl Fn(&mut MarkdownEditor, &mut gpui::Window, &mut gpui::Context<MarkdownEditor>)
        + 'static,
) -> Button {
    Button::new(id)
        .ghost()
        .xsmall()
        .icon(Icon::from(icon))
        .tooltip(tooltip)
        .on_click(cx.listener(move |this, _, window, cx| on_click(this, window, cx)))
}

fn separator(cx: &App) -> impl IntoElement {
    div().w_px().h_4().bg(cx.theme().border)
}

/// Render the static toolbar row. `window` is unused today but kept in the
/// seam for the link popover's focus handling.
pub(super) fn render_toolbar(
    editor: &mut MarkdownEditor,
    cx: &mut gpui::Context<MarkdownEditor>,
) -> impl IntoElement {
    let link_editor = editor.link_editor_inputs();

    h_flex()
        .flex_wrap()
        .gap_0p5()
        .px_1()
        .py_0p5()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(toolbar_button("md-h1", ExpIcon::Heading1, "Heading 1", cx, |this, window, cx| {
            this.apply_line_prefix(LinePrefix::Heading(1), window, cx);
        }))
        .child(toolbar_button("md-h2", ExpIcon::Heading2, "Heading 2", cx, |this, window, cx| {
            this.apply_line_prefix(LinePrefix::Heading(2), window, cx);
        }))
        .child(toolbar_button("md-h3", ExpIcon::Heading3, "Heading 3", cx, |this, window, cx| {
            this.apply_line_prefix(LinePrefix::Heading(3), window, cx);
        }))
        .child(separator(cx))
        .child(toolbar_button("md-bold", ExpIcon::Bold, "Bold", cx, |this, window, cx| {
            this.apply_inline_wrap("**", window, cx);
        }))
        .child(toolbar_button("md-italic", ExpIcon::Italic, "Italic", cx, |this, window, cx| {
            this.apply_inline_wrap("*", window, cx);
        }))
        .child(toolbar_button(
            "md-strike",
            ExpIcon::Strikethrough,
            "Strikethrough",
            cx,
            |this, window, cx| {
                this.apply_inline_wrap("~~", window, cx);
            },
        ))
        .child(toolbar_button("md-code", ExpIcon::Code, "Code", cx, |this, window, cx| {
            this.apply_inline_wrap("`", window, cx);
        }))
        .child(separator(cx))
        .map(|this| {
            // Link control: swaps to an inline URL editor (never a prompt
            // modal — §4.5 "Links").
            if let Some((url_input, text_input)) = link_editor {
                this.child(render_link_editor(url_input, text_input, cx))
            } else {
                this.child(toolbar_button("md-link", ExpIcon::Link, "Link", cx, |this, window, cx| {
                    this.open_link_editor(window, cx);
                }))
            }
        })
        .child(toolbar_button("md-quote", ExpIcon::Quote, "Quote", cx, |this, window, cx| {
            this.apply_line_prefix(LinePrefix::Quote, window, cx);
        }))
        .child(separator(cx))
        .child(toolbar_button("md-ul", ExpIcon::List, "Bullet list", cx, |this, window, cx| {
            this.apply_line_prefix(LinePrefix::Bullet, window, cx);
        }))
        .child(toolbar_button(
            "md-ol",
            ExpIcon::ListOrdered,
            "Numbered list",
            cx,
            |this, window, cx| {
                this.apply_line_prefix(LinePrefix::Ordered, window, cx);
            },
        ))
        .child(toolbar_button(
            "md-task",
            ExpIcon::ListChecks,
            "Task list",
            cx,
            |this, window, cx| {
                this.apply_line_prefix(LinePrefix::Task, window, cx);
            },
        ))
        .child(separator(cx))
        .child(toolbar_button(
            "md-clear",
            ExpIcon::RemoveFormatting,
            "Clear formatting",
            cx,
            |this, window, cx| {
                this.clear_formatting(window, cx);
            },
        ))
        .child(separator(cx))
        .child(toolbar_button(
            "md-image",
            ExpIcon::Image,
            "Insert image",
            cx,
            |this, window, cx| {
                this.pick_image(window, cx);
            },
        ))
        .child(div().flex_1())
        .child({
            let (icon, tip) = if editor.is_preview() {
                (ExpIcon::Pencil, "Write")
            } else {
                (ExpIcon::Eye, "Preview")
            };
            toolbar_button("md-preview", icon, tip, cx, |this, window, cx| {
                this.toggle_preview(window, cx);
            })
        })
}

fn render_link_editor(
    url_input: gpui::Entity<InputState>,
    text_input: gpui::Entity<InputState>,
    cx: &mut gpui::Context<MarkdownEditor>,
) -> impl IntoElement {
    h_flex()
        .gap_1()
        .items_center()
        .child(div().w_48().child(Input::new(&url_input).xsmall()))
        .child(div().w_32().child(Input::new(&text_input).xsmall()))
        .child(
            Button::new("md-link-apply")
                .ghost()
                .xsmall()
                .icon(Icon::from(ExpIcon::Check))
                .tooltip("Apply link")
                .on_click(cx.listener(|this, _, window, cx| this.apply_link(window, cx))),
        )
        .child(
            Button::new("md-link-cancel")
                .ghost()
                .xsmall()
                .icon(Icon::from(ExpIcon::Unlink))
                .tooltip("Cancel")
                .on_click(cx.listener(|this, _, window, cx| this.close_link_editor(window, cx))),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_empty_selection_places_caret_inside() {
        let (value, caret) = toggle_wrap("hello ", 6..6, "**");
        assert_eq!(value, "hello ****");
        assert_eq!(caret, 8);
    }

    #[test]
    fn wrap_selection() {
        let (value, caret) = toggle_wrap("make bold now", 5..9, "**");
        assert_eq!(value, "make **bold** now");
        assert_eq!(caret, 13);
    }

    #[test]
    fn unwrap_wrapped_selection() {
        let (value, _) = toggle_wrap("make **bold** now", 5..13, "**");
        assert_eq!(value, "make bold now");
    }

    #[test]
    fn unwrap_when_delims_surround_selection() {
        let (value, _) = toggle_wrap("make **bold** now", 7..11, "**");
        assert_eq!(value, "make bold now");
    }

    #[test]
    fn heading_toggles_on_and_off() {
        let (value, _) = toggle_line_prefix("title", 0..0, LinePrefix::Heading(2));
        assert_eq!(value, "## title");
        let (value, _) = toggle_line_prefix(&value, 3..3, LinePrefix::Heading(2));
        assert_eq!(value, "title");
    }

    #[test]
    fn heading_replaces_other_heading() {
        let (value, _) = toggle_line_prefix("# title", 2..2, LinePrefix::Heading(3));
        assert_eq!(value, "### title");
    }

    #[test]
    fn ordered_numbers_selected_lines() {
        let (value, _) = toggle_line_prefix("a\nb\nc", 0..5, LinePrefix::Ordered);
        assert_eq!(value, "1. a\n2. b\n3. c");
    }

    #[test]
    fn bullet_toggles_to_task() {
        let (value, _) = toggle_line_prefix("- item", 3..3, LinePrefix::Task);
        assert_eq!(value, "- [ ] item");
    }

    #[test]
    fn task_preserves_nesting_indent() {
        let (value, _) = toggle_line_prefix("  - child", 4..4, LinePrefix::Task);
        assert_eq!(value, "  - [ ] child");
    }

    #[test]
    fn strip_formatting_flattens_marks() {
        let (value, caret) = strip_inline_formatting("keep **bold** and `code`", 5..24);
        assert_eq!(value, "keep bold and code");
        assert_eq!(caret, 18);
    }
}
