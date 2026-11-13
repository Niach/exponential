//! EXP-1324 — THE CodeBlock at size sm (shadcn's code-block, radix-nova),
//! the desktop's gpui port off `fixtures/trace.json` `geometry.codeBlock`:
//! a `lg` box with an input-colour hairline over the input colour at
//! `fillAlpha`; the header (the `Output` title, the ghost copy button
//! pulled `headerActionsOutset` into the padding, ui-copy → ui-check for
//! 2 s); mono code that never wraps, scrolls sideways and stops `maxLines`
//! tall. Colours come off the catalog's ONE tokenizer
//! (`eui_core::code::tokenize_code`, `ansi` for command output). The
//! destructive variant: a destructive hairline, destructive text, no fill.
//! #EXP-1804 `follow` (genUI CodeBlock `follow`, a live log): the block
//! sticks to its bottom while the code grows, unless the reader scrolled up
//! (scrolling back onto the end re-arms it) — THE tail rule of the session
//! feed ([`crate::controls::TailFollow`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::{
    div, px, AnyElement, App, ClipboardItem, FontWeight, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
};
use gpui::prelude::FluentBuilder as _;
use gpui_component::{h_flex, v_flex, ActiveTheme as _, Icon};
use steer::trace::geometry::code_block as g;

use crate::icons::registry;

/// How long the copy button shows its check.
const COPIED_FLASH: Duration = Duration::from_secs(2);

thread_local! {
    static COPIED: RefCell<HashMap<SharedString, Instant>> = RefCell::new(HashMap::new());
    /// #EXP-1804: a following block's scroll handle and tail state, by id.
    static FOLLOWS: RefCell<HashMap<SharedString, (gpui::ScrollHandle, crate::controls::TailFollow)>> = RefCell::new(HashMap::new());
}

fn follow_state(id: &SharedString) -> (gpui::ScrollHandle, crate::controls::TailFollow) {
    FOLLOWS.with(|follows| follows.borrow_mut().entry(id.clone()).or_insert_with(|| (gpui::ScrollHandle::new(), Default::default())).clone())
}

fn copied(id: &SharedString) -> bool {
    COPIED.with(|copied| {
        copied
            .borrow()
            .get(id)
            .is_some_and(|at| at.elapsed() < COPIED_FLASH)
    })
}

pub(crate) struct CodeBlockProps {
    pub id: SharedString,
    /// The header title (`Output`); `None` = no header.
    pub title: Option<SharedString>,
    pub code: String,
    /// A catalog `codeLanguage` (`ansi`, `plain`, `rust`, …).
    pub language: &'static str,
    pub destructive: bool,
    /// Draw the copy action in the header.
    pub copyable: bool,
    /// #EXP-1804: stick to the bottom while the code grows (a live log)
    /// unless the reader scrolled up.
    pub follow: bool,
}

/// A token kind's colour (catalog `codeToken`).
fn token_color(kind: &str, destructive: bool, cx: &App) -> Hsla {
    let theme = cx.theme();
    if destructive {
        return theme.danger;
    }
    let hue = |c: theme::Srgb8| c.to_hsla();
    match kind {
        "comment" | "punctuation" | "operator" => theme.muted_foreground,
        "success" | "string" => hue(theme::tokens::avatar::GREEN),
        "error" => theme.danger,
        // Web: warning = `--hljs-number` (iOS, Android: the same orange).
        "number" | "warning" => hue(theme::tokens::avatar::ORANGE),
        "keyword" | "tag" => hue(theme::tokens::avatar::VIOLET),
        "type" => hue(theme::tokens::avatar::TEAL),
        "function" | "property" | "attribute" => hue(theme::tokens::avatar::BLUE),
        _ => theme.foreground,
    }
}

/// The text the copy action copies (escape-free for `ansi`).
pub(crate) fn code_block_text(code: &str, language: &str) -> String {
    eui_core::code::code_text(code, language)
}

