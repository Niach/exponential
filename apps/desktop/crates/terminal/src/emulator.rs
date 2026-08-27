// Clean reimplementation from the VT spec + rio-vt (MIT). NOT derived from Zed's GPL terminal crates.
//! The emulator (masterplan-v3 §6 / §6.6): rio-vt `Crosswords` behind a
//! `FairMutex`, the `EventProxy` `EventListener` bridge, and the event drain
//! that answers the **reply-required** event family (`PtyWrite`,
//! `ColorRequest`, `TextAreaSizeRequest`) back into the PTY writer — drop
//! those replies and full-screen TUIs (`vim`, the `claude` TUI) hang on a
//! blank screen probing DA/DSR at startup.
//!
//! EXP-636 moved the core from alacritty_terminal to rio-vt for its
//! sixel/kitty/iTerm2 graphics: decoded pixels arrive as `UpdateGraphics`
//! events and are handed to the paint side as [`GraphicsUpdate`]s, while the
//! placements stay in `term.graphics` and are resolved per frame by the
//! element. The headless CLI never enables graphics, so it parses image
//! streams without retaining a single pixel.
//!
//! OSC-52 clipboard stays off (§6.15): rio has no config gate for it, so the
//! drain ignoring `ClipboardStore`/`ClipboardLoad` IS the gate.

use rio_graphics::{atlas_image_key, kitty_image_key, GraphicData};
use rio_vt::ansi::CursorShape;
use rio_vt::config::colors::ColorRgb;
use rio_vt::crosswords::grid::Scroll;
use rio_vt::crosswords::pos::{Column, Line, Pos};
use rio_vt::crosswords::{Crosswords, CrosswordsSize, Mode};
use rio_vt::event::sync::FairMutex;
use rio_vt::event::{EventListener, RioEvent, WindowId, WindowSize};
use rio_vt::performer::handler::Processor;
use std::sync::Arc;

/// The terminal mode bitflags (`BRACKETED_PASTE`, the mouse modes, `APP_CURSOR`,
/// `ALT_SCREEN`…) — the name the key/mouse encoders were written against.
pub type TermMode = Mode;

/// The emulator core, generic over our event proxy.
pub type Term = Crosswords<EventProxy>;

/// The shared emulator handle: contended only between the read thread
/// (`processor.advance` under the lock, §6.4) and the paint snapshot —
/// `FairMutex` keeps heavy output (`yes`, huge `cat`) from starving paint.
pub type TermHandle = Arc<FairMutex<Term>>;

/// Scrollback history lines (the same default the alacritty core had).
pub const SCROLLBACK_LINES: usize = 10_000;

/// Nominal cell pixel metrics until a window reports real ones: the headless
/// CLI never paints, but CSI 14t/16t must still answer something sane, and
/// rio refuses to place sixels while the cell size is zero.
pub const DEFAULT_CELL_PX: (u32, u32) = (8, 16);

/// Grid + pixel dimensions for `Crosswords::new`/`resize`. The pixel form is
/// mandatory for image protocols: placements derive their cell span from it.
fn size_spec(cols: u16, rows: u16, (cell_w, cell_h): (u32, u32)) -> CrosswordsSize {
    CrosswordsSize::new_with_dimensions(
        cols as usize,
        rows as usize,
        cols as u32 * cell_w,
        rows as u32 * cell_h,
        cell_w,
        cell_h,
    )
}

/// §6.6: the tiny `Send` proxy the core requires — forwards every `RioEvent`
/// onto a flume channel drained on the foreground.
#[derive(Clone)]
pub struct EventProxy(flume::Sender<RioEvent>);

impl EventProxy {
    pub fn new(tx: flume::Sender<RioEvent>) -> Self {
        Self(tx)
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: RioEvent, _id: WindowId) {
        let _ = self.0.send(event);
    }

