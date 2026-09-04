//! EXP-568: the FLOATING format rail above a selection in the WYSIWYG
//! description.
//!
//! Replaces the EXP-261 static toolbar (and the EXP-285/417 pinned variants
//! the hosts used to render above their scroll regions): formatting is a
//! selection affordance now, so the rail appears over the selected text and
//! nowhere else. It carries the same [`FormatCommand`] surface the static bar
//! did — the document still mutates through the tested block machinery — but
//! splits it across two pages so a 10-button strip never floats over the
//! text:
//!
//! * **Main** — `#` · link | text-format · lists · quote · code. The
//!   selection-shaped entries only: EXP-587 moved emoji · image · attach to
//!   the static insert bar under the description ([`super::description`]'s
//!   `render_insert_bar`), because inserting over a selection — which is the
//!   only time the rail is up — replaced the selected text.
//! * **Text** — back · Text · H1 · H2 · H3 | bold · italic · strike · clear.
//!   Reached from Main's `a`-glyph and left by it or Escape.
//! * **Link** — not a mode: it is derived from the host's `link_input`, and
//!   replaces the rail body with the URL field while it is up.
//!
//! Main↔Text is a width wipe (EXP-523 motion tokens): the content is pinned
//! at the target width and the chrome's clip morphs around it, so no glyph
//! re-centers mid-flight.
//!
//! EXP-587: the chrome OCCLUDES. It floats over the editor, and gpui hit-tests
//! every hitbox under the pointer unless one blocks the mouse — so a press on
//! a rail button also reached the editor's gap-click / block caret handlers,
//! which collapsed the selection and took the rail down before the button's
//! click ever fired ("Aa just closes the bar").

use gpui::prelude::FluentBuilder as _;
use gpui::{
    anchored, deferred, div, px, Anchor, App, Bounds, Context, Entity, InteractiveElement as _,
    IntoElement, MouseButton, ParentElement as _, Pixels, SharedString, Styled as _, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{InputState},
    popover::Popover,
    v_flex, ActiveTheme as _, Icon, Selectable as _, Sizable as _,
};
use gpui_markdown_editor::{FormatCommand, FormatState};

use super::description::WysiwygDescription;
use crate::icons::registry;
use crate::ExpIcon;
use crate::controls::glass_input;

/// Which page of the rail is showing. Link is deliberately absent — the
/// host's `link_input` already IS that state, and duplicating it would let the
/// two disagree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RailMode {
    Main,
    Text,
}

/// Gap between the selection's top edge and the rail's bottom edge.
const RAIL_GAP_PX: f32 = 6.;

/// Button / separator slots per page — the wipe geometry reads these, so
/// they must track the pages' `items` exactly.
const MAIN_PAGE_SLOTS: (usize, usize) = (6, 1);
const TEXT_PAGE_SLOTS: (usize, usize) = (9, 1);

fn separator(cx: &App) -> impl IntoElement {
    div().w_px().h_4().bg(cx.theme().border)
}

/// Content width of a rail page, derived from the metrics its own layout uses:
/// an xsmall icon button is `size_5` (1.25rem), the row gap is `gap_0p5`
/// (0.125rem) and a separator is a hard 1px. Computed from the live
/// `rem_size` rather than hardcoded pixels — the same idiom the dock and the
/// screens' chip strips use.
fn rail_content_width(buttons: usize, separators: usize, window: &Window) -> f32 {
    let rem = f32::from(window.rem_size());
    let slots = buttons + separators;
    buttons as f32 * 1.25 * rem
        + separators as f32
        + slots.saturating_sub(1) as f32 * 0.125 * rem
}

