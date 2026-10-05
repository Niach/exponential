//! EXP-1156 — the ONE dragged column edge: every sidebar column in the IDE
//! (the settings nav, the card's second sidebars — the Inbox / Reviews list
//! and Recent runs, EXP-1192 — and the Files / Source Control screens' own
//! lists) is
//! resized by dragging its RIGHT edge, the width remembered PER PANEL in
//! `ui-prefs.json` ([`crate::ui_prefs::sidebar_width`]), a double-click on the
//! edge resetting it to the panel's default. No hover expansion, no collapse
//! toggle — the web lane's `ResizeHandle` with the same numbers, all of them
//! generated tokens (`theme::tokens::sidebar`).
//!
//! Deliberately NOT gpui-component's `h_resizable` / resizable panel group:
//! EXP-851 removed it from the shell because its panel group resolves under
//! the EXP-492 fit-content passes and collapses (`screens::pinned_panel_root`
//! and the harness in `screens.rs` guard that). A column edge is one width a
//! host owns, so it is a strip, a pure clamp and three listeners:
//!
//! - [`handle`] renders the strip (the host positions it on the edge per
//!   its [`EdgeAnchor`] — EXP-1163: the left column's edge is the content
//!   CARD's left border, a strip as tall as the card); its mouse-down starts a [`ResizeDrag`] on the host,
//!   a double-click resets the width instead;
//! - [`drag_capture`] is the host's per-frame window-level capture while a
//!   drag is live (the pointer leaves an 8px strip at once) — the
//!   `markdown/editor.rs` `ImageResizeDrag` pattern;
//! - every frame reads the width back through [`panel_width`], which CLAMPS
//!   at read time: a shrunk window narrows the column without rewriting what
//!   the reader chose, and widens it back when the window grows.

use gpui::{
    canvas, div, prelude::FluentBuilder as _, px, AnyElement, Context, CursorStyle,
    InteractiveElement as _, IntoElement as _, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Stateful, Styled as _, Div,
};
use theme::tokens::sidebar as tokens;

/// EXP-1156: the columns that remember a width. The pref KEYS are the
/// vocabulary the web client's localStorage shares, so one name means one
/// column on both clients (EXP-1192: the web's `review` key has no IDE
/// column any more — a run's file tree lives in its pane).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SidebarPanel {
    /// The rail (`LeftOccupant::Rail`), fixed at its default.
    Main,
    /// The card's Inbox / Reviews second sidebar (EXP-1192) — the list an
    /// open detail was picked from.
    List,
    /// The settings nav.
    Settings,
    /// The Agent page's Recent runs (a second sidebar in the card).
    Recent,
    /// The Files screen's tree (a list INSIDE a screen, not the left column).
    Files,
    /// The Source Control screen's history list (ditto).
    SourceControl,
}

impl SidebarPanel {
    /// Every column drags but the main menu, which stays as it is.
    pub(crate) fn resizable(self) -> bool {
        self != Self::Main
    }

    #[cfg(test)]
    pub(crate) const ALL: [Self; 6] = [
        Self::Main,
        Self::List,
        Self::Settings,
        Self::Recent,
        Self::Files,
        Self::SourceControl,
    ];

    /// The `ui-prefs.json` key — shared with the web's localStorage.
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::List => "list",
            Self::Settings => "settings",
            Self::Recent => "recent",
            Self::Files => "files",
            Self::SourceControl => "sourceControl",
        }
    }

    /// The width a never-dragged (or reset) panel opens at.
    pub(crate) const fn default_width(self) -> f32 {
        match self {
            Self::Main => tokens::DEFAULT_MAIN,
            Self::List => tokens::DEFAULT_LIST,
            Self::Settings => tokens::DEFAULT_SETTINGS,
            Self::Recent => tokens::DEFAULT_RECENT,
            Self::Files => tokens::DEFAULT_FILES,
            Self::SourceControl => tokens::DEFAULT_SOURCE_CONTROL,
        }
    }
}