    fn send_event_with_high_priority(&self, event: RioEvent, id: WindowId) {
        self.send_event(event, id);
    }
}

/// Outward signals produced by the event drain — everything that is NOT a
/// reply written straight back to the PTY. The gpui layer maps these to
/// tab-title updates, `cx.notify()`, and the visual bell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmulatorSignal {
    /// Repaint requested (damage / cursor-blink / mouse-cursor changes).
    Redraw,
    /// Tab title: `Some(title)` from OSC 0/2, `None` on reset (§6.6).
    Title(Option<String>),
    /// Terminal bell — optional subtle visual bell, no audio in v1 (§6.6).
    Bell,
}

/// Decoded image pixels the paint side must upload, plus texture keys to
/// free. Keys are rio's texture keys (`kitty_image_key` for kitty images,
/// `atlas_image_key` for sixel/iTerm2), the same ones the placements in
/// `term.graphics` name.
#[derive(Debug, Default)]
pub struct GraphicsUpdate {
    pub images: Vec<(u64, GraphicData)>,
    pub removed: Vec<u64>,
}

pub struct Emulator {
    term: TermHandle,
    events: flume::Receiver<RioEvent>,
    cols: u16,
    rows: u16,
    cell_px: (u32, u32),
    graphics_enabled: bool,
    pending_graphics: Vec<GraphicsUpdate>,
}

