//! Fields: the text field, the search field, the growing textarea and the
//! checkbox.

use gpui::{
    div, prelude::FluentBuilder as _, px, App, Div, ElementId, Entity, Focusable as _,
    InteractiveElement as _, ParentElement as _, Stateful, Styled, Window,
};
use gpui_component::{
    input::{Input, InputState, TextareaState},
    ActiveTheme as _, Icon, Sizable as _,
};

use super::sizing::WebControl as _;
use crate::chrome::Chrome;

/// EXP-720: the ONE text-field recipe (styleguide `text-field`, web
/// `@exp/ui input.tsx`): card fill under the card stroke, and focus
/// swaps the STROKE to the chrome's `stroke_active` — no ring.
/// gpui-component's `Input` paints its focused state as `theme.ring` (the
/// neutral RING token the web keeps for BUTTON focus-visible halos) plus a
/// halo child, which is why an autofocused dialog field used to wear a
/// bright grey outline no other field carried. The theme has no hook for the
/// focused input stroke alone (`ring` also drives every other focus ring), so
/// the swap happens here: `focus_bordered(false)` mutes the component's own
/// focused style and the active stroke rides the caller refinement, which
/// `Input` replays last. Every `Input` goes through this — construct with
/// it, never `Input::new`.
///
/// EXP-963: the corner is the FIELD rung, `radius_lg` (12) — the web
/// `rounded-lg` input, iOS/Android `GlassTextField`. gpui-component paints
/// every control at `theme.radius` (the row's 10), which left the desktop's
/// fields one step tighter than the other three clients'; the refinement is
/// replayed after the component's own `.rounded(theme.radius)`, so it wins
/// without forking the theme (buttons and rows keep their 10).
pub fn glass_input(state: &Entity<InputState>, window: &Window, cx: &App) -> Input {
    let chrome = Chrome::global(cx);
    let focused = state.focus_handle(cx).is_focused(window);
    let active = chrome.stroke_active;
    Input::new(state)
        .focus_bordered(false)
        .rounded(px(chrome.radius_lg))
        .when(focused, |input| input.border_color(active))
}

/// The two rungs of the search field (EXP-963, web `SearchField size`):
/// `Md` is the stock 36px field, `Sm` the 28px one dense columns use (the
/// diff pane's file filter, a picker's own search row).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchFieldSize {
    Md,
    Sm,
}

/// The `Sm` search field's height: the compact ROW rung (web `h-7`, the
/// sidebar's 28px one-line row — [`crate::controls::flat_row_compact`]),
/// not a control rung: a filter sits over rows of that height and reads as
/// one of them.
pub const SEARCH_FIELD_SM_H: f32 = 28.;

/// EXP-963 — the ONE "filter this list" field, the desktop twin of the web
/// `SearchField` and the natives' `GlassSheetSearchField`: the text field
/// with the search glyph (the chrome's `icons.search`) INSIDE it and a ghost
/// clear circle (`icons.clear`) that appears only once there is something to
/// clear — and puts the caret back in the field, so typing continues. It is
/// a [`glass_input`], not a new box: every chrome decision (fill, hairline,
/// focus stroke, radius) still comes from there, and a host inside a popover
/// still chains `.appearance(false)` to go chrome-less.
///
/// The clear is OURS, not gpui-component's `cleanable(true)`: that one draws
/// an `X` from the component's own icon set, and the registry's clear glyph
/// (the circled cross every other client draws) is a different mark.
pub fn search_field(
    state: &Entity<InputState>,
    size: SearchFieldSize,
    window: &Window,
    cx: &App,
) -> Input {
    use gpui_component::button::{Button, ButtonVariants as _};
    let icons = &Chrome::global(cx).icons;
    let muted = cx.theme().muted_foreground;
    let glyph = match size {
        SearchFieldSize::Md => 16.,
        SearchFieldSize::Sm => 14.,
    };
    let has_text = !state.read(cx).value().is_empty();
    let input = glass_input(state, window, cx).prefix(
        Icon::empty()
            .path(icons.search.clone())
            .size(px(glyph))
            .flex_shrink_0()
            .text_color(muted),
    );
    let input = match size {
        SearchFieldSize::Md => input.web_input(),
        // `Styled::h`, named: `Input` has an inherent `h` of its own that
        // only sizes a MULTI-line editor, and it would shadow the box height.
        SearchFieldSize::Sm => Styled::h(input.small(), px(SEARCH_FIELD_SM_H)),
    };
    if !has_text {
        return input;
    }
    let clear_target = state.clone();
    input.suffix(
        Button::new(("search-field-clear", state.entity_id()))
            .ghost()
            .cursor_pointer()
            .xsmall()
            .tab_stop(false)
            .icon(
                Icon::empty()
                    .path(icons.clear.clone())
                    .size(px(glyph))
                    .text_color(muted),
            )
            .tooltip("Clear search")
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                clear_target.update(cx, |input, cx| input.set_value("", window, cx));
                clear_target.read(cx).focus_handle(cx).focus(window, cx);
            }),
    )
}

