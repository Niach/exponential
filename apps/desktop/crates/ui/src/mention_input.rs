//! The lightweight mention-capable textarea (masterplan-v3 §4.2 comment
//! composer / §4.6 autocomplete) — an `InputState` wrapper carrying the same
//! caret-anchored `@`-member / `#`-issue completion the markdown editor has,
//! **without** the block model, toolbar or image path (comments have none on
//! web; this is explicitly "not the heavy block editor").
//!
//! The owner (the timeline) keeps holding the inner `Entity<InputState>` for
//! value reads and its own submit subscription; this view only layers the
//! completion overlay + keyboard capture (↑/↓ select, Enter/Tab accept, Esc
//! dismiss) on top of the rendered input.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    canvas, deferred, div, point, px, Bounds, Entity, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, SharedString, Styled as _, Subscription,
    TextRun, Window,
};
use gpui_component::input::{self, Input, InputEvent, InputState};
use gpui_component::{h_flex, v_flex, ActiveTheme as _};

use crate::markdown::{
    byte_offset_to_position, detect_trigger, CompletionItem, CompletionSource, PendingToken,
};

struct ActiveCompletion {
    token: PendingToken,
    items: Vec<CompletionItem>,
    selected: usize,
}

/// A multi-line input with the §4.6 autocomplete layered on. Build with the
/// owner's `InputState`; call [`MentionInput::set_source`] with a
/// team-scoped [`crate::markdown::store_completion_source`].
pub struct MentionInput {
    input: Entity<InputState>,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    source: Option<Rc<dyn CompletionSource>>,
    completion: Option<ActiveCompletion>,
    _subscription: Subscription,
}

impl MentionInput {
    pub fn new(input: Entity<InputState>, cx: &mut gpui::Context<Self>) -> Self {
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
            _subscription: subscription,
        }
    }

    /// The completion source (re-pointed on issue/team change; `None`
    /// disables the overlay).
    pub fn set_source(&mut self, source: Option<Rc<dyn CompletionSource>>) {
        self.source = source;
        self.completion = None;
    }

    fn refresh_completion(&mut self, input: &Entity<InputState>, cx: &mut gpui::Context<Self>) {
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
        self.input.update(cx, |state, cx| {
            let value = state.value().to_string();
            let start = completion.token.start;
            let end = (start + 1 + completion.token.query.len()).min(value.len());
            if start > value.len() {
                return;
            }
            let new_value = format!("{}{} {}", &value[..start], item.insert, &value[end..]);
            let caret = start + item.insert.len() + 1;
            let position = byte_offset_to_position(&new_value, caret);
            state.set_value(new_value, window, cx);
            state.set_cursor_position(position, window, cx);
        });
        cx.notify();
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

    // -- keyboard capture (runs BEFORE the InputState's own handlers) --------

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
    fn render_completion(&self, window: &Window, cx: &gpui::Context<Self>) -> Option<gpui::AnyElement> {
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

        let theme = cx.theme();
        let items = completion.items.clone();
        let selected = completion.selected;
        let menu = v_flex()
            .id("mention-completion")
            .occlude()
            .min_w(px(260.))
            .max_w(px(380.))
            .p_1()
            .gap_0p5()
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .border_1()
            .border_color(theme.border)
            .rounded(px(6.))
            .shadow_md()
            .children(items.iter().enumerate().map(|(index, item)| {
                let is_selected = index == selected;
                h_flex()
                    .id(gpui::ElementId::from(("mention-completion-item", index)))
                    .w_full()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .when(is_selected, |el| el.bg(theme.accent))
                    .hover(|el| el.bg(theme.accent))
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
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .child(item.label.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(item.detail.clone()),
                    )
                    .into_any_element()
            }));

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
            .child(Input::new(&self.input).w_full())
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