pub(crate) fn code_block(props: CodeBlockProps, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let input = theme::tokens::INPUT.to_hsla();
    let border = if props.destructive {
        theme.danger.opacity(g::DESTRUCTIVE_BORDER_ALPHA)
    } else {
        input
    };
    let title_color = if props.destructive { theme.danger } else { theme.foreground };
    let lines = eui_core::code::tokenize_code(&props.code, props.language);
    let has_header = props.title.is_some() || props.copyable;
    let header = has_header.then(|| {
        let id = props.id.clone();
        let text = code_block_text(&props.code, props.language);
        let is_copied = copied(&id);
        let copy = props.copyable.then(|| {
            let id = id.clone();
            crate::controls::ghost_icon_button(
                SharedString::from(format!("{}-copy", props.id)),
                Icon::new(if is_copied { registry::UI_CHECK } else { registry::UI_COPY })
                    .size(px(g::COPY_ICON)),
                cx,
            )
            .size(px(g::COPY_BUTTON))
            .tooltip(if is_copied { steer::trace::copy::COPIED } else { steer::trace::copy::COPY })
            .on_click(move |_, _, cx: &mut App| {
                cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                COPIED.with(|copied| copied.borrow_mut().insert(id.clone(), Instant::now()));
                cx.refresh_windows();
                cx.spawn(async move |cx| {
                    cx.background_executor().timer(COPIED_FLASH).await;
                    let _ = cx.update(|cx| cx.refresh_windows());
                })
                .detach();
            })
        });
        h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .justify_between()
            .pl(px(g::PADDING))
            .pr(px(g::PADDING - g::HEADER_ACTIONS_OUTSET))
            .pt(px(g::PADDING - g::HEADER_ACTIONS_OUTSET))
            .min_h(px(g::COPY_BUTTON + g::PADDING - g::HEADER_ACTIONS_OUTSET))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(g::TITLE_SIZE))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(title_color)
                    .children(props.title.clone()),
            )
            .children(copy)
    });
    let destructive = props.destructive;
    let code_lines = lines.into_iter().map(|tokens| {
        h_flex()
            .flex_shrink_0()
            .h(px(g::LINE_HEIGHT))
            .whitespace_nowrap()
            .children(tokens.into_iter().map(|token| {
                div()
                    .flex_shrink_0()
                    .text_color(token_color(&token.kind, destructive, cx))
                    .child(SharedString::from(token.text))
            }))
            // An empty line still holds its height.
            .child(div().w(px(0.)).child(" "))
    });
    let max_height = g::LINE_HEIGHT * g::MAX_LINES as f32;
    let follow = props.follow.then(|| follow_state(&props.id));
    // The follow's probe: paint-time, once the layout settled the offset
    // and the range (`controls::TailFollow::scroller`'s rule).
    let probe = follow.clone().map(|(handle, tail)| {
        if tail.take_snap() {
            handle.scroll_to_bottom();
        }
        gpui::canvas(
            move |_, window, _| {
                let offset = f32::from(handle.offset().y);
                let max = f32::from(handle.max_offset().y);
                match tail.on_layout(offset, max) {
                    crate::controls::TailLayout::Keep => {}
                    crate::controls::TailLayout::Repin => {
                        handle.scroll_to_bottom();
                        window.refresh();
                    }
                    crate::controls::TailLayout::Released | crate::controls::TailLayout::Rearmed => window.refresh(),
                }
            },
            |_, _: (), _, _| {},
        )
        .absolute()
        .size_0()
    });
    let code = div()
        .id(SharedString::from(format!("{}-code", props.id)))
        .w_full()
        .max_h(px(max_height + g::PADDING + if has_header { g::CONTENT_TOP_WITH_HEADER } else { g::PADDING }))
        .overflow_scroll()
        .pl(px(g::PADDING))
        .pr(px(g::PADDING))
        .pb(px(g::PADDING))
        .pt(px(if has_header { g::CONTENT_TOP_WITH_HEADER } else { g::PADDING }))
        .font_family(theme::terminal::FONT_FAMILY)
        .text_size(px(g::FONT_SIZE))
        .line_height(px(g::LINE_HEIGHT))
        .text_color(if destructive { theme.danger } else { theme.foreground })
        .when_some(follow, |code, (handle, tail)| {
            code.track_scroll(&handle).on_scroll(move |event, window, _| {
                let dy = f32::from(event.delta.pixel_delta(window.line_height()).y);
                if tail.on_wheel(dy) {
                    window.refresh();
                }
            })
        })
        .child(v_flex().min_w_0().children(code_lines));
    v_flex()
        .id(props.id.clone())
        .w_full()
        .min_w_0()
        .rounded(px(theme::tokens::radius::LG))
        .border(px(g::BORDER))
        .border_color(border)
        .when(!destructive, |this| this.bg(input.opacity(g::FILL_ALPHA)))
        .overflow_hidden()
        .relative()
        .children(header)
        .child(code)
        .children(probe)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_takes_the_escape_free_text() {
        let code = "\u{1b}[32mBUILD SUCCESSFUL\u{1b}[0m in 41s";
        assert_eq!(code_block_text(code, "ansi"), "BUILD SUCCESSFUL in 41s");
        assert_eq!(code_block_text("a\r\nb", "plain"), "a\nb");
    }
}