/// EXP-1156: the pure clamp. `width` lands in `[MIN_WIDTH, MAX_WIDTH]`, and
/// the column stays at most HALF of `extent` — for the left column (the
/// settings nav) `extent` is the window width; for a column inside the card
/// (a second sidebar, the Files / Source Control lists) it is the SCREEN
/// AREA, so the content beside it always keeps the larger share. EXP-1192:
/// no column sits beside a folded rail any more, so nothing is reserved for
/// one. The half-bound never pushes below `MIN_WIDTH`: a window too narrow
/// for both rules keeps the minimum and lets the content side give.
pub(crate) fn clamp_width(width: f32, extent: f32) -> f32 {
    let max = (extent / 2.)
        .min(tokens::MAX_WIDTH)
        .max(tokens::MIN_WIDTH);
    if !width.is_finite() {
        return tokens::MIN_WIDTH;
    }
    // Whole px: a dragged width is a pref, and `409.99994` is float noise.
    width.round().max(tokens::MIN_WIDTH).min(max.floor())
}

/// EXP-1156: the width `panel` renders at right now — the remembered one, else
/// its default, clamped into `extent` (see [`clamp_width`] for what `extent`
/// is per panel). Read every frame; never written back.
pub(crate) fn panel_width(panel: SidebarPanel, extent: f32) -> f32 {
    // The main menu is FIXED at its default: it has no handle, and a stored
    // `main` (an older build's) is never read.
    if !panel.resizable() {
        return panel.default_width();
    }
    let width = crate::ui_prefs::sidebar_width(panel.key()).unwrap_or(panel.default_width());
    clamp_width(width, extent)
}

/// EXP-1156: one live drag of a column edge, owned by the host view. Kept as
/// the pointer's start and the RENDERED width at mouse-down, so the edge
/// moves by exactly the pointer's travel — it never jumps under the cursor
/// on mouse-down, wherever inside the 8px strip the press landed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ResizeDrag {
    pub panel: SidebarPanel,
    start_x: f32,
    start_width: f32,
    /// The clamp's extent at mouse-down (a window cannot resize mid-drag).
    extent: f32,
    /// False until the pointer first travels: a plain click (or the first
    /// press of a double-click) must not persist the rendered width as a
    /// pref, which would pin today's default for good.
    moved: bool,
}

impl ResizeDrag {
    pub(crate) fn begin(panel: SidebarPanel, pointer_x: f32, extent: f32) -> Self {
        Self {
            panel,
            start_x: pointer_x,
            start_width: panel_width(panel, extent),
            extent,
            moved: false,
        }
    }

    /// The clamped width for a pointer at `pointer_x`, or `None` while the
    /// pointer has not left its press position yet.
    fn track(&mut self, pointer_x: f32) -> Option<f32> {
        if !self.moved && pointer_x == self.start_x {
            return None;
        }
        self.moved = true;
        Some(clamp_width(
            self.start_width + (pointer_x - self.start_x),
            self.extent,
        ))
    }
}

/// EXP-1156: a view that hosts dragged edges keeps ONE `Option<ResizeDrag>`
/// (at most one edge moves at a time) and hands it out here.
pub(crate) trait ResizeHost: Sized + 'static {
    fn resize_drag(&mut self) -> &mut Option<ResizeDrag>;
}

/// EXP-1156: how far a [`EdgeAnchor::Column`] strip reaches INSIDE the
/// column it resizes; the rest of its `HANDLE_WIDTH` lies outside the edge.
/// Not centred, on purpose: a list's slim scrollbar owns an 8px hit strip
/// flush inside its right edge (EXP-1095), and a centred strip would take
/// half of it — the thumb could no longer be grabbed.
pub(crate) const EDGE_INSET: f32 = 1.;

/// EXP-1163: what a strip is pinned to. The host positions the strip at
/// [`EdgeAnchor::strip_left`] of the edge's line pixel and gives it the
/// height of what it drags; the hairline lands ON that pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EdgeAnchor {
    /// The left column's edge IS the main content card's left border: the
    /// strip is centred on that border, as tall as the card (never the
    /// window), and its hairline stops clear of the card's rounded corners.
    /// The `PANEL_MARGIN` gap keeps the strip off the sidebar's scrollbar.
    Card,
    /// A screen's own list (Files, Source Control): the list's right border
    /// beside a plain viewer, no card. [`EDGE_INSET`] in, the rest out, full
    /// height.
    Column,
}

