//! Server-side window frames (#EDE-16, Tier A): a title bar the compositor
//! draws itself above every window that asks for server-side decorations
//! (xdg-decoration) or is an X11 window without its own (Motif hints), so
//! apps without client-side decorations still have a title, the IDE's
//! window controls (minimize, maximize or restore, close), a draggable bar
//! and resizable edges. One rule (#EDE-65): every toplevel has exactly one
//! title bar. A client that draws its own (GTK 4 and libadwaita without
//! xdg-decoration, gpui's client-side windows asking `client_side`) keeps
//! it; one that asks for or leaves the choice to us gets this one. No gpui,
//! colour tokens only ([`crate::text::colour`]), never agent-editable.
//! Behaviour follows Cinnamon's (double-click maximizes, the frame stays
//! when maximized, none when fullscreen); no code is shared with it.

use std::cell::RefCell;

use smithay::{
    backend::renderer::{element::texture::TextureRenderElement, ImportMem, Renderer},
    desktop::Window,
    utils::{Logical, Point, Rectangle},
};

use crate::{
    grabs::resize::ResizeEdge,
    state::{window_state, Ede},
    text::{self, colour, Canvas, TextureCache},
};

/// The title bar's height in logical px.
pub const HEIGHT: i32 = 36;
/// The invisible resize band around a decorated window.
pub const EDGE: i32 = 6;
const BUTTON: i32 = 28;
const RADIUS: f32 = 10.0;
const TITLE_SIZE: f32 = 14.0;

/// What is under the pointer on a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Title,
    Close,
    Maximize,
    Minimize,
    Edge(ResizeEdge),
}


impl Ede {
    /// Does `window` get a compositor-drawn frame?
    pub fn is_decorated(&self, window: &Window) -> bool {
        let st = window_state(window);
        st.decorated && st.fullscreen.is_none()
    }

    pub fn frame_height(&self, window: &Window) -> i32 {
        if self.is_decorated(window) {
            HEIGHT
        } else {
            0
        }
    }

    /// The title bar's rectangle in space coordinates.
    pub fn frame_rect(&self, window: &Window) -> Option<Rectangle<i32, Logical>> {
        if !self.is_decorated(window) {
            return None;
        }
        let geo = self.space.element_geometry(window)?;
        Some(Rectangle::new((geo.loc.x, geo.loc.y - HEIGHT).into(), (geo.size.w, HEIGHT).into()))
    }

    /// The window's rectangle with its frame (what tiling fills).
    pub fn framed_geometry(&self, window: &Window) -> Option<Rectangle<i32, Logical>> {
        let geo = self.space.element_geometry(window)?;
        let fh = self.frame_height(window);
        Some(Rectangle::new((geo.loc.x, geo.loc.y - fh).into(), (geo.size.w, geo.size.h + fh).into()))
    }

    /// The decorated window and the frame part under `pos`, topmost first.
    pub fn frame_under(&self, pos: Point<f64, Logical>) -> Option<(Window, Hit)> {
        if self.is_locked() {
            return None;
        }
        let p: Point<i32, Logical> = (pos.x.floor() as i32, pos.y.floor() as i32).into();
        for w in self.space.elements().rev() {
            if !self.is_decorated(w) {
                // A window without our frame above a decorated one takes
                // the press itself.
                if self.space.element_geometry(w).is_some_and(|g| g.contains(p)) {
                    return None;
                }
                continue;
            }
            let Some(framed) = self.framed_geometry(w) else { continue };
            let bar = Rectangle::new(framed.loc, (framed.size.w, HEIGHT).into());
            if bar.contains(p) {
                return Some((w.clone(), bar_hit(framed.loc.x + framed.size.w - p.x)));
            }
            let mut band = framed;
            band.loc -= (EDGE, EDGE).into();
            band.size += (2 * EDGE, 2 * EDGE).into();
            if band.contains(p) && !framed.contains(p) && window_state(w).tile.is_none() {
                let mut edges = ResizeEdge::empty();
                if p.x < framed.loc.x {
                    edges |= ResizeEdge::LEFT;
                }
                if p.x >= framed.loc.x + framed.size.w {
                    edges |= ResizeEdge::RIGHT;
                }
                if p.y < framed.loc.y {
                    edges |= ResizeEdge::TOP;
                }
                if p.y >= framed.loc.y + framed.size.h {
                    edges |= ResizeEdge::BOTTOM;
                }
                return Some((w.clone(), Hit::Edge(edges)));
            }
            // Inside the window itself: its own surface takes it.
            if framed.contains(p) {
                return None;
            }
        }
        None
    }

