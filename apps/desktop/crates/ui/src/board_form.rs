//! Shared board form pieces (EXP-288): the icon picker over the curated
//! contract glyphs and the color swatch grid, used by the create-board
//! dialog, the per-board settings page, the action editor and the
//! start-coding action inputs. Callback-style so each host owns its own state
//! (dialog draft vs immediate `boards.update`).

use gpui::{
    div, px, App, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{
    button::{Button, ButtonVariants as _},
    h_flex,
    popover::Popover,
    v_flex, ActiveTheme as _, Icon, Sizable as _,
};

use crate::icons::registry;

/// EXP-575: THE icon picker — one slim outline swatch showing the current
/// pick that opens the curated grid in a popover, so the 60-glyph grid never
/// sits inline in a form. `allows_none` adds a "No icon" reset (optional
/// action inputs) and reports `None`; the swatch then shows a dashed
/// placeholder. Byte-for-byte the same shape as web `IconPicker`, iOS
/// `IconPicker` and Android `IconPicker`.
pub(crate) fn icon_picker(
    id_prefix: impl Into<SharedString>,
    selected: Option<&str>,
    allows_none: bool,
    on_pick: impl Fn(Option<&'static str>, &mut Window, &mut App) + Clone + 'static,
    cx: &App,
) -> impl IntoElement {
    let id_prefix: SharedString = id_prefix.into();
    let selected: SharedString = selected.unwrap_or_default().to_string().into();
    let has_pick = !selected.is_empty();
    let glyph = if has_pick {
        crate::icons::board_icon_name_glyph(&selected)
    } else {
        Icon::from(registry::UI_ICON_PLACEHOLDER).text_color(cx.theme().muted_foreground)
    };
    let mut trigger = Button::new(SharedString::from(format!("{id_prefix}-icon-trigger")))
        .outline()
        .cursor_pointer()
        .size(px(36.))
        .icon(glyph);
    if !has_pick {
        trigger = trigger.border_dashed();
    }
    Popover::new(SharedString::from(format!("{id_prefix}-icon-popover")))
        .trigger(trigger)
        .content(move |_, _, cx| {
            let popover = cx.entity();
            let id_prefix = id_prefix.clone();
            let selected = selected.clone();
            let on_pick = on_pick.clone();
            // The grid only wraps inside a DEFINITE width — a popover's
            // content box is unconstrained. 8 × 28px cells + 7 gaps.
            let mut content = v_flex().w(px(266.)).p_1().gap_1();
            if allows_none && has_pick {
                let on_pick = on_pick.clone();
                let popover = popover.clone();
                content = content.child(
                    Button::new(SharedString::from(format!("{id_prefix}-icon-none")))
                        .ghost()
                        .cursor_pointer()
                        .xsmall()
                        .label("No icon")
                        .on_click(move |_, window, cx| {
                            on_pick(None, window, cx);
                            popover.update(cx, |state, cx| state.dismiss(window, cx));
                        }),
                );
            }
            let grid_prefix = id_prefix.clone();
            content.child(icon_swatch_grid(
                grid_prefix,
                &selected,
                move |name, window, cx| {
                    on_pick(Some(name), window, cx);
                    popover.update(cx, |state, cx| state.dismiss(window, cx));
                },
                cx,
            ))
        })
}

/// Web `LABEL_COLORS` (`lib/label-colors.ts`) — the swatch palette shared by
/// board + label colors (fixed hex literals on web too).
pub(crate) const SWATCH_COLORS: [&str; 20] = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308", "#84cc16", "#22c55e", "#10b981",
    "#14b8a6", "#06b6d4", "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7", "#ec4899",
    "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
];

/// The icon grid inside [`icon_picker`]'s popover: one clickable cell per
/// curated contract glyph (`domain::contract::BOARD_ICON_VALUES`); the
/// selected one carries the primary ring.
fn icon_swatch_grid(
    id_prefix: SharedString,
    selected: &str,
    on_pick: impl Fn(&'static str, &mut Window, &mut App) + Clone + 'static,
    cx: &App,
) -> impl IntoElement {
    let mut grid = h_flex().flex_wrap().gap_1p5();
    for &name in domain::contract::BOARD_ICON_VALUES {
        let is_selected = name == selected;
        let on_pick = on_pick.clone();
        grid = grid.child(
            div()
                .id(SharedString::from(format!("{id_prefix}-icon-{name}")))
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(if is_selected {
                    cx.theme().primary
                } else {
                    cx.theme().border
                })
                .cursor_pointer()
                .child(
                    crate::icons::board_icon_name_glyph(name)
                        .small()
                        .text_color(if is_selected {
                            cx.theme().primary
                        } else {
                            cx.theme().muted_foreground
                        }),
                )
                .on_click(move |_, window, cx| on_pick(name, window, cx)),
        );
    }
    grid
}

/// Web `ColorSwatchGrid`: a wrapping row of rounded-full swatches; the
/// selected one carries a ring (approximated as a padded border ring).
pub(crate) fn color_swatch_grid(
    id_prefix: &'static str,
    selected: &str,
    on_pick: impl Fn(&'static str, &mut Window, &mut App) + Clone + 'static,
    cx: &App,
) -> impl IntoElement {
    let mut grid = h_flex().flex_wrap().gap_1p5();
    for color in SWATCH_COLORS {
        let fill = crate::settings::parse_hex_color(color).unwrap_or(cx.theme().muted_foreground);
        let is_selected = color == selected;
        let on_pick = on_pick.clone();
        grid = grid.child(
            div()
                .id(SharedString::from(format!("{id_prefix}-swatch-{color}")))
                .size(px(24.))
                .rounded_full()
                .p(px(2.))
                .border_1()
                .border_color(if is_selected {
                    cx.theme().foreground
                } else {
                    gpui::transparent_black()
                })
                .cursor_pointer()
                .child(div().size_full().rounded_full().bg(fill))
                .on_click(move |_, window, cx| on_pick(color, window, cx)),
        );
    }
    grid
}
