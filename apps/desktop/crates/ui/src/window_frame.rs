//! Rounded Linux CSD window frame (EXP-269).
//!
//! Vendored adaptation of gpui-component's `window_border.rs` (rev a9a7341,
//! Apache-2.0; itself derived from zed's `window_shadow.rs` example). This
//! file is Apache-2.0, NOT the repository licence — see the `NOTICE` and
//! `LICENSE-APACHE-2.0` files at the root of this crate. Two deliberate
//! deltas from the stock version — which is why we carry a copy at all
//! (upstream hardcodes `BORDER_RADIUS: px(0.0)`):
//!
//! 1. [`FRAME_RADIUS`] is 12px (the glass section radius), and
//! 2. ALL FOUR corners are rounded (stock only rounds the top pair).
//!
//! The stock `Root` frame is disabled (`Root::bordered(false)` +
//! a transparent Root background) and every window's root view wraps its
//! content in [`window_frame`] instead. Non-Linux platforms and the Linux
//! server-decoration fallback render a pass-through. `window_paddings` is NOT
//! copied — the public `gpui_component::window_paddings` stays the source of
//! truth for inset math (it is radius-independent).

use gpui::{
    div, point, prelude::FluentBuilder as _, px, AnyElement, App, Corners, CursorStyle, Decorations,
    Edges, Hsla, InteractiveElement as _, IntoElement, MouseButton, ParentElement, Pixels, Point,
    RenderOnce, ResizeEdge, Size, Styled, Tiling, Window,
};

use gpui_component::ActiveTheme;

#[cfg(not(target_os = "linux"))]
const SHADOW_SIZE: Pixels = px(0.0);
#[cfg(target_os = "linux")]
const SHADOW_SIZE: Pixels = px(12.0);
const BORDER_SIZE: Pixels = px(1.0);
/// Half-width of the resize hit band on each side of the visible frame.
const RESIZE_HIT_SIZE: Pixels = px(4.0);
/// EXP-269: rounded window corners — the glass section radius.
const FRAME_RADIUS: Pixels = px(12.0);

/// Create a new rounded window frame.
pub(crate) fn window_frame() -> WindowFrame {
    WindowFrame::default()
}

/// Renders the custom rounded window border and shadow on Linux CSD;
/// pass-through everywhere else.
#[derive(IntoElement, Default)]
pub(crate) struct WindowFrame {
    children: Vec<AnyElement>,
}

/// The visible frame's per-corner radius — zero on every corner the frame
/// itself renders square (server decorations, or a tiled edge).
///
/// **Every layer that paints to the window EDGE must apply these radii to
/// itself.** gpui's `ContentMask` is a plain rectangle (`ContentMask { bounds }`
/// at the pinned rev), so the `overflow_hidden` on the frame does NOT clip a
/// child's background quad to the rounded corner: the child keeps painting
/// its square corner into the 12px notch outside the arc, and on Linux CSD
/// that reads as an opaque "black corner" sticking out from behind the
/// rounded frame. Only the element that actually paints can round itself —
/// hence this is a shared helper rather than something the frame can enforce.
pub(crate) fn frame_radii(window: &Window) -> Corners<Pixels> {
    let Decorations::Client { tiling } = window.window_decorations() else {
        return Corners::all(px(0.0));
    };
    let radius = |rounded: bool| if rounded { FRAME_RADIUS } else { px(0.0) };
    Corners {
        top_left: radius(!(tiling.top || tiling.left)),
        top_right: radius(!(tiling.top || tiling.right)),
        bottom_right: radius(!(tiling.bottom || tiling.right)),
        bottom_left: radius(!(tiling.bottom || tiling.left)),
    }
}

/// Apply [`frame_radii`] to a layer that paints to ALL FOUR window edges —
/// the full-size page-gradient background every `window_frame()` host puts
/// directly inside the frame. A no-op off Linux CSD (all radii zero).
pub(crate) fn round_to_frame<E: Styled>(el: E, window: &Window) -> E {
    apply_radii(el, frame_radii(window))
}

fn apply_radii<E: Styled>(el: E, radii: Corners<Pixels>) -> E {
    el.rounded_tl(radii.top_left)
        .rounded_tr(radii.top_right)
        .rounded_br(radii.bottom_right)
        .rounded_bl(radii.bottom_left)
}

/// Per-side inset of the visible frame from the outer window bounds.
fn client_frame_insets(shadow_size: Pixels, tiling: &Tiling) -> Edges<Pixels> {
    let mut insets = Edges::all(shadow_size);
    if tiling.top {
        insets.top = px(0.0);
    }
    if tiling.bottom {
        insets.bottom = px(0.0);
    }
    if tiling.left {
        insets.left = px(0.0);
    }
    if tiling.right {
        insets.right = px(0.0);
    }
    insets
}

