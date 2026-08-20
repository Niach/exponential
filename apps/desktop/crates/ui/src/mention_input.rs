//! The lightweight mention-capable textarea (masterplan-v3 §4.2 comment
//! composer / §4.6 autocomplete) — a `TextareaState` wrapper carrying the same
//! caret-anchored `@`-member / `#`-issue / `:`-emoji completion the markdown
//! editor has,
//! **without** the block model, toolbar or image path (comments have none on
//! web; this is explicitly "not the heavy block editor").
//!
//! The owner (the timeline) keeps holding the inner `Entity<TextareaState>` for
//! value reads and its own submit subscription; this view only layers the
//! completion overlay + keyboard capture (↑/↓ select, Enter/Tab accept, Esc
//! dismiss) on top of the rendered input.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    canvas, deferred, div, point, px, Bounds, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, SharedString, Styled as _, Subscription,
    TextRun, Window,
};
use gpui_component::input::{self, InputEvent, Textarea, TextareaState};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::markdown::{
    byte_offset_to_position, detect_trigger, exact_emoji, CompletionItem, CompletionSource,
    PendingToken,
};

struct ActiveCompletion {
    token: PendingToken,
    items: Vec<CompletionItem>,
    selected: usize,
}

/// Replace `token` in `input` with `replacement`, optionally followed by the
/// trailing space menu acceptance adds (the emoji auto-commit adds none —
/// the user is still typing the sentence).
fn replace_token(
    input: &Entity<TextareaState>,
    token: &PendingToken,
    replacement: &str,
    trailing_space: bool,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    input.update(cx, |state, cx| {
        let Some((new_value, caret)) =
            replaced_token_text(state.value().as_ref(), token, replacement, trailing_space)
        else {
            return;
        };
        let position = byte_offset_to_position(&new_value, caret);
        state.set_value(new_value, window, cx);
        state.set_cursor_position(position, window, cx);
    });
}

/// The pure half of [`replace_token`]: the resulting value plus the caret's
/// byte offset in it, or `None` when the token no longer fits the value
/// (a race between the keystroke and the acceptance).
fn replaced_token_text(
    value: &str,
    token: &PendingToken,
    replacement: &str,
    trailing_space: bool,
) -> Option<(String, usize)> {
    let start = token.start;
    if start > value.len() || !value.is_char_boundary(start) {
        return None;
    }
    let end = (start + token.token_len()).min(value.len());
    if !value.is_char_boundary(end) {
        return None;
    }
    let tail = if trailing_space { " " } else { "" };
    let new_value = format!("{}{replacement}{tail}{}", &value[..start], &value[end..]);
    Some((new_value, start + replacement.len() + tail.len()))
}

/// A multi-line input with the §4.6 autocomplete layered on. Build with the
/// owner's `TextareaState`; call [`MentionInput::set_source`] with a
/// team-scoped [`crate::markdown::store_completion_source`].
pub struct MentionInput {
    input: Entity<TextareaState>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    source: Option<Rc<dyn CompletionSource>>,
    completion: Option<ActiveCompletion>,
    /// EXP-551: a queued `:tada:` auto-commit (token + the unicode replacing
    /// it), drained on the next render.
    pending_commit: Option<(PendingToken, String)>,
    /// EXP-568: draw the textarea's own border/background? The comment
    /// composer turns it OFF — it lives inside a glass card that already owns
    /// the chrome, and a nested box-in-box read as two stacked inputs.
    appearance: bool,
    _subscription: Subscription,
}

impl MentionInput {
    pub fn new(input: Entity<TextareaState>, cx: &mut gpui::Context<Self>) -> Self {
        let subscription = cx.subscribe(&input, |this, input, event: &InputEvent, cx| {
            match event {
                InputEvent::Change => {
                    this.refresh_completion(&input, cx);
                    cx.notify();
                }
                InputEvent::Blur if this.completion.take().is_some() => cx.notify(),
                _ => {}
            }
        });
        Self {
            input,
            bounds: Rc::new(Cell::new(Bounds::default())),
            source: None,
            completion: None,
            pending_commit: None,
            appearance: true,
            _subscription: subscription,
        }
    }

    /// EXP-568: see [`Self::appearance`]. Off = no border, no background —
    /// the host's card supplies both.
    pub fn set_appearance(&mut self, appearance: bool) {
        self.appearance = appearance;
    }

    /// The completion source (re-pointed on issue/team change; `None`
    /// disables the overlay).
    pub fn set_source(&mut self, source: Option<Rc<dyn CompletionSource>>) {
        self.source = source;
        self.completion = None;
        self.pending_commit = None;
    }

    /// EXP-551: insert picked text (the emoji picker's glyph) at the cursor.
    /// `TextareaState::insert` is undo-atomic, so one ⌘Z removes the glyph.
    pub fn insert_text(&mut self, text: &str, window: &mut Window, cx: &mut gpui::Context<Self>) {
        self.input
            .update(cx, |state, cx| state.insert(text.to_string(), window, cx));
        cx.notify();
    }