    /// The frame's render element at `loc` (output-local, logical), if any.
    pub fn frame_elements<R>(&self, renderer: &mut R, window: &Window, loc: Point<i32, Logical>, scale: f64, focused: bool, redact: bool) -> Vec<TextureRenderElement<R::TextureId>>
    where
        R: Renderer + ImportMem,
        R::TextureId: Clone + 'static,
    {
        if !self.is_decorated(window) {
            return vec![];
        }
        let width = window.geometry().size.w;
        if width <= 0 {
            return vec![];
        }
        let title = if redact { String::new() } else { crate::ipc::window_title_of(window) };
        let cache = window.user_data().get_or_insert(|| RefCell::new(TextureCache::default()));
        let maximized = window_state(window).tile == Some(crate::layout::Tile::Maximized);
        let key = text::key_of(&(&title, width, focused, maximized, scale.to_bits()));
        let bar_top: Point<i32, Logical> = (loc.x, loc.y - HEIGHT).into();
        let at = bar_top.to_f64().to_physical(scale);
        let element = cache.borrow_mut().element(renderer, key, scale.round().max(1.0) as i32, at, || draw_bar(&title, width, focused, maximized, scale));
        element.into_iter().collect()
    }
}

/// The button under a press `from_right` px left of the bar's right edge.
fn bar_hit(from_right: i32) -> Hit {
    match (from_right - 4).max(0) / BUTTON {
        0 => Hit::Close,
        1 => Hit::Maximize,
        2 => Hit::Minimize,
        _ => Hit::Title,
    }
}

/// The bar: card background with rounded top corners, a hairline below,
/// the title (foreground when focused, muted when not) and the IDE's
/// window controls from the right: close, maximize (restore when
/// maximized), minimize.
fn draw_bar(title: &str, width: i32, focused: bool, maximized: bool, scale: f64) -> Canvas {
    let s = scale.max(0.5) as f32;
    let (w, h) = (((width as f32) * s).round() as u32, ((HEIGHT as f32) * s).round() as u32);
    let mut c = Canvas::new(w.max(1), h.max(1));
    c.fill_rounded_top(w as i32, h as i32, RADIUS * s, colour::CARD);
    c.fill_rect(0, h as i32 - 1, w as i32, 1, colour::BORDER_ON_CARD);
    let fg = if focused { colour::FOREGROUND } else { colour::MUTED_FOREGROUND };
    // Buttons from the right: close, maximize, minimize.
    let btn = BUTTON as f32 * s;
    let close_x = w as f32 - btn - 4.0 * s;
    let max_x = close_x - btn;
    let min_x = max_x - btn;
    let cy = h as f32 / 2.0;
    let glyph = 4.5 * s;
    let thick = 1.5 * s;
    let cx = close_x + btn / 2.0;
    c.line(cx - glyph, cy - glyph, cx + glyph, cy + glyph, thick, fg);
    c.line(cx - glyph, cy + glyph, cx + glyph, cy - glyph, thick, fg);
    let mx = max_x + btn / 2.0;
    let square = |c: &mut Canvas, x: f32, y: f32, r: f32| {
        c.line(x - r, y - r, x + r, y - r, thick, fg);
        c.line(x + r, y - r, x + r, y + r, thick, fg);
        c.line(x + r, y + r, x - r, y + r, thick, fg);
        c.line(x - r, y + r, x - r, y - r, thick, fg);
    };
    if maximized {
        // Restore: two offset squares, the back one showing its top-right.
        let r = glyph * 0.8;
        let d = glyph * 0.4;
        c.line(mx - r + d, cy - r - d, mx + r + d, cy - r - d, thick, fg);
        c.line(mx + r + d, cy - r - d, mx + r + d, cy + r - d, thick, fg);
        square(&mut c, mx - d, cy + d, r);
    } else {
        square(&mut c, mx, cy, glyph);
    }
    let nx = min_x + btn / 2.0;
    c.line(nx - glyph, cy, nx + glyph, cy, thick, fg);
    // The title, centred in what the buttons leave.
    let size = TITLE_SIZE * s;
    let avail = (min_x - 12.0 * s * 2.0).max(0.0);
    let fitted = text::fit(title, size, avail);
    let tw = text::measure(&fitted, size);
    let x = ((min_x - tw) / 2.0).max(12.0 * s);
    let baseline = cy + text::ascent(size) / 2.0 - 1.0 * s;
    text::draw(&mut c, x, baseline, &fitted, size, fg);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_draws_at_any_scale() {
        for scale in [1.0, 1.5, 2.0] {
            for maximized in [false, true] {
                let c = draw_bar("Terminal", 400, true, maximized, scale);
                assert_eq!(c.height as f64, (HEIGHT as f64 * scale).round());
            }
        }
        let _ = draw_bar("", 1, false, false, 1.0);
    }

    #[test]
    fn the_controls_read_close_maximize_minimize_from_the_right() {
        assert_eq!(bar_hit(0), Hit::Close);
        assert_eq!(bar_hit(BUTTON + 3), Hit::Close);
        assert_eq!(bar_hit(BUTTON + 4), Hit::Maximize);
        assert_eq!(bar_hit(2 * BUTTON + 4), Hit::Minimize);
        assert_eq!(bar_hit(3 * BUTTON + 4), Hit::Title);
    }
}