impl Emulator {
    pub fn new(cols: u16, rows: u16) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let (tx, rx) = flume::unbounded();
        let mut term = Crosswords::new(
            size_spec(cols, rows, DEFAULT_CELL_PX),
            CursorShape::Block,
            EventProxy::new(tx),
            WindowId::from(0u64),
            0,
            SCROLLBACK_LINES,
        );
        // Parity with the previous core (EXP-636): no DEC 2027 grapheme
        // clustering, so DECRQM 2027 answers "reset" like before and wide-char
        // column math stays what the steer pickers were validated against.
        term.set_grapheme_clustering(false);
        Self {
            term: Arc::new(FairMutex::new(term)),
            events: rx,
            cols,
            rows,
            cell_px: DEFAULT_CELL_PX,
            graphics_enabled: false,
            pending_graphics: Vec::new(),
        }
    }

    pub fn term(&self) -> TermHandle {
        self.term.clone()
    }

    pub fn size(&self) -> (u16, u16) {
        (self.cols, self.rows)
    }

    /// Reshape the grid + reflow scrollback (§6.10 step 2). The PTY resize
    /// (SIGWINCH) is step 1 and lives on `Pty`; the session fires both
    /// together on integer cell changes only.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.max(1);
        let rows = rows.max(1);
        self.cols = cols;
        self.rows = rows;
        self.term.lock().resize(size_spec(cols, rows, self.cell_px));
    }

    /// Cell pixel metrics from the paint side (EXP-636): feeds the CSI 14t
    /// pixel reply and lets rio size/rescale image placements. A no-op when
    /// unchanged; otherwise a same-grid resize so existing placements track
    /// the new stride.
    pub fn set_cell_px(&mut self, width: u32, height: u32) {
        let cell_px = (width.max(1), height.max(1));
        if cell_px == self.cell_px {
            return;
        }
        self.cell_px = cell_px;
        self.term.lock().resize(size_spec(self.cols, self.rows, cell_px));
    }

    pub fn cell_px(&self) -> (u32, u32) {
        self.cell_px
    }

    /// Retain decoded image pixels for painting. Off by default so a headless
    /// consumer (the CLI daemon) parses image protocols without buffering.
    pub fn enable_graphics(&mut self) {
        self.graphics_enabled = true;
    }

    /// Image uploads/frees queued since the last call, in arrival order.
    pub fn take_graphics(&mut self) -> Vec<GraphicsUpdate> {
        std::mem::take(&mut self.pending_graphics)
    }

    /// Feed raw bytes straight into the grid — no PTY needed. Test fixtures
    /// and the steer harnesses paint screens with this; the live path is the
    /// read loop's long-lived `Processor` (a fresh one here carries no
    /// partial-escape state across calls).
    pub fn advance_bytes(&self, bytes: &[u8]) {
        advance_bytes(&self.term, bytes);
    }

    /// Drain all pending terminal events (§6.6's table). Reply-required
    /// events are answered by calling `write` (which the session wires to the
    /// shared PTY writer); everything user-facing comes back as signals.
    pub fn drain_events(&mut self, write: &mut dyn FnMut(&[u8])) -> Vec<EmulatorSignal> {
        let mut signals = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            match event {
                // How DA1/DA2, DSR/cursor-position, and other query replies
                // get back to the child. Drop this and `claude` hangs.
                RioEvent::PtyWrite(_, text) => write(text.as_bytes()),
                RioEvent::ColorRequest(_, index, formatter) => {
                    let rgb = { self.term.lock().colors()[index] }
                        .map(ColorRgb::from_color_arr)
                        .unwrap_or_else(|| default_color(index));
                    write(formatter(rgb).as_bytes());
                }
                // Same reply-required family as ColorRequest (§6.6): a
                // querying TUI can hang if the CSI 14t/18t answer never comes.
                RioEvent::TextAreaSizeRequest(_, formatter) => {
                    let (cell_w, cell_h) = self.cell_px;
                    let clamp = |px: u32| px.min(u16::MAX as u32) as u16;
                    let window_size = WindowSize {
                        rows: self.rows,
                        cols: self.cols,
                        width: clamp(self.cols as u32 * cell_w),
                        height: clamp(self.rows as u32 * cell_h),
                    };
                    write(formatter(window_size).as_bytes());
                }
                // §6.15: OSC-52 gated off — never bridge the child and the
                // system clipboard in v1.
                RioEvent::ClipboardStore(..) | RioEvent::ClipboardLoad(..) => {
                    log::debug!("ignoring OSC-52 clipboard event (disabled, §6.15)");
                }
                // rio reports an OSC title reset as an empty title.
                RioEvent::Title(title) if title.is_empty() => {
                    signals.push(EmulatorSignal::Title(None))
                }
                RioEvent::Title(title) => signals.push(EmulatorSignal::Title(Some(title))),
                RioEvent::ResetTitle => signals.push(EmulatorSignal::Title(None)),
                RioEvent::Bell => signals.push(EmulatorSignal::Bell),
                // Decoded image pixels (sixel/iTerm2 → atlas keys, kitty →
                // its protocol image id) plus texture keys to free.
                RioEvent::UpdateGraphics { queues, .. } => {
                    if self.graphics_enabled {
                        let images = queues
                            .pending
                            .into_iter()
                            .map(|data| (atlas_image_key(data.id.0), data))
                            .chain(
                                queues
                                    .pending_images
                                    .into_iter()
                                    .map(|(id, data)| (kitty_image_key(id), data)),
                            )
                            .collect();
                        self.pending_graphics.push(GraphicsUpdate {
                            images,
                            removed: queues.remove_queue,
                        });
                    }
                }
                RioEvent::TerminalDamaged(_)
                | RioEvent::MouseCursorDirty
                | RioEvent::CursorBlinkingChange
                | RioEvent::CursorBlinkingChangeOnRoute(_) => signals.push(EmulatorSignal::Redraw),
                // Everything else is Rio-the-app's own machinery (its event
                // loop, tabs, notifications, child exit) which we deliberately
                // don't use (§6.1.1) — our exit path is the pty wait thread +
                // read-loop EOF (§6.7).
                _ => {}
            }
        }
        signals
    }

    /// Plain-text snapshot of the visible screen, one string per row,
    /// trailing whitespace trimmed. Wide-char spacer cells are skipped so a
    /// CJK/emoji glyph contributes exactly one char (§6.9). Test/debug helper.
    pub fn screen_lines(&self) -> Vec<String> {
        screen_lines(&self.term)
    }
}