    fn refresh_completion(&mut self, input: &Entity<TextareaState>, cx: &mut gpui::Context<Self>) {
        let Some(source) = self.source.clone() else {
            self.completion = None;
            return;
        };
        let (value, cursor) = {
            let state = input.read(cx);
            (state.value().to_string(), state.cursor())
        };
        let Some(token) = detect_trigger(&value, cursor) else {
            self.completion = None;
            return;
        };
        // EXP-551: `:tada:` — a CLOSED emoji token whose query is an exact
        // shortcode commits immediately (no menu, no trailing space), so the
        // habit of typing both colons never leaves literal shortcode text.
        if token.closed {
            if let Some(unicode) = exact_emoji(&token.query, cx) {
                self.completion = None;
                self.pending_commit = Some((token, unicode));
                return;
            }
        }
        let items = source.query(token.trigger, &token.query, cx);
        if items.is_empty() {
            self.completion = None;
            return;
        }
        self.completion = Some(ActiveCompletion {
            token,
            items,
            selected: 0,
        });
    }

    fn accept_completion(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(completion) = self.completion.take() else {
            return;
        };
        let Some(item) = completion.items.get(completion.selected).cloned() else {
            return;
        };
        replace_token(&self.input, &completion.token, &item.insert, true, window, cx);
        cx.notify();
    }

    /// EXP-551: run the auto-commit the last `refresh_completion` queued. It
    /// cannot fire inside the `Change` subscription — replacing the value
    /// there would re-enter the same input update — so the render pass that
    /// follows the keystroke drains it.
    fn drain_pending_commit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some((token, unicode)) = self.pending_commit.take() else {
            return;
        };
        replace_token(&self.input, &token, &unicode, false, window, cx);
    }

    fn move_completion(&mut self, delta: isize, cx: &mut gpui::Context<Self>) {
        if let Some(completion) = self.completion.as_mut() {
            let len = completion.items.len() as isize;
            if len > 0 {
                let next = (completion.selected as isize + delta).rem_euclid(len);
                completion.selected = next as usize;
                cx.notify();
            }
        }
    }

    // -- keyboard capture (runs BEFORE the input state's own handlers) --------

    fn on_move_up(&mut self, _: &input::MoveUp, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.completion.is_some() {
            self.move_completion(-1, cx);
            cx.stop_propagation();
        }
    }

    fn on_move_down(&mut self, _: &input::MoveDown, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.completion.is_some() {
            self.move_completion(1, cx);
            cx.stop_propagation();
        }
    }

    fn on_escape(&mut self, _: &input::Escape, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.completion.is_some() {
            self.completion = None;
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn on_enter(&mut self, action: &input::Enter, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.completion.is_some() && !action.shift {
            self.accept_completion(window, cx);
            cx.stop_propagation();
        }
    }

    fn on_tab(
        &mut self,
        _: &input::IndentInline,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.completion.is_some() {
            self.accept_completion(window, cx);
            cx.stop_propagation();
        }
    }

    /// Caret-anchored completion menu (same anchoring math as the markdown
    /// editor's — §4.6 "positioned at the caret pixel, rendered into the
    /// overlay layer").
    fn render_completion(
        &self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let completion = self.completion.as_ref()?;

        let state = self.input.read(cx);
        let value = state.value().to_string();
        let cursor = state.cursor().min(value.len());
        let position = state.cursor_position();
        let line_height = state.line_height().unwrap_or(px(20.));
        let scroll = state.scroll_offset();
        let line_start = value[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_text = value[line_start..cursor].to_string();

        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());
        let caret_x = if line_text.is_empty() {
            px(0.)
        } else {
            let run = TextRun {
                len: line_text.len(),
                font: text_style.font(),
                color: gpui::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(SharedString::from(line_text), font_size, &[run], None)
                .width
        };

        let origin = self.bounds.get().origin;
        let anchor = point(
            origin.x + caret_x + px(8.),
            origin.y + scroll.y + line_height * (position.line as f32 + 1.) + px(8.),
        );

        let items = completion.items.clone();
        let selected = completion.selected;
        let (popover, popover_foreground, border, radius, accent) = {
            let theme = cx.theme();
            (
                theme.popover,
                theme.popover_foreground,
                theme.border,
                theme.radius,
                theme.accent,
            )
        };
        let rows: Vec<gpui::AnyElement> = items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let is_selected = index == selected;
                h_flex()
                    .id(gpui::ElementId::from(("mention-completion-item", index)))
                    .w_full()
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .when(is_selected, |el| el.bg(accent))
                    .hover(move |el| el.bg(accent))
                    .cursor_pointer()
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            if let Some(completion) = this.completion.as_mut() {
                                completion.selected = index;
                            }
                            this.accept_completion(window, cx);
                        }),
                    )
                    // EXP-426: the shared decorated row — avatars for `@`
                    // (this composer offers no `#`).
                    .child(crate::markdown::completion_row_content(&item, cx))
                    .into_any_element()
            })
            .collect();
        let menu = v_flex()
            .id("mention-completion")
            .occlude()
            .min_w(px(260.))
            .max_w(px(380.))
            .p_1()
            .gap_0p5()
            .bg(popover)
            .text_color(popover_foreground)
            .border_1()
            .border_color(border)
            .rounded(radius)
            .shadow_md()
            .children(rows);

        Some(
            deferred(
                gpui::anchored()
                    .position(anchor)
                    .snap_to_window_with_margin(px(8.))
                    .child(menu),
            )
            .with_priority(200)
            .into_any_element(),
        )
    }
}