impl ParentElement for WindowFrame {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for WindowFrame {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let decorations = window.window_decorations();
        // Keep the platform client inset stable even when fully tiled —
        // clearing it makes the first resize after restore double-count the
        // shadow in `compute_outer_size` and the window jumps larger.
        let platform_inset = SHADOW_SIZE;
        let visual_shadow = match decorations {
            Decorations::Client { tiling }
                if tiling.top && tiling.bottom && tiling.left && tiling.right =>
            {
                px(0.0)
            }
            _ => SHADOW_SIZE,
        };
        if matches!(decorations, Decorations::Client { .. }) {
            window.set_client_inset(platform_inset);
        }
        let window_size = window.window_bounds().get_bounds().size;
        let radii = frame_radii(window);

        div()
            .id("window-backdrop")
            .bg(gpui::transparent_black())
            .map(|div| match decorations {
                Decorations::Server => div,
                Decorations::Client { tiling, .. } => apply_radii(
                    div.flex()
                        .flex_col()
                        .overflow_hidden()
                        .bg(gpui::transparent_black()),
                    radii,
                )
                    .when(!tiling.top, |div| div.pt(visual_shadow))
                    .when(!tiling.bottom, |div| div.pb(visual_shadow))
                    .when(!tiling.left, |div| div.pl(visual_shadow))
                    .when(!tiling.right, |div| div.pr(visual_shadow))
                    .on_mouse_down(MouseButton::Left, move |_, window, _| {
                        let Decorations::Client { tiling } = window.window_decorations() else {
                            return;
                        };
                        if tiling.top && tiling.bottom && tiling.left && tiling.right {
                            return;
                        }
                        let size = window.window_bounds().get_bounds().size;
                        let pos = window.mouse_position();
                        let insets = client_frame_insets(platform_inset, &tiling);

                        if let Some(edge) =
                            resize_edge(pos, size, insets, &tiling, RESIZE_HIT_SIZE)
                        {
                            window.start_window_resize(edge)
                        }
                    }),
            })
            .size_full()
            .child(
                div()
                    .cursor(CursorStyle::default())
                    .map(|div| match decorations {
                        Decorations::Server => div.size_full(),
                        Decorations::Client { tiling } => apply_radii(
                            div.flex_1().min_h_0().min_w_0().overflow_hidden(),
                            radii,
                        )
                            .border_color(cx.theme().window_border)
                            .when(!tiling.top, |div| div.border_t(BORDER_SIZE))
                            .when(!tiling.bottom, |div| div.border_b(BORDER_SIZE))
                            .when(!tiling.left, |div| div.border_l(BORDER_SIZE))
                            .when(!tiling.right, |div| div.border_r(BORDER_SIZE))
                            .when(!tiling.is_tiled(), |div| {
                                div.shadow(vec![gpui::BoxShadow {
                                    color: Hsla { h: 0., s: 0., l: 0., a: 0.3 },
                                    blur_radius: visual_shadow / 2.,
                                    spread_radius: px(0.),
                                    offset: point(px(0.0), px(0.0)),
                                    inset: false,
                                }])
                            }),
                    })
                    .on_mouse_move(|_e, _, cx| {
                        cx.stop_propagation();
                    })
                    .bg(gpui::transparent_black())
                    .children(self.children),
            )
            .when(matches!(decorations, Decorations::Client { .. }), |this| {
                let Decorations::Client { tiling, .. } = decorations else {
                    return this;
                };
                this.child(div().absolute().size_full().children(resize_hit_zones(
                    window_size,
                    platform_inset,
                    RESIZE_HIT_SIZE,
                    &tiling,
                )))
            })
    }
}

fn cursor_style_for_resize_edge(edge: ResizeEdge) -> CursorStyle {
    match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    }
}