/// EXP-698 — the ONE textarea state: a multi-line field GROWS with its
/// content between `min_rows` and `max_rows` instead of standing at a
/// hard-coded pixel height. Every dialog textarea used to pick its own
/// `h(px(72.))` / `h(px(80.))` / `h(px(120.))` / `h(px(180.))`, which is four
/// different fields for one control; the row range is the web/mobile
/// contract (`min-h`/`max-h` in rows) and it is the same number on every
/// client.
///
/// The row range lives on the STATE, not on the element: gpui-component
/// carries the layout mode in `TextareaState` (`auto_grow`), so this is the
/// constructor half. The CHROME half is the theme's — `theme.input` is the
/// glass card stroke since EXP-698, so any `appearance(true)` field is
/// already a glass field and needs nothing here; a field inside a
/// [`crate::controls::glass_group`] row keeps `appearance(false)`, because
/// there the GROUP is the field.
pub fn web_textarea(
    min_rows: usize,
    max_rows: usize,
    window: &mut Window,
    cx: &mut gpui::Context<TextareaState>,
) -> TextareaState {
    TextareaState::new(window, cx).auto_grow(min_rows, max_rows)
}

/// What a [`checkbox`] holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    Unchecked,
    Checked,
    /// A bulk selection that disagrees: the same box with a minus.
    Indeterminate,
}

impl From<bool> for CheckState {
    fn from(checked: bool) -> Self {
        if checked {
            Self::Checked
        } else {
            Self::Unchecked
        }
    }
}

/// The checkbox's box (web `size-4`).
pub const CHECKBOX_PX: f32 = 16.;

/// EXP-970 — the checkbox, the web `Checkbox` twin: the 16px square that
/// holds a TABLE's selection (the issue list's bulk-select column, the
/// rail's issue rows, a checklist item in the description) and nothing else
/// — a picker's multi-select rows draw the `ui-selected`/`ui-unselected`
/// circle pair instead. Glass tokens throughout: the row fill under the
/// STRONG stroke (a 16px unchecked square has to stay legible on the row it
/// sits on), the small rung's corner (`radius_sm`, web `rounded-sm`), and
/// checked = the primary fill under the primary foreground with the chrome's
/// `icons.check` glyph (`icons.minus` for indeterminate). Disabled halves the
/// opacity and drops the pointer. The caller chains `.on_click`.
///
/// gpui-component's `Checkbox` is not used: its box is the theme's input
/// stroke at the crate's radius and its tick is the crate's own icon, none
/// of them the ladder's (the IDE's `only_controls_constructs_checkboxes`).
pub fn checkbox(
    id: impl Into<ElementId>,
    state: CheckState,
    disabled: bool,
    cx: &App,
) -> Stateful<Div> {
    let chrome = Chrome::global(cx);
    let theme = cx.theme();
    let marked = state != CheckState::Unchecked;
    let square = div()
        .id(id)
        .flex_shrink_0()
        .size(px(CHECKBOX_PX))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(chrome.radius_sm))
        .border_1()
        .when(!disabled, |square| square.cursor_pointer())
        .when(disabled, |square| square.opacity(0.5));
    let square = if marked {
        square.bg(theme.primary).border_color(theme.primary)
    } else {
        square
            .bg(chrome.fill_row)
            .border_color(chrome.stroke_strong)
    };
    let glyph = match state {
        CheckState::Unchecked => return square,
        CheckState::Checked => chrome.icons.check.clone(),
        CheckState::Indeterminate => chrome.icons.minus.clone(),
    };
    square.child(
        Icon::empty()
            .path(glyph)
            .size(px(14.))
            .flex_shrink_0()
            .text_color(theme.primary_foreground),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_bool_is_a_two_state_check() {
        use super::CheckState;
        assert_eq!(CheckState::from(true), CheckState::Checked);
        assert_eq!(CheckState::from(false), CheckState::Unchecked);
    }
}