use gpui::prelude::FluentBuilder as _;

impl Render for MentionInput {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.drain_pending_commit(window, cx);
        let bounds = self.bounds.clone();
        let completion = self.render_completion(window, cx);

        div()
            .key_context("MentionInput")
            .relative()
            .w_full()
            .capture_action(cx.listener(Self::on_move_up))
            .capture_action(cx.listener(Self::on_move_down))
            .capture_action(cx.listener(Self::on_escape))
            .capture_action(cx.listener(Self::on_enter))
            .capture_action(cx.listener(Self::on_tab))
            // `Input` has no intrinsic width (its root is plain flex) and this
            // wrapper is a flex ROW — without an explicit width an empty
            // input collapses to its content (the tiny comment composer,
            // EXP-67).
            .child(Textarea::new(&self.input).w_full().appearance(self.appearance))
            .child(
                canvas(
                    move |element_bounds, _, _| bounds.set(element_bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when_some(completion, |el, completion| el.child(completion))
    }
}

#[cfg(test)]
mod tests {
    //! The composer's completion EDIT is unit-tested here; the end-to-end
    //! trigger→menu→insert path is locked by the WYSIWYG host's `#[gpui::test]`
    //! suite (`wysiwyg/description.rs`), which shares `detect_trigger` and
    //! `exact_emoji`. Drawing a gpui-component text input in a test window is
    //! not possible (the macOS content-type sync wants a real platform
    //! window), so nothing here builds a view.

    use super::*;
    use crate::markdown::CompletionTrigger;

    fn token(trigger: CompletionTrigger, start: usize, query: &str, closed: bool) -> PendingToken {
        PendingToken {
            trigger,
            start,
            query: query.to_string(),
            closed,
        }
    }

    /// EXP-551: accepting an emoji row replaces the whole `:query` token with
    /// the unicode plus the trailing space the `@`/`#` rows add.
    #[test]
    fn accepting_an_emoji_row_replaces_the_token() {
        let value = "ship :tada";
        let token = token(CompletionTrigger::Emoji, 5, "tada", false);
        let (next, caret) = replaced_token_text(value, &token, "🎉", true).expect("edit");
        assert_eq!(next, "ship 🎉 ");
        assert_eq!(&next[..caret], "ship 🎉 ");
    }

    /// EXP-551: the `:tada:` auto-commit eats the CLOSING colon too and adds
    /// no trailing space — the user is mid-sentence.
    #[test]
    fn the_auto_commit_eats_the_closing_colon() {
        let value = "ship :tada:";
        let token = token(CompletionTrigger::Emoji, 5, "tada", true);
        let (next, caret) = replaced_token_text(value, &token, "🎉", false).expect("edit");
        assert_eq!(next, "ship 🎉");
        assert_eq!(caret, next.len());
    }

    /// Text after the caret survives (the token ends at the caret, not at the
    /// end of the line).
    #[test]
    fn trailing_text_survives_the_replacement() {
        let value = "a :tada rest";
        let token = token(CompletionTrigger::Emoji, 2, "tada", false);
        let (next, _) = replaced_token_text(value, &token, "🎉", true).expect("edit");
        assert_eq!(next, "a 🎉  rest");
    }

    /// `@`/`#` tokens keep the old arithmetic (trigger char + query).
    #[test]
    fn mention_tokens_replace_trigger_plus_query() {
        let value = "cc @ja";
        let token = token(CompletionTrigger::Mention, 3, "ja", false);
        let (next, _) =
            replaced_token_text(value, &token, "@jane@example.com", true).expect("edit");
        assert_eq!(next, "cc @jane@example.com ");
    }

    /// A stale token (the value shrank under it) is a no-op, never a panic.
    #[test]
    fn a_stale_token_is_dropped() {
        let stale = token(CompletionTrigger::Emoji, 40, "tada", false);
        assert!(replaced_token_text("short", &stale, "🎉", true).is_none());
        // Mid-multibyte start offsets are refused too.
        let split = token(CompletionTrigger::Emoji, 1, "tada", false);
        assert!(replaced_token_text("é:tada", &split, "🎉", true).is_none());
    }
}