/// Current grid geometry `(cols, rows)` from a shared [`TermHandle`] — the
/// steer publisher's off-thread read for the §8.4 `hello` (TRUE geometry,
/// never a hardcoded 80×24). Mirrors the [`screen_lines`] free-fn pattern.
pub fn grid_size(term: &TermHandle) -> (u16, u16) {
    let term = term.lock();
    (term.columns().max(1) as u16, term.screen_lines().max(1) as u16)
}

/// Whether the child has bracketed paste (DEC private mode 2004) on — the
/// same bit `Terminal::paste` gates on, as a free fn over a shared
/// [`TermHandle`] (the [`grid_size`] pattern) for the steer publisher's
/// off-thread remote-input path (§8.4).
pub fn bracketed_paste_enabled(term: &TermHandle) -> bool {
    term.lock().mode().contains(Mode::BRACKETED_PASTE)
}

/// Scrollback display offset from a shared [`TermHandle`] — `0` when the
/// viewport sits at the live bottom of the grid, `> 0` while the user has
/// scrolled up into history. The steer plan-picker watcher freezes on a
/// scrolled viewport so history scrolling can't fake a picker appearing or
/// resolving (EXP-150).
pub fn display_offset(term: &TermHandle) -> usize {
    term.lock().display_offset()
}

/// Snap the viewport back to the live bottom of the grid (display offset 0).
/// The steer remote-answer path uses this (EXP-611): a viewport a local user
/// left scrolled into history feeds HISTORY rows to picker detection, so
/// every remote answer was refused as transient until its retry TTL dropped
/// it — the steerer saw "No confirmation from the desktop" forever. A remote
/// answer targets the LIVE picker by construction, so snapping to the bottom
/// is exactly what the local user would do before pressing the key.
pub fn scroll_to_bottom(term: &TermHandle) {
    term.lock().scroll_display(Scroll::Bottom);
}

/// Scroll the viewport up into history by `lines` (clamped by the core to
/// the available scrollback) — how a harness simulates a user reading
/// history without linking the emulator crate directly.
pub fn scroll_up(term: &TermHandle, lines: usize) {
    term.lock().scroll_display(Scroll::Delta(lines.min(i32::MAX as usize) as i32));
}

/// Feed raw bytes into a shared [`TermHandle`] (see
/// [`Emulator::advance_bytes`]).
pub fn advance_bytes(term: &TermHandle, bytes: &[u8]) {
    let mut processor = Processor::default();
    processor.advance(&mut *term.lock(), bytes);
}

/// Free-function variant of [`Emulator::screen_lines`] usable with just a
/// [`TermHandle`].
pub fn screen_lines(term: &TermHandle) -> Vec<String> {
    let term = term.lock();
    let offset = term.display_offset() as i32;
    let cols = term.columns();
    (0..term.screen_lines())
        .map(|row| {
            let line = Line(row as i32 - offset);
            let mut text = String::new();
            for col in 0..cols {
                let pos = Pos::new(line, Column(col));
                let square = term.grid[pos];
                if square.is_spacer() {
                    continue; // trailing half of a double-width glyph (§6.9)
                }
                // rio's empty square reads back as NUL (the old core gave a
                // space) — every text consumer (steer pickers!) expects ' '.
                text.extend(term.grid.cell_text(pos).map(|c| if c == '\0' { ' ' } else { c }));
            }
            text.truncate(text.trim_end().len());
            text
        })
        .collect()
}