/// Cursor-only overlay for each resize edge/corner; resize starts from the
/// backdrop `on_mouse_down` via [`resize_edge`].
fn resize_hit_zones(
    window_size: Size<Pixels>,
    shadow_size: Pixels,
    hit_size: Pixels,
    tiling: &Tiling,
) -> Vec<AnyElement> {
    if tiling.top && tiling.bottom && tiling.left && tiling.right {
        return Vec::new();
    }

    let insets = client_frame_insets(shadow_size, tiling);
    let inner_left = insets.left;
    let inner_right = window_size.width - insets.right;
    let inner_top = insets.top;
    let inner_bottom = window_size.height - insets.bottom;
    // Overlay is laid out in the padded content box; convert from window coords.
    let frame_origin = point(insets.left, insets.top);
    let band = hit_size + hit_size;
    let span_x = inner_right - inner_left + band;
    let span_y = inner_bottom - inner_top + band;

    let mut zones: Vec<AnyElement> = Vec::new();

    let mut push_zone = |edge: ResizeEdge, origin: Point<Pixels>, zone_size: Size<Pixels>| {
        let origin = origin - frame_origin;
        zones.push(
            div()
                .absolute()
                .left(origin.x)
                .top(origin.y)
                .w(zone_size.width)
                .h(zone_size.height)
                .cursor(cursor_style_for_resize_edge(edge))
                .into_any_element(),
        );
    };

    if !tiling.top {
        push_zone(
            ResizeEdge::Top,
            point(inner_left - hit_size, inner_top - hit_size),
            Size::new(span_x, band),
        );
    }
    if !tiling.bottom {
        push_zone(
            ResizeEdge::Bottom,
            point(inner_left - hit_size, inner_bottom - hit_size),
            Size::new(span_x, band),
        );
    }
    if !tiling.left {
        push_zone(
            ResizeEdge::Left,
            point(inner_left - hit_size, inner_top - hit_size),
            Size::new(band, span_y),
        );
    }
    if !tiling.right {
        push_zone(
            ResizeEdge::Right,
            point(inner_right - hit_size, inner_top - hit_size),
            Size::new(band, span_y),
        );
    }

    // Corners after edge strips so hit-testing prefers them.
    if !tiling.top && !tiling.left {
        push_zone(
            ResizeEdge::TopLeft,
            point(inner_left - hit_size, inner_top - hit_size),
            Size::new(band, band),
        );
    }
    if !tiling.top && !tiling.right {
        push_zone(
            ResizeEdge::TopRight,
            point(inner_right - hit_size, inner_top - hit_size),
            Size::new(band, band),
        );
    }
    if !tiling.bottom && !tiling.left {
        push_zone(
            ResizeEdge::BottomLeft,
            point(inner_left - hit_size, inner_bottom - hit_size),
            Size::new(band, band),
        );
    }
    if !tiling.bottom && !tiling.right {
        push_zone(
            ResizeEdge::BottomRight,
            point(inner_right - hit_size, inner_bottom - hit_size),
            Size::new(band, band),
        );
    }

    zones
}

/// Hit-test resize edges on a narrow band around the visible inner frame,
/// not the full shadow padding.
fn resize_edge(
    pos: Point<Pixels>,
    size: Size<Pixels>,
    insets: Edges<Pixels>,
    tiling: &Tiling,
    hit_size: Pixels,
) -> Option<ResizeEdge> {
    let inner_left = insets.left;
    let inner_right = size.width - insets.right;
    let inner_top = insets.top;
    let inner_bottom = size.height - insets.bottom;

    let on_left = pos.x >= inner_left - hit_size
        && pos.x <= inner_left + hit_size
        && pos.y >= inner_top - hit_size
        && pos.y <= inner_bottom + hit_size;
    let on_right = pos.x >= inner_right - hit_size
        && pos.x <= inner_right + hit_size
        && pos.y >= inner_top - hit_size
        && pos.y <= inner_bottom + hit_size;
    let on_top = pos.y >= inner_top - hit_size
        && pos.y <= inner_top + hit_size
        && pos.x >= inner_left - hit_size
        && pos.x <= inner_right + hit_size;
    let on_bottom = pos.y >= inner_bottom - hit_size
        && pos.y <= inner_bottom + hit_size
        && pos.x >= inner_left - hit_size
        && pos.x <= inner_right + hit_size;

    if !tiling.top && !tiling.left && on_top && on_left {
        return Some(ResizeEdge::TopLeft);
    }
    if !tiling.top && !tiling.right && on_top && on_right {
        return Some(ResizeEdge::TopRight);
    }
    if !tiling.bottom && !tiling.left && on_bottom && on_left {
        return Some(ResizeEdge::BottomLeft);
    }
    if !tiling.bottom && !tiling.right && on_bottom && on_right {
        return Some(ResizeEdge::BottomRight);
    }
    if !tiling.top && on_top {
        return Some(ResizeEdge::Top);
    }
    if !tiling.bottom && on_bottom {
        return Some(ResizeEdge::Bottom);
    }
    if !tiling.left && on_left {
        return Some(ResizeEdge::Left);
    }
    if !tiling.right && on_right {
        return Some(ResizeEdge::Right);
    }
    None
}
