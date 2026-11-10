//! The OVERLAY placement rule every renderer keeps (`src/overlay.ts` in the
//! TS package; `fixtures/overlay-geometry.json` locks it): an anchor box, the
//! overlay's size, the viewport and a preferred side → where it lands.
//! `OVERLAY_OFFSET` px from the anchor on the preferred side, centred on the
//! cross axis; FLIP when the preferred side has less room than needed and the
//! opposite has more; then SHIFT on the cross axis to stay `OVERLAY_PADDING`
//! inside the viewport.

use serde::{Deserialize, Serialize};

pub const OVERLAY_OFFSET: f64 = 4.0;
pub const OVERLAY_PADDING: f64 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlaySide {
    Top,
    Right,
    Bottom,
    Left,
}

impl OverlaySide {
    pub fn parse(s: &str) -> Option<OverlaySide> {
        Some(match s {
            "top" => OverlaySide::Top,
            "right" => OverlaySide::Right,
            "bottom" => OverlaySide::Bottom,
            "left" => OverlaySide::Left,
            _ => return None,
        })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            OverlaySide::Top => "top",
            OverlaySide::Right => "right",
            OverlaySide::Bottom => "bottom",
            OverlaySide::Left => "left",
        }
    }

    fn opposite(self) -> OverlaySide {
        match self {
            OverlaySide::Top => OverlaySide::Bottom,
            OverlaySide::Bottom => OverlaySide::Top,
            OverlaySide::Left => OverlaySide::Right,
            OverlaySide::Right => OverlaySide::Left,
        }
    }

    fn vertical(self) -> bool {
        matches!(self, OverlaySide::Top | OverlaySide::Bottom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OverlayAlign {
    Start,
    #[default]
    Center,
    End,
}

impl OverlayAlign {
    /// VAPP-276: the catalog's `overlayAlign` (`start|center|end`).
    pub fn parse(s: &str) -> Option<OverlayAlign> {
        Some(match s {
            "start" => OverlayAlign::Start,
            "center" => OverlayAlign::Center,
            "end" => OverlayAlign::End,
            _ => return None,
        })
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            OverlayAlign::Start => "start",
            OverlayAlign::Center => "center",
            OverlayAlign::End => "end",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OverlayPlacement {
    pub x: f64,
    pub y: f64,
    pub side: OverlaySide,
    /// True when the preferred side was swapped for its opposite.
    pub flipped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaceOptions {
    pub side: OverlaySide,
    pub align: OverlayAlign,
    pub offset: f64,
    pub padding: f64,
}

impl Default for PlaceOptions {
    fn default() -> Self {
        PlaceOptions { side: OverlaySide::Bottom, align: OverlayAlign::Center, offset: OVERLAY_OFFSET, padding: OVERLAY_PADDING }
    }
}

impl PlaceOptions {
    pub fn side(side: OverlaySide) -> Self {
        PlaceOptions { side, ..Default::default() }
    }

    /// VAPP-276: a side + an alignment along it (`top` + `start` = top-start).
    pub fn placement(side: OverlaySide, align: OverlayAlign) -> Self {
        PlaceOptions { side, align, ..Default::default() }
    }
}

fn room(side: OverlaySide, anchor: &Rect, viewport: &Size, padding: f64) -> f64 {
    match side {
        OverlaySide::Top => anchor.y - padding,
        OverlaySide::Bottom => viewport.height - (anchor.y + anchor.height) - padding,
        OverlaySide::Left => anchor.x - padding,
        OverlaySide::Right => viewport.width - (anchor.x + anchor.width) - padding,
    }
}

fn round3(n: f64) -> f64 {
    (n * 1000.0).round() / 1000.0
}

/// VAPP-281: where an overlay of `size` sits against `anchor` on `side`,
/// `align`ed along it, `offset` away: NO flip and NO shift (a popup hosted
/// OUTSIDE the surface: the host's window system constrains it, e.g. an
/// `xdg_positioner`'s flip/slide). Mirrors `anchorPosition` (TS).
pub fn anchor_position(anchor: &Rect, size: &Size, side: OverlaySide, align: OverlayAlign, offset: f64) -> (f64, f64) {
    let along_x = match align {
        OverlayAlign::Start => anchor.x,
        OverlayAlign::End => anchor.x + anchor.width - size.width,
        OverlayAlign::Center => anchor.x + anchor.width / 2.0 - size.width / 2.0,
    };
    let along_y = match align {
        OverlayAlign::Start => anchor.y,
        OverlayAlign::End => anchor.y + anchor.height - size.height,
        OverlayAlign::Center => anchor.y + anchor.height / 2.0 - size.height / 2.0,
    };
    match side {
        OverlaySide::Top => (along_x, anchor.y - offset - size.height),
        OverlaySide::Bottom => (along_x, anchor.y + anchor.height + offset),
        OverlaySide::Left => (anchor.x - offset - size.width, along_y),
        OverlaySide::Right => (anchor.x + anchor.width + offset, along_y),
    }
}

/// Mirrors `placeOverlay` exactly (the TS rounds to 3 decimals).
pub fn place_overlay(anchor: &Rect, size: &Size, viewport: &Size, options: &PlaceOptions) -> OverlayPlacement {
    let preferred = options.side;
    let offset = options.offset;
    let padding = options.padding;
    let vertical = preferred.vertical();
    let needed = if vertical { size.height } else { size.width } + offset;
    let mut side = preferred;
    let mut flipped = false;
    let preferred_room = room(preferred, anchor, viewport, padding);
    if preferred_room < needed && room(preferred.opposite(), anchor, viewport, padding) > preferred_room {
        side = preferred.opposite();
        flipped = true;
    }
    let (mut x, mut y) = anchor_position(anchor, size, side, options.align, offset);
    if side.vertical() {
        x = x.max(padding).min((viewport.width - padding - size.width).max(padding));
    } else {
        y = y.max(padding).min((viewport.height - padding - size.height).max(padding));
    }
    OverlayPlacement { x: round3(x), y: round3(y), side, flipped }
}

/// #EXP-1765: an overlay whose box reaches `inset` OUTSIDE its content on
/// every side (the IDE Popover: its 1 pt ring outside its content box),
/// placed in gpui-component's Popover order: the inset first (a `start` box
/// begins `inset` before the anchor's start, an `end` one `inset` past its
/// end), then [`place_overlay`]'s flip and margin clamp, then the CONTENT's
/// offset from the anchor's origin rounded to whole points (gpui's
/// `Positioner`, half away from zero). The snap may leave the box up to half
/// a point past the margin, as the IDE's does. Mirrors `placeInsetOverlay`
/// (TS); `overlay-geometry.json` `inset` locks it.
pub fn place_inset_overlay(anchor: &Rect, size: &Size, viewport: &Size, options: &PlaceOptions, inset: f64) -> OverlayPlacement {
    let wide = Rect { x: anchor.x - inset, width: anchor.width + 2.0 * inset, ..*anchor };
    let mut p = place_overlay(&wide, size, viewport, options);
    let snap = |at: f64, origin: f64| round3(origin + (at + inset - origin).round() - inset);
    p.x = snap(p.x, anchor.x);
    p.y = snap(p.y, anchor.y);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flip_happens_only_when_the_opposite_side_has_more_room() {
        let tight = place_overlay(
            &Rect { x: 0.0, y: 290.0, width: 100.0, height: 20.0 },
            &Size { width: 100.0, height: 400.0 },
            &Size { width: 400.0, height: 600.0 },
            &PlaceOptions::side(OverlaySide::Bottom),
        );
        assert!(!tight.flipped);
        assert_eq!(tight.side, OverlaySide::Bottom);
    }
}