/// Default color for a `ColorRequest` on an index the child never set —
/// standard xterm-256 defaults, reimplemented from the xterm spec (§6.8;
/// the theme-mapped palette is a paint-time concern in `element.rs`).
fn default_color(index: usize) -> ColorRgb {
    /// xterm's default 16-color table.
    const ANSI_16: [(u8, u8, u8); 16] = [
        (0x00, 0x00, 0x00), // black
        (0xcd, 0x00, 0x00), // red
        (0x00, 0xcd, 0x00), // green
        (0xcd, 0xcd, 0x00), // yellow
        (0x00, 0x00, 0xee), // blue
        (0xcd, 0x00, 0xcd), // magenta
        (0x00, 0xcd, 0xcd), // cyan
        (0xe5, 0xe5, 0xe5), // white
        (0x7f, 0x7f, 0x7f), // bright black
        (0xff, 0x00, 0x00), // bright red
        (0x00, 0xff, 0x00), // bright green
        (0xff, 0xff, 0x00), // bright yellow
        (0x5c, 0x5c, 0xff), // bright blue
        (0xff, 0x00, 0xff), // bright magenta
        (0x00, 0xff, 0xff), // bright cyan
        (0xff, 0xff, 0xff), // bright white
    ];
    let rgb = |(r, g, b): (u8, u8, u8)| ColorRgb { r, g, b };
    match index {
        0..=15 => rgb(ANSI_16[index]),
        // 6×6×6 color cube: component n ∈ 0..6 → 0 or 55 + 40n.
        16..=231 => {
            let i = index - 16;
            let component = |n: usize| if n == 0 { 0 } else { (55 + 40 * n) as u8 };
            ColorRgb { r: component(i / 36), g: component((i / 6) % 6), b: component(i % 6) }
        }
        // 24-step grayscale ramp: 8 + 10n.
        232..=255 => {
            let v = (8 + 10 * (index - 232)) as u8;
            ColorRgb { r: v, g: v, b: v }
        }
        // Specials (NamedColor::Foreground = 256, Background, Cursor, then
        // the dim variants and dim foreground) — the same table layout as
        // the previous core.
        256 => rgb((0xe5, 0xe5, 0xe5)), // foreground
        257 => rgb((0x00, 0x00, 0x00)), // background
        258 => rgb((0xe5, 0xe5, 0xe5)), // cursor
        259..=266 => {
            // Dim variants: 2/3 of the base 8-color table.
            let (r, g, b) = ANSI_16[index - 259];
            ColorRgb {
                r: (r as u16 * 2 / 3) as u8,
                g: (g as u16 * 2 / 3) as u8,
                b: (b as u16 * 2 / 3) as u8,
            }
        }
        267 => rgb((0xff, 0xff, 0xff)), // bright foreground
        268 => rgb((0x98, 0x98, 0x98)), // dim foreground
        _ => rgb((0x00, 0x00, 0x00)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_vt::crosswords::grid::Dimensions;

    /// A 2×2 opaque red RGBA kitty image, transmitted and displayed at the
    /// cursor with explicit image id 1 (the fixture rio's own example uses).
    const KITTY_RED_2X2: &[u8] = b"\x1b_Gf=32,s=2,v=2,a=T,i=1;/wAA//8AAP//AAD//wAA/w==\x1b\\";

    /// A 6×6 solid red sixel: raster attributes, one palette entry, six full
    /// sixel columns.
    const SIXEL_RED_6X6: &[u8] = b"\x1bPq\"1;1;6;6#0;2;100;0;0#0~~~~~~\x1b\\";

    fn drain(emulator: &mut Emulator) -> (Vec<EmulatorSignal>, Vec<u8>) {
        let mut written = Vec::new();
        let signals = emulator.drain_events(&mut |bytes| written.extend_from_slice(bytes));
        (signals, written)
    }

    #[test]
    fn size_spec_reports_cells_and_pixels() {
        let size = size_spec(80, 24, (8, 16));
        assert_eq!(size.columns(), 80);
        assert_eq!(size.screen_lines(), 24);
        assert_eq!(size.total_lines(), 24);
        assert_eq!((size.width, size.height), (640, 384));
        assert_eq!((size.square_width(), size.square_height()), (8.0, 16.0));
    }

    #[test]
    fn plain_text_lands_in_grid() {
        let emulator = Emulator::new(20, 4);
        emulator.advance_bytes(b"hello grid");
        assert_eq!(emulator.screen_lines()[0], "hello grid");
    }

    #[test]
    fn empty_cells_read_back_as_spaces() {
        // rio stores NUL in untouched squares; the text snapshot must show
        // the gap as spaces, exactly like the previous core did.
        let emulator = Emulator::new(20, 4);
        emulator.advance_bytes(b"\x1b[4Ga");
        assert_eq!(emulator.screen_lines()[0], "   a");
    }

    #[test]
    fn scroll_to_bottom_snaps_a_scrolled_viewport() {
        // EXP-611: a viewport scrolled into history feeds HISTORY rows to
        // `screen_lines` — the steer answer path snaps it back before
        // injecting.
        let emulator = Emulator::new(20, 4);
        for i in 0..20 {
            emulator.advance_bytes(format!("line {i}\r\n").as_bytes());
        }
        let term = emulator.term();
        term.lock().scroll_display(Scroll::Delta(10));
        assert!(display_offset(&term) > 0);
        assert_ne!(screen_lines(&term)[0], "line 17");
        scroll_to_bottom(&term);
        assert_eq!(display_offset(&term), 0);
        assert_eq!(screen_lines(&term)[0], "line 17");
    }

    #[test]
    fn title_events_surface_as_signals() {
        let mut emulator = Emulator::new(20, 4);
        emulator.advance_bytes(b"\x1b]0;my-title\x07");
        let (signals, written) = drain(&mut emulator);
        assert!(signals.contains(&EmulatorSignal::Title(Some("my-title".into()))));
        assert!(written.is_empty());
    }

    #[test]
    fn title_reset_maps_empty_title_to_none() {
        let mut emulator = Emulator::new(20, 4);
        emulator.advance_bytes(b"\x1b]0;x\x07\x1b]0;\x07");
        let (signals, _) = drain(&mut emulator);
        assert_eq!(
            signals.iter().filter(|s| matches!(s, EmulatorSignal::Title(_))).last(),
            Some(&EmulatorSignal::Title(None))
        );
    }

    #[test]
    fn dsr_cursor_report_is_replied_to_the_writer() {
        let mut emulator = Emulator::new(20, 4);
        emulator.advance_bytes(b"\x1b[6n"); // DSR: report cursor position
        let (_, written) = drain(&mut emulator);
        assert_eq!(written, b"\x1b[1;1R"); // cursor at home
    }

    #[test]
    fn text_area_size_request_is_replied_in_cells() {
        let mut emulator = Emulator::new(80, 24);
        emulator.advance_bytes(b"\x1b[18t"); // report text-area size in chars
        let (_, written) = drain(&mut emulator);
        assert_eq!(written, b"\x1b[8;24;80t");
    }

    #[test]
    fn text_area_size_request_in_pixels_uses_cell_metrics() {
        let mut emulator = Emulator::new(80, 24);
        emulator.advance_bytes(b"\x1b[14t"); // report text-area size in pixels
        let (_, written) = drain(&mut emulator);
        assert_eq!(written, b"\x1b[4;384;640t", "default 8x16 cells");
        emulator.set_cell_px(9, 20);
        emulator.advance_bytes(b"\x1b[14t");
        let (_, written) = drain(&mut emulator);
        assert_eq!(written, b"\x1b[4;480;720t");
    }

    #[test]
    fn color_request_replies_with_default_when_unset() {
        let mut emulator = Emulator::new(20, 4);
        emulator.advance_bytes(b"\x1b]4;1;?\x07"); // OSC 4: query color 1 (red)
        let (_, written) = drain(&mut emulator);
        let reply = String::from_utf8(written).expect("utf8 reply");
        assert!(reply.contains("cd00") || reply.contains("cdcd"), "reply: {reply:?}");
    }

    #[test]
    fn osc52_store_is_swallowed_by_the_drain() {
        let mut emulator = Emulator::new(20, 4);
        // OSC 52 copy: emits ClipboardStore, which the drain drops (§6.15).
        emulator.advance_bytes(b"\x1b]52;c;aGVsbG8=\x07");
        let (signals, written) = drain(&mut emulator);
        assert!(written.is_empty());
        assert!(!signals.iter().any(|s| matches!(s, EmulatorSignal::Title(_))));
    }

    #[test]
    fn bracketed_paste_tracks_mode_2004() {
        let emulator = Emulator::new(20, 4);
        let term = emulator.term();
        assert!(!bracketed_paste_enabled(&term));
        emulator.advance_bytes(b"\x1b[?2004h");
        assert!(bracketed_paste_enabled(&term));
        emulator.advance_bytes(b"\x1b[?2004l");
        assert!(!bracketed_paste_enabled(&term));
    }

    #[test]
    fn resize_reshapes_the_grid() {
        let mut emulator = Emulator::new(20, 4);
        emulator.advance_bytes(b"before resize");
        emulator.resize(40, 10);
        assert_eq!(emulator.size(), (40, 10));
        assert_eq!(emulator.screen_lines().len(), 10);
        assert!(emulator.screen_lines()[0].contains("before resize"));
    }

    #[test]
    fn kitty_transmit_surfaces_as_a_graphics_update() {
        let mut emulator = Emulator::new(20, 4);
        emulator.enable_graphics();
        emulator.advance_bytes(KITTY_RED_2X2);
        drain(&mut emulator);
        let updates = emulator.take_graphics();
        assert_eq!(updates.len(), 1, "one upload batch: {updates:?}");
        let (key, data) = &updates[0].images[0];
        assert_eq!(*key, kitty_image_key(1));
        assert_eq!((data.width, data.height), (2, 2));
        assert!(updates[0].removed.is_empty());
        assert_eq!(emulator.term().lock().graphics.kitty_placements.len(), 1);
        assert!(emulator.take_graphics().is_empty(), "taken once");
    }

    #[test]
    fn graphics_are_dropped_when_not_enabled() {
        // The headless CLI path: the placement is still tracked (so the grid
        // stays consistent) but no pixels are retained.
        let mut emulator = Emulator::new(20, 4);
        emulator.advance_bytes(KITTY_RED_2X2);
        drain(&mut emulator);
        assert!(emulator.take_graphics().is_empty());
        assert_eq!(emulator.term().lock().graphics.kitty_placements.len(), 1);
    }

    #[test]
    fn sixel_places_an_atlas_graphic() {
        let mut emulator = Emulator::new(20, 4);
        emulator.enable_graphics();
        emulator.advance_bytes(SIXEL_RED_6X6);
        drain(&mut emulator);
        let updates = emulator.take_graphics();
        assert_eq!(updates.len(), 1, "sixel upload: {updates:?}");
        let (key, _) = &updates[0].images[0];
        assert!(*key >= 1 << 32, "atlas keys live above the kitty u32 space: {key}");
        assert_eq!(emulator.term().lock().graphics.atlas_placements.len(), 1);
    }

    #[test]
    fn default_color_xterm_math() {
        let rgb = |r, g, b| ColorRgb { r, g, b };
        assert_eq!(default_color(1), rgb(0xcd, 0, 0));
        assert_eq!(default_color(16), rgb(0, 0, 0));
        assert_eq!(default_color(231), rgb(255, 255, 255));
        assert_eq!(default_color(232), rgb(8, 8, 8));
        assert_eq!(default_color(255), rgb(238, 238, 238));
        // 196 = 16 + 180 → r=5,g=0,b=0 → (255, 0, 0)
        assert_eq!(default_color(196), rgb(255, 0, 0));
    }
}
