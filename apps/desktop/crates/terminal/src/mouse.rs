// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! Mouse handling (masterplan-v3 §6.8's mouse subsection): pixel → grid-cell
//! mapping for local selection, and **mouse-mode reporting** to the child
//! when a TUI requests it (`vim`, `htop`).
//!
//! PROVENANCE: the report encodings are written from the xterm
//! control-sequence documentation ("Xterm Control Sequences" — Mouse
//! Tracking: Normal / UTF-8 / SGR extended modes). Zed's GPL
//! `mappings/mouse.rs` was not read while writing this file.
//!
//! Encodings implemented:
//! - **SGR (1006)** — `CSI < Cb ; Cx ; Cy M` (press/motion) or `m` (release),
//!   1-based cell coordinates, no coordinate limit. Preferred by every modern
//!   TUI; alacritty and xterm enable it via `TermMode::SGR_MOUSE`.
//! - **Normal (X10 + 1000/1002/1003)** — `CSI M Cb+32 Cx+32+1 Cy+32+1` as raw
//!   bytes; coordinates clamp at 223 (255 − 32). With `TermMode::UTF8_MOUSE`
//!   (1005) coordinates > 95 are UTF-8 encoded instead of clamped.
//!
//! Button codes: 0/1/2 = left/middle/right, 3 = release (normal mode only),
//! wheel up/down = 64/65; modifiers add shift=4, alt(meta)=8, ctrl=16; motion
//! adds 32.

use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::term::TermMode;
use gpui::{Modifiers, MouseButton, Pixels, Point as PixelPoint};

/// What kind of mouse transition is being reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventKind {
    Press,
    Release,
    /// Pointer moved (with or without a held button).
    Motion,
}

/// A cell position in *viewport* coordinates (0-based col/row from the
/// top-left of the visible grid) — what mouse reports encode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewportCell {
    pub col: usize,
    pub row: usize,
}

/// Map a window-space pixel position onto the grid.
///
/// Returns the viewport cell (clamped into the grid), the equivalent
/// *buffer*-coordinate [`Point`] (viewport row shifted by the scrollback
/// `display_offset` — what `alacritty_terminal::selection` expects), and the
/// [`Side`] of the cell the pixel landed in (selection anchors care).
pub fn grid_cell(
    position: PixelPoint<Pixels>,
    origin: PixelPoint<Pixels>,
    cell_width: Pixels,
    line_height: Pixels,
    cols: usize,
    rows: usize,
    display_offset: usize,
) -> (ViewportCell, Point, Side) {
    let cols = cols.max(1);
    let rows = rows.max(1);

    let x = f32::from(position.x - origin.x);
    let y = f32::from(position.y - origin.y);
    let cell_w = f32::from(cell_width).max(1.0);
    let line_h = f32::from(line_height).max(1.0);

    let col = ((x / cell_w).floor().max(0.0) as usize).min(cols - 1);
    let row = ((y / line_h).floor().max(0.0) as usize).min(rows - 1);

    let in_cell_x = x - (col as f32) * cell_w;
    let side = if in_cell_x > cell_w / 2.0 {
        Side::Right
    } else {
        Side::Left
    };

    let point = Point::new(Line(row as i32 - display_offset as i32), Column(col));
    (ViewportCell { col, row }, point, side)
}

/// Base report code for a gpui mouse button (`None` for buttons terminals do
/// not report).
pub fn button_code(button: MouseButton) -> Option<u8> {
    match button {
        MouseButton::Left => Some(0),
        MouseButton::Middle => Some(1),
        MouseButton::Right => Some(2),
        _ => None,
    }
}

/// Whether this event should be reported to the child at all, given the
/// terminal's current mode and the local modifiers.
///
/// Shift is the universal local-override: xterm reserves shifted clicks for
/// local selection even when the app grabbed the mouse.
pub fn should_report(
    kind: MouseEventKind,
    button_held: bool,
    shift: bool,
    mode: &TermMode,
) -> bool {
    if shift || !mode.intersects(TermMode::MOUSE_MODE) {
        return false;
    }
    match kind {
        MouseEventKind::Press | MouseEventKind::Release => true,
        MouseEventKind::Motion if button_held => mode
            .intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION),
        MouseEventKind::Motion => mode.contains(TermMode::MOUSE_MOTION),
    }
}

