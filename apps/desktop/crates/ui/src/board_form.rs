//! Shared board form pieces (EXP-288): the icon grid over the curated
//! contract glyphs and the color swatch grid, used by BOTH the create-board
//! dialog and the per-board settings page. Callback-style so each host owns
//! its own state (dialog draft vs immediate `boards.update`).

use gpui::{
    div, px, App, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled, Window,
};
use gpui_component::{h_flex, ActiveTheme as _, Sizable as _};

/// Web `LABEL_COLORS` (`lib/label-colors.ts`) — the swatch palette shared by
/// board + label colors (fixed hex literals on web too).
pub(crate) const SWATCH_COLORS: [&str; 20] = [
    "#ef4444", "#dc2626", "#f97316", "#f59e0b", "#eab308", "#84cc16", "#22c55e", "#10b981",
    "#14b8a6", "#06b6d4", "#0ea5e9", "#3b82f6", "#6366f1", "#8b5cf6", "#a855f7", "#ec4899",
    "#f43f5e", "#78716c", "#64748b", "#a3a3a3",
];

/// The icon picker grid: one clickable cell per curated contract glyph
/// (`domain::contract::BOARD_ICON_VALUES`); the selected one carries the
/// primary ring.
pub(crate) fn icon_swatch_grid(
    id_prefix: &'static str,
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