/// Outer box width for a content width: the chrome's `p_1` (0.25rem a side)
/// plus its 1px border a side (gpui lays out border-box).
fn rail_box_width(content: f32, window: &Window) -> f32 {
    content + 0.5 * f32::from(window.rem_size()) + 2.
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
            .ghost().cursor_pointer()
            .xsmall()
            .icon(Icon::from(icon))
            .tooltip(tooltip)
            .selected(active)
            .on_click(cx.listener(move |this, _, window, cx| {
                this.apply_format(command.clone(), window, cx);
            }))
    }

    pub(super) fn icon_button(
        id: &'static str,
        icon: ExpIcon,
        tooltip: &'static str,
        active: bool,
    ) -> Button {
        Button::new(id)
            .ghost().cursor_pointer()
            .xsmall()
            .icon(Icon::from(icon))
            .tooltip(tooltip)
            .selected(active)
    }

    /// The rail, or `None` while it is not earning its place on screen.
    ///
    /// Visible for a non-empty selection — or while one of its OWN popovers /
    /// the link field holds it open, since those blur the block that anchors
    /// it and it must not vanish under the user's cursor.
    pub(super) fn render_rail(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if !self.rail_visible(window, cx) {
            self.rail_anchor = None;
            return None;
        }
        // Refresh the cache while the geometry is readable; a popover-blurred
        // block reports none, and then the last known anchor stands in.
        if let Some(bounds) = self.editor.read(cx).selection_viewport_bounds(window, cx) {
            self.rail_anchor = Some(bounds);
        }
        let anchor: Bounds<Pixels> = self.rail_anchor?;

        let state = self.format_state(window, cx);
        let body = match (self.link_input.as_ref(), self.rail_mode) {
            (Some((input, _)), _) => self.render_link_page(input.clone(), &state, window, cx),
            (None, RailMode::Main) => self.render_main_page(&state, window, cx),
            (None, RailMode::Text) => self.render_text_page(&state, window, cx),
        };

        Some(
            deferred(
                anchored()
                    // Bottom-left at the selection's top-left: the rail hangs
                    // ABOVE the text it formats, never over it.
                    .anchor(Anchor::BottomLeft)
                    .position(gpui::point(
                        anchor.origin.x,
                        anchor.origin.y - px(RAIL_GAP_PX),
                    ))
                    .snap_to_window_with_margin(px(8.))
                    .child(body),
            )
            .with_priority(1),
        )
    }

    fn rail_visible(&self, window: &Window, cx: &App) -> bool {
        // The lists popover or the link field is up: the rail must survive
        // them even though they have taken focus out of the block that
        // anchors it.
        if self.lists_open || self.link_input.is_some() {
            return true;
        }
        // Otherwise: a selection AND the editor still owning focus. The
        // focus gate is what stops a rail from hovering over an abandoned
        // selection once the user moves to the title or the comment composer
        // — a blurred block keeps its `selected_range`. It cannot fight the
        // rail's own buttons: they `track_focus` inside this view's handle,
        // so pressing one keeps `contains_focused` true.
        self.is_focused(window, cx) && self.editor.read(cx).has_nonempty_selection(window, cx)
    }

    /// EXP-568: the rail just went away — drop the transient pages with it, so
    /// the next selection opens on Main rather than wherever the last one was
    /// left. Called from the editor's event handlers, never from render: a
    /// render that mutates the state it renders from is a frame behind.
    pub(super) fn sync_rail_state(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.rail_visible(window, cx) {
            return;
        }
        self.rail_mode = RailMode::Main;
        self.rail_anim_from = None;
        self.lists_open = false;
        self.rail_anchor = None;
    }

    /// The rail's chrome: a popover-material pill with the page's buttons in
    /// it. `wipe_to` carries the target CONTENT width during a Main↔Text
    /// transition (the children are pinned at it while the chrome's clip
    /// animates), and is `None` in the settled state, where the row simply
    /// takes its natural width.
    fn rail_chrome(
        &self,
        items: Vec<gpui::AnyElement>,
        wipe: Option<(f32, f32, f32)>,
        cx: &App,
    ) -> gpui::AnyElement {
        let theme = cx.theme();
        let chrome = h_flex()
            .id("wysiwyg-rail-chrome")
            .occlude()
            // Belt and braces with `occlude`: a press that lands on the
            // chrome's padding must not travel on to the editor either.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .items_center()
            .gap_0p5()
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .shadow_md();
        let Some((from, to, content)) = wipe else {
            return chrome.children(items).into_any_element();
        };
        gpui_component::animation::EffectTransition::new(theme::motion::FAST)
            .ease(theme::motion::standard())
            .width(px(from), px(to))
            .apply(
                chrome.overflow_hidden().child(
                    h_flex()
                        .flex_none()
                        .items_center()
                        .gap_0p5()
                        .w(px(content))
                        .children(items),
                ),
                gpui::ElementId::NamedInteger("wysiwyg-rail-wipe".into(), self.rail_anim_seq),
            )
            .into_any_element()
    }

    /// The wipe geometry for the page currently rendering, or `None` once it
    /// has settled (see [`Self::set_rail_mode`]).
    fn rail_wipe(
        &self,
        buttons: usize,
        separators: usize,
        window: &Window,
    ) -> Option<(f32, f32, f32)> {
        let from = self.rail_anim_from?;
        let content = rail_content_width(buttons, separators, window);
        let to = rail_box_width(content, window);
        (from != to).then_some((from, to, content))
    }

    fn render_main_page(
        &self,
        state: &FormatState,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let mut items: Vec<gpui::AnyElement> = Vec::new();
        items.push(
            Self::icon_button(
                "wysiwyg-rail-ref",
                registry::EDITOR_ISSUE_REF,
                "Link an issue",
                false,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.insert_issue_ref_trigger(window, cx)
            }))
            .into_any_element(),
        );
        items.push(
            Self::icon_button(
                "wysiwyg-rail-link",
                registry::EDITOR_LINK,
                "Link",
                state.link.is_some(),
            )
            .on_click(cx.listener(|this, _, window, cx| this.open_link_editor(window, cx)))
            .into_any_element(),
        );
        items.push(separator(cx).into_any_element());
        items.push(
            Self::icon_button(
                "wysiwyg-rail-text",
                registry::EDITOR_TEXT_FORMAT,
                "Text formatting",
                false,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.set_rail_mode(RailMode::Text, window, cx)
            }))
            .into_any_element(),
        );
        items.push(self.lists_popover(state, cx).into_any_element());
        items.push(
            Self::format_button(
                "wysiwyg-rail-quote",
                registry::EDITOR_QUOTE,
                "Quote",
                state.quote,
                FormatCommand::Quote,
                cx,
            )
            .into_any_element(),
        );
        items.push(
            Self::format_button(
                "wysiwyg-rail-code",
                registry::EDITOR_CODE,
                "Code",
                state.code,
                FormatCommand::Code,
                cx,
            )
            .into_any_element(),
        );

        let wipe = self.rail_wipe(MAIN_PAGE_SLOTS.0, MAIN_PAGE_SLOTS.1, window);
        self.rail_chrome(items, wipe, cx)
    }

    fn render_text_page(
        &self,
        state: &FormatState,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        // "Text" is the block's resting state: no heading, no list, no quote.
        let plain = state.heading.is_none()
            && !state.bullet_list
            && !state.ordered_list
            && !state.task_list
            && !state.quote;
        let items: Vec<gpui::AnyElement> = vec![
            Self::icon_button("wysiwyg-rail-back", registry::UI_BACK, "Back", false)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.set_rail_mode(RailMode::Main, window, cx)
                }))
                .into_any_element(),
            Self::format_button(
                "wysiwyg-rail-paragraph",
                registry::EDITOR_TEXT,
                "Text",
                plain,
                FormatCommand::Paragraph,
                cx,
            )
            .into_any_element(),
            Self::format_button(
                "wysiwyg-rail-h1",
                registry::EDITOR_HEADING_1,
                "Heading 1",
                state.heading == Some(1),
                FormatCommand::Heading(1),
                cx,
            )
            .into_any_element(),
            Self::format_button(
                "wysiwyg-rail-h2",
                registry::EDITOR_HEADING_2,
                "Heading 2",
                state.heading == Some(2),
                FormatCommand::Heading(2),
                cx,
            )
            .into_any_element(),
            Self::format_button(
                "wysiwyg-rail-h3",
                registry::EDITOR_HEADING_3,
                "Heading 3",
                state.heading == Some(3),
                FormatCommand::Heading(3),
                cx,
            )
            .into_any_element(),
            separator(cx).into_any_element(),
            Self::format_button(
                "wysiwyg-rail-bold",
                registry::EDITOR_BOLD,
                "Bold",
                state.bold,
                FormatCommand::Bold,
                cx,
            )
            .into_any_element(),
            Self::format_button(
                "wysiwyg-rail-italic",
                registry::EDITOR_ITALIC,
                "Italic",
                state.italic,
                FormatCommand::Italic,
                cx,
            )
            .into_any_element(),
            Self::format_button(
                "wysiwyg-rail-strike",
                registry::EDITOR_STRIKETHROUGH,
                "Strikethrough",
                state.strikethrough,
                FormatCommand::Strikethrough,
                cx,
            )
            .into_any_element(),
            Self::format_button(
                "wysiwyg-rail-clear",
                registry::EDITOR_CLEAR_FORMATTING,
                "Clear formatting",
                false,
                FormatCommand::ClearFormatting,
                cx,
            )
            .into_any_element(),
        ];

        let wipe = self.rail_wipe(TEXT_PAGE_SLOTS.0, TEXT_PAGE_SLOTS.1, window);
        self.rail_chrome(items, wipe, cx)
    }

    /// Web parity with `LinkControl`: one URL field (the selection — or the
    /// link run under a bare caret — supplies the anchor text), apply, an
    /// explicit cancel, and Remove only when there IS a link.
    fn render_link_page(
        &self,
        url_input: Entity<InputState>,
        state: &FormatState,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let mut items: Vec<gpui::AnyElement> = vec![
            div()
                .w_48()
                .child(glass_input(&url_input, window, cx).xsmall())
                .into_any_element(),
            Self::icon_button("wysiwyg-rail-link-apply", registry::UI_CHECK, "Apply link", false)
                .on_click(cx.listener(|this, _, window, cx| this.apply_link(window, cx)))
                .into_any_element(),
            Self::icon_button("wysiwyg-rail-link-cancel", registry::UI_CLOSE, "Cancel", false)
                .on_click(cx.listener(|this, _, _window, cx| this.close_link_editor(cx)))
                .into_any_element(),
        ];
        if state.link.is_some() {
            items.push(
                Self::icon_button(
                    "wysiwyg-rail-link-remove",
                    registry::EDITOR_UNLINK,
                    "Remove link",
                    false,
                )
                .on_click(cx.listener(|this, _, window, cx| this.remove_link(window, cx)))
                .into_any_element(),
            );
        }
        // The URL field has no button-grid width, so the link page never
        // wipes — it simply replaces the rail body.
        self.rail_chrome(items, None, cx)
    }

    /// The three list kinds behind one trigger. The trigger wears whichever
    /// kind is active (and lights up), so the resting rail still says which
    /// list the block is in without spending three slots on it.
    fn lists_popover(&self, state: &FormatState, cx: &mut Context<Self>) -> Popover {
        let active = state.bullet_list || state.ordered_list || state.task_list;
        let glyph = if state.ordered_list {
            registry::EDITOR_LIST_ORDERED
        } else if state.task_list {
            registry::EDITOR_LIST_TODO
        } else {
            registry::EDITOR_LIST
        };
        let view = cx.entity();
        let (bullet, ordered, task) = (state.bullet_list, state.ordered_list, state.task_list);
        Popover::new("wysiwyg-rail-lists-popover")
            .open(self.lists_open)
            .p_1()
            .trigger(Self::icon_button(
                "wysiwyg-rail-lists",
                glyph,
                "Lists",
                active,
            ))
            .on_open_change(cx.listener(|this, open: &bool, _window, cx| {
                this.set_lists_open(*open, cx);
            }))
            .content(move |_, _, cx| {
                let hover = cx.theme().muted;
                v_flex()
                    .min_w(px(168.))
                    .child(list_row(
                        "wysiwyg-rail-list-bullet",
                        registry::EDITOR_LIST,
                        "List",
                        bullet,
                        hover,
                        view.clone(),
                        FormatCommand::BulletList,
                    ))
                    .child(list_row(
                        "wysiwyg-rail-list-ordered",
                        registry::EDITOR_LIST_ORDERED,
                        "Numbered list",
                        ordered,
                        hover,
                        view.clone(),
                        FormatCommand::OrderedList,
                    ))
                    .child(list_row(
                        "wysiwyg-rail-list-task",
                        registry::EDITOR_LIST_TODO,
                        "Checklist",
                        task,
                        hover,
                        view.clone(),
                        FormatCommand::TaskList,
                    ))
            })
    }
}