/// Encode one mouse report (`button` = base code from [`button_code`] or
/// 64/65 for wheel). `cell` is the viewport cell. Returns `None` when the
/// coordinates cannot be encoded (normal mode beyond 223 without UTF-8).
pub fn mouse_report(
    button: u8,
    kind: MouseEventKind,
    cell: ViewportCell,
    modifiers: &Modifiers,
    mode: &TermMode,
) -> Option<Vec<u8>> {
    let mut cb = button;
    if modifiers.shift {
        cb += 4;
    }
    if modifiers.alt {
        cb += 8;
    }
    if modifiers.control {
        cb += 16;
    }
    if kind == MouseEventKind::Motion {
        cb += 32;
    }

    if mode.contains(TermMode::SGR_MOUSE) {
        let terminator = if kind == MouseEventKind::Release { 'm' } else { 'M' };
        Some(
            format!("\x1b[<{};{};{}{}", cb, cell.col + 1, cell.row + 1, terminator).into_bytes(),
        )
    } else {
        // Normal encoding: release loses button identity (code 3).
        if kind == MouseEventKind::Release {
            cb = (cb & !0b11) | 3;
            // A wheel "release" is never reported in normal mode.
            if button >= 64 {
                return None;
            }
        }
        let mut bytes = vec![0x1b, b'[', b'M', 32 + cb];
        let utf8 = mode.contains(TermMode::UTF8_MOUSE);
        encode_normal_coord(&mut bytes, cell.col + 1, utf8)?;
        encode_normal_coord(&mut bytes, cell.row + 1, utf8)?;
        Some(bytes)
    }
}

/// Normal-mode coordinate: byte `32 + coord` (1-based), UTF-8 extended when
/// mode 1005 is on. Coordinates a plain byte cannot carry are dropped.
fn encode_normal_coord(out: &mut Vec<u8>, coord_1_based: usize, utf8: bool) -> Option<()> {
    let value = 32 + coord_1_based;
    if value < 128 {
        out.push(value as u8);
        Some(())
    } else if utf8 && value < 2048 {
        // Two-byte UTF-8, exactly how xterm's 1005 mode extends the range.
        out.push(0b1100_0000 | (value >> 6) as u8);
        out.push(0b1000_0000 | (value & 0x3f) as u8);
        Some(())
    } else if !utf8 && value <= 255 {
        out.push(value as u8);
        Some(())
    } else {
        None
    }
}

/// Wheel reports when the child grabbed the mouse: one report per line,
/// button 64 (up) / 65 (down). `lines` > 0 = up (into history direction).
pub fn wheel_reports(
    lines: i32,
    cell: ViewportCell,
    modifiers: &Modifiers,
    mode: &TermMode,
) -> Vec<u8> {
    let button = if lines > 0 { 64 } else { 65 };
    let count = lines.unsigned_abs().min(32) as usize;
    let mut out = Vec::new();
    for _ in 0..count {
        if let Some(report) = mouse_report(button, MouseEventKind::Press, cell, modifiers, mode) {
            out.extend_from_slice(&report);
        }
    }
    out
}