impl EdgeAnchor {
    /// The hairline's x inside the strip.
    pub(crate) fn line_x(self) -> f32 {
        match self {
            Self::Card => tokens::HANDLE_WIDTH / 2.,
            Self::Column => EDGE_INSET,
        }
    }

    /// The strip's left for an edge whose line pixel starts at `edge_x`
    /// (host coordinates).
    pub(crate) fn strip_left(self, edge_x: f32) -> f32 {
        edge_x - self.line_x()
    }

    /// How far the hairline stops short of the strip's top and bottom: the
    /// card's corner radius, so the line runs on the straight border only.
    pub(crate) fn line_inset_y(self) -> f32 {
        match self {
            Self::Card => theme::tokens::radius::LG,
            Self::Column => 0.,
        }
    }
}

/// The hairline's colour: the strongest glass stroke, the same one an active
/// row's outline wears — a quiet line, not an accent.
fn hairline_color() -> gpui::Hsla {
    theme::tokens::glass::STROKE_ACTIVE.to_hsla()
}

/// EXP-1156: the edge strip — `HANDLE_WIDTH` wide, NOT positioned (the host
/// places it at `anchor.strip_left(edge)` and sizes its height, see
/// [`EdgeAnchor`]), with a 1px hairline ON the edge that shows on hover and,
/// `active`, for the whole drag. A left press starts the drag on the host; a
/// double-click (`click_count == 2`) forgets the width instead, back to the
/// panel default. `extent` = the clamp extent the host rendered the column
/// with ([`panel_width`]).
pub(crate) fn handle<V: ResizeHost>(
    panel: SidebarPanel,
    anchor: EdgeAnchor,
    extent: f32,
    active: bool,
    cx: &Context<V>,
) -> Stateful<Div> {
    let group: gpui::SharedString = format!("sidebar-resize-{}", panel.key()).into();
    div()
        .id(gpui::ElementId::Name(group.clone()))
        .group(group.clone())
        .absolute()
        .w(px(tokens::HANDLE_WIDTH))
        .cursor_col_resize()
        .child(
            div()
                .absolute()
                .left(px(anchor.line_x()))
                .top(px(anchor.line_inset_y()))
                .bottom(px(anchor.line_inset_y()))
                .w(px(1.))
                .bg(hairline_color())
                .when(!active, |line| {
                    line.invisible()
                        .group_hover(group.clone(), |style| style.visible())
                }),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this: &mut V, event: &MouseDownEvent, _window, cx| {
                // The press is the edge's: never a row click or a window drag
                // underneath it.
                cx.stop_propagation();
                if event.click_count >= 2 {
                    *this.resize_drag() = None;
                    crate::ui_prefs::set_sidebar_width(panel.key(), None);
                } else {
                    *this.resize_drag() =
                        Some(ResizeDrag::begin(panel, f32::from(event.position.x), extent));
                }
                cx.notify();
            }),
        )
}