/// One row of the lists popover. Handlers go through the captured view entity
/// rather than `cx.listener` because the popover's content closure is a `Fn`
/// that rebuilds its rows on every open.
fn list_row(
    id: &'static str,
    icon: ExpIcon,
    label: &'static str,
    active: bool,
    hover: gpui::Hsla,
    view: Entity<WysiwygDescription>,
    command: FormatCommand,
) -> impl IntoElement {
    h_flex()
        .id(id)
        .px_2()
        .py_1()
        .gap_2()
        .text_sm()
        .rounded_sm()
        .cursor_pointer()
        .when(active, |row| row.font_weight(gpui::FontWeight::MEDIUM))
        .hover(move |style| style.bg(hover))
        .child(Icon::from(icon).size_4())
        .child(SharedString::from(label))
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            view.update(cx, |this, cx| {
                this.set_lists_open(false, cx);
                this.apply_format(command.clone(), window, cx);
            });
        })
}

/// Rail-driven state, kept on the view so the render functions above stay a
/// pure projection of it.
impl WysiwygDescription {
    pub(super) fn apply_format(
        &mut self,
        command: FormatCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.editor
            .update(cx, |editor, cx| editor.apply_format(command, window, cx));
        // Rail commands are real edits with no blur to ride on for the
        // active-state refresh; the editor's own Changed event handles saving.
        cx.notify();
    }