/// Alternate-scroll (xterm mode 1007): in the alt screen with no mouse grab,
/// wheel motion becomes cursor-key presses so pagers/editors scroll. Honors
/// DECCKM for the CSI/SS3 form.
pub fn alt_scroll_reports(lines: i32, mode: &TermMode) -> Vec<u8> {
    let final_byte = if lines > 0 { b'A' } else { b'B' };
    let prefix: &[u8] = if mode.contains(TermMode::APP_CURSOR) {
        b"\x1bO"
    } else {
        b"\x1b["
    };
    let count = lines.unsigned_abs().min(32) as usize;
    let mut out = Vec::with_capacity(count * 3);
    for _ in 0..count {
        out.extend_from_slice(prefix);
        out.push(final_byte);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

    fn cell(col: usize, row: usize) -> ViewportCell {
        ViewportCell { col, row }
    }

    fn no_mods() -> Modifiers {
        Modifiers::default()
    }

    #[test]
    fn grid_cell_maps_pixels_and_clamps() {
        let origin = point(px(10.0), px(20.0));
        let (vc, p, side) = grid_cell(point(px(10.0), px(20.0)), origin, px(8.0), px(16.0), 80, 24, 0);
        assert_eq!((vc.col, vc.row), (0, 0));
        assert_eq!(p, Point::new(Line(0), Column(0)));
        assert_eq!(side, Side::Left);

        // Right half of cell 2, row 1.
        let (vc, _, side) = grid_cell(point(px(10.0 + 21.0), px(20.0 + 17.0)), origin, px(8.0), px(16.0), 80, 24, 0);
        assert_eq!((vc.col, vc.row), (2, 1));
        assert_eq!(side, Side::Right);

        // Out-of-bounds clamps into the grid (negative and beyond).
        let (vc, _, _) = grid_cell(point(px(0.0), px(0.0)), origin, px(8.0), px(16.0), 80, 24, 0);
        assert_eq!((vc.col, vc.row), (0, 0));
        let (vc, _, _) = grid_cell(point(px(9999.0), px(9999.0)), origin, px(8.0), px(16.0), 80, 24, 0);
        assert_eq!((vc.col, vc.row), (79, 23));
    }

    #[test]
    fn grid_cell_shifts_by_display_offset_for_buffer_point() {
        let origin = point(px(0.0), px(0.0));
        // Scrolled back 5 lines: viewport row 2 is buffer line -3.
        let (_, p, _) = grid_cell(point(px(4.0), px(40.0)), origin, px(8.0), px(16.0), 80, 24, 5);
        assert_eq!(p, Point::new(Line(-3), Column(0)));
    }

    #[test]
    fn sgr_press_release_and_motion() {
        let mode = TermMode::SGR_MOUSE | TermMode::MOUSE_REPORT_CLICK;
        assert_eq!(
            mouse_report(0, MouseEventKind::Press, cell(0, 0), &no_mods(), &mode).unwrap(),
            b"\x1b[<0;1;1M"
        );
        assert_eq!(
            mouse_report(0, MouseEventKind::Release, cell(0, 0), &no_mods(), &mode).unwrap(),
            b"\x1b[<0;1;1m"
        );
        assert_eq!(
            mouse_report(2, MouseEventKind::Press, cell(9, 4), &no_mods(), &mode).unwrap(),
            b"\x1b[<2;10;5M"
        );
        // Motion adds 32.
        assert_eq!(
            mouse_report(0, MouseEventKind::Motion, cell(1, 1), &no_mods(), &mode).unwrap(),
            b"\x1b[<32;2;2M"
        );
    }

    #[test]
    fn sgr_modifier_bits() {
        let mode = TermMode::SGR_MOUSE;
        let mods = Modifiers {
            control: true,
            alt: true,
            ..Default::default()
        };
        // 0 + 8 (alt) + 16 (ctrl) = 24.
        assert_eq!(
            mouse_report(0, MouseEventKind::Press, cell(0, 0), &mods, &mode).unwrap(),
            b"\x1b[<24;1;1M"
        );
    }

    #[test]
    fn normal_mode_encoding_and_release_code() {
        let mode = TermMode::MOUSE_REPORT_CLICK;
        // left press at (0,0): CSI M, 32+0, 33, 33
        assert_eq!(
            mouse_report(0, MouseEventKind::Press, cell(0, 0), &no_mods(), &mode).unwrap(),
            &[0x1b, b'[', b'M', 32, 33, 33]
        );
        // release → button bits replaced by 3
        assert_eq!(
            mouse_report(0, MouseEventKind::Release, cell(0, 0), &no_mods(), &mode).unwrap(),
            &[0x1b, b'[', b'M', 35, 33, 33]
        );
        // beyond byte range without UTF-8 → dropped
        assert_eq!(
            mouse_report(0, MouseEventKind::Press, cell(300, 0), &no_mods(), &mode),
            None
        );
        // …but encodes with UTF8_MOUSE (1005)
        let utf8_mode = TermMode::MOUSE_REPORT_CLICK | TermMode::UTF8_MOUSE;
        let bytes =
            mouse_report(0, MouseEventKind::Press, cell(300, 0), &no_mods(), &utf8_mode).unwrap();
        assert_eq!(&bytes[..4], &[0x1b, b'[', b'M', 32]);
        // 32 + 301 = 333 → 2-byte UTF-8 for U+014D
        assert_eq!(&bytes[4..6], "ō".as_bytes());
        assert_eq!(bytes[6], 33);
    }

    #[test]
    fn wheel_and_alt_scroll() {
        let sgr = TermMode::SGR_MOUSE | TermMode::MOUSE_REPORT_CLICK;
        assert_eq!(
            wheel_reports(2, cell(0, 0), &no_mods(), &sgr),
            b"\x1b[<64;1;1M\x1b[<64;1;1M".to_vec()
        );
        assert_eq!(
            wheel_reports(-1, cell(3, 2), &no_mods(), &sgr),
            b"\x1b[<65;4;3M".to_vec()
        );
        assert_eq!(alt_scroll_reports(2, &TermMode::NONE), b"\x1b[A\x1b[A".to_vec());
        assert_eq!(
            alt_scroll_reports(-1, &TermMode::APP_CURSOR),
            b"\x1bOB".to_vec()
        );
    }

    #[test]
    fn should_report_gating() {
        let click_only = TermMode::MOUSE_REPORT_CLICK;
        let drag = TermMode::MOUSE_DRAG;
        let motion = TermMode::MOUSE_MOTION;

        // Shift always forces local handling.
        assert!(!should_report(MouseEventKind::Press, false, true, &click_only));
        // No mouse mode → never report.
        assert!(!should_report(MouseEventKind::Press, false, false, &TermMode::NONE));
        // Click mode: press/release yes, motion no.
        assert!(should_report(MouseEventKind::Press, false, false, &click_only));
        assert!(should_report(MouseEventKind::Release, false, false, &click_only));
        assert!(!should_report(MouseEventKind::Motion, true, false, &click_only));
        // Drag mode: motion only while a button is held.
        assert!(should_report(MouseEventKind::Motion, true, false, &drag));
        assert!(!should_report(MouseEventKind::Motion, false, false, &drag));
        // Motion mode: all motion.
        assert!(should_report(MouseEventKind::Motion, false, false, &motion));
    }

    #[test]
    fn button_codes() {
        assert_eq!(button_code(MouseButton::Left), Some(0));
        assert_eq!(button_code(MouseButton::Middle), Some(1));
        assert_eq!(button_code(MouseButton::Right), Some(2));
    }
}