/// EXP-1156: the host's capture while `drag` is live — an absolutely
/// positioned canvas whose paint registers window-level move/up listeners
/// for the frame (the pointer is off the strip after one pixel of travel)
/// and pins the column-resize cursor over the whole window. A move writes the
/// clamped width straight into the prefs (in memory at once, on disk
/// debounced) and repaints the host — no transition: a drag is direct
/// manipulation. A move with the button no longer held (the up landed
/// outside the window) ends the drag like an up.
pub(crate) fn drag_capture<V: ResizeHost>(
    drag: Option<ResizeDrag>,
    cx: &Context<V>,
) -> Option<AnyElement> {
    drag?;
    let entity = cx.entity();
    Some(
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.set_window_cursor_style(CursorStyle::ResizeColumn);
                let move_entity = entity.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                    if !phase.bubble() {
                        return;
                    }
                    move_entity.update(cx, |this, cx| {
                        if event.pressed_button != Some(MouseButton::Left) {
                            if this.resize_drag().take().is_some() {
                                cx.notify();
                            }
                            return;
                        }
                        let Some(drag) = this.resize_drag().as_mut() else {
                            return;
                        };
                        let key = drag.panel.key();
                        if let Some(width) = drag.track(f32::from(event.position.x)) {
                            crate::ui_prefs::set_sidebar_width(key, Some(width));
                            cx.notify();
                        }
                    });
                });
                let up_entity = entity.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                    if !phase.bubble() {
                        return;
                    }
                    up_entity.update(cx, |this, cx| {
                        if this.resize_drag().take().is_some() {
                            cx.notify();
                        }
                    });
                });
            },
        )
        .absolute()
        .size_full()
        .into_any_element(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The key / default table IS the shared vocabulary: these strings are
    /// the web's localStorage keys and these numbers the generated tokens.
    #[test]
    fn keys_and_defaults_match_the_shared_table() {
        let table: Vec<(&str, f32)> = SidebarPanel::ALL
            .iter()
            .map(|panel| (panel.key(), panel.default_width()))
            .collect();
        assert_eq!(
            table,
            vec![
                ("main", 272.),
                ("list", 352.),
                ("settings", 272.),
                ("recent", 272.),
                ("files", 320.),
                ("sourceControl", 320.),
            ]
        );
        assert_eq!((tokens::MIN_WIDTH, tokens::MAX_WIDTH), (272., 560.));
        assert_eq!(tokens::HANDLE_WIDTH, 8.);
        // Every default is a width the clamp keeps on a roomy window.
        for panel in SidebarPanel::ALL {
            let width = panel.default_width();
            assert_eq!(clamp_width(width, 2000.), width, "{panel:?}");
        }
    }

    /// The clamp: [MIN, MAX] on a wide extent, the column held to half of
    /// it, and never below MIN however narrow it gets.
    #[test]
    fn the_clamp_bounds_the_column() {
        // Plenty of room: only MIN/MAX bite.
        assert_eq!(clamp_width(100., 2000.), 272.);
        assert_eq!(clamp_width(400., 2000.), 400.);
        assert_eq!(clamp_width(900., 2000.), 560.);
        // 1000px extent: the column stops at half of it.
        assert_eq!(clamp_width(540., 1000.), 500.);
        // Too narrow for half: the minimum holds.
        assert_eq!(clamp_width(400., 500.), 272.);
        // Nonsense never escapes the range.
        assert_eq!(clamp_width(f32::NAN, 2000.), 272.);
        assert_eq!(clamp_width(f32::INFINITY, 2000.), 272.);
    }

    /// The edge moves by the pointer's TRAVEL from the rendered width — no
    /// jump on press — a press that never moves writes nothing, and the
    /// travel is clamped like any other width.
    #[test]
    fn a_drag_follows_the_pointer_travel() {
        let mut drag = ResizeDrag {
            panel: SidebarPanel::List,
            start_x: 400.,
            start_width: 352.,
            extent: 1600.,
            moved: false,
        };
        assert_eq!(drag.track(400.), None, "a press is not a drag");
        assert_eq!(drag.track(430.), Some(382.));
        assert_eq!(drag.track(400.), Some(352.), "back where it began, once moved");
        assert_eq!(drag.track(0.), Some(272.));
        assert_eq!(drag.track(2000.), Some(560.));
        // On a 1000px extent the same drag stops at half of it.
        let mut narrow = ResizeDrag { extent: 1000., ..drag };
        assert_eq!(narrow.track(2000.), Some(500.));
    }

    /// EXP-1163: the left column's strip is CENTRED on the card's left
    /// border (4px out over the gap, 4px in over the card) with its hairline
    /// on the border pixel, clear of the 12px corners; a screen list's strip
    /// keeps 1px in so the list's scrollbar keeps its hit strip.
    #[test]
    fn the_strip_sits_on_its_edge() {
        assert_eq!(EdgeAnchor::Card.line_x(), 4.);
        assert_eq!(EdgeAnchor::Card.strip_left(282.), 278.);
        assert_eq!(
            EdgeAnchor::Card.strip_left(282.) + EdgeAnchor::Card.line_x(),
            282.,
            "the hairline lands on the border"
        );
        assert_eq!(EdgeAnchor::Card.line_inset_y(), 12.);
        assert_eq!(EdgeAnchor::Column.line_x(), 1.);
        assert_eq!(EdgeAnchor::Column.strip_left(319.), 318.);
        assert_eq!(EdgeAnchor::Column.line_inset_y(), 0.);
    }
}