    fn format_state(&self, window: &Window, cx: &App) -> FormatState {
        self.editor.read(cx).format_state(window, cx)
    }

    pub(super) fn set_lists_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.lists_open != open {
            self.lists_open = open;
            cx.notify();
        }
    }

    /// Swap rail pages, arming the width wipe. The settle pass clears
    /// `rail_anim_from` once the transition is over so the rail returns to its
    /// natural width — a target width computed a pixel or two off must never
    /// become the rail's permanent size.
    pub(super) fn set_rail_mode(
        &mut self,
        mode: RailMode,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.rail_mode == mode {
            return;
        }
        let (buttons, separators) = match self.rail_mode {
            RailMode::Main => MAIN_PAGE_SLOTS,
            RailMode::Text => TEXT_PAGE_SLOTS,
        };
        let leaving = rail_content_width(buttons, separators, window);
        self.rail_anim_from = Some(rail_box_width(leaving, window));
        self.rail_mode = mode;
        self.rail_anim_seq = self.rail_anim_seq.wrapping_add(1);
        let seq = self.rail_anim_seq;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(theme::motion::FAST).await;
            this.update(cx, |this, cx| {
                if this.rail_anim_seq == seq {
                    this.rail_anim_from = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// EXP-568: the `#` entry. `detect_trigger` only fires a token at a line
    /// start or after whitespace (the web `(?<![\w#])` rule), so a `#` glued
    /// to the preceding word would insert dead text instead of opening the
    /// issue menu — the space is what makes the button work.
    pub(super) fn insert_issue_ref_trigger(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Read the boundary off the SOURCE selection: the rail's click has
        // already blurred the block, so the focused-only caret accessor is
        // `None` by the time this runs.
        let editor = self.editor.read(cx);
        let markdown = editor.markdown(cx);
        let start = editor.source_selection(cx).range.start.min(markdown.len());
        let glued = markdown
            .get(..start)
            .and_then(|before| before.chars().next_back())
            .is_some_and(|char| !char.is_whitespace());
        let text = if glued { " #" } else { "#" };
        self.editor
            .update(cx, |editor, cx| editor.insert_text_at_caret(text, window, cx));
        cx.notify();
    }
}
