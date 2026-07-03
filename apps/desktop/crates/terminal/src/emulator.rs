// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! The emulator (masterplan-v3 §6 / §6.6): alacritty `Term` behind a
//! `FairMutex`, the `EventProxy` `EventListener` bridge, and the event drain
//! that answers the **reply-required** event family (`PtyWrite`,
//! `ColorRequest`, `TextAreaSizeRequest`) back into the PTY writer — drop
//! those replies and full-screen TUIs (`vim`, the `claude` TUI) hang on a
//! blank screen probing DA/DSR at startup.
//!
//! OSC-52 clipboard is disabled at BOTH levels (§6.15): `Osc52::Disabled` on
//! the emulator `Config`, and `ClipboardStore`/`ClipboardLoad` ignored in the
//! drain.

use alacritty_terminal::event::{Event as AlacTermEvent, EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Osc52, Term};
use std::sync::Arc;
use vte::ansi::Rgb;

/// Own `Dimensions` impl (§6.10): upstream 0.26 has **no** production size
/// type — the only stock impl (`TermSize`) lives in
/// `alacritty_terminal::term::test`. `Term::new` / `Term::resize` are generic
/// over `Dimensions`, so this ~12-line struct is the production-clean path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSize {
    pub columns: usize,
    pub screen_lines: usize,
    pub total_lines: usize,
}

impl GridSize {
    pub fn new(columns: usize, screen_lines: usize) -> Self {
        Self { columns, screen_lines, total_lines: screen_lines }
    }
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.total_lines
    }
    fn screen_lines(&self) -> usize {
        self.screen_lines
    }
    fn columns(&self) -> usize {
        self.columns
    }
}

/// §6.6: the tiny `Send` proxy `Term::new` requires — forwards every
/// `AlacTermEvent` onto a flume channel drained on the foreground.
#[derive(Clone)]
pub struct EventProxy(flume::Sender<AlacTermEvent>);

impl EventProxy {
    pub fn new(tx: flume::Sender<AlacTermEvent>) -> Self {
        Self(tx)
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: AlacTermEvent) {
        let _ = self.0.send(event);
    }
}

/// The shared emulator handle: contended only between the read thread
/// (`processor.advance` under the lock, §6.4) and the paint
/// (`renderable_content`) — `FairMutex` keeps heavy output (`yes`, huge
/// `cat`) from starving paint.
pub type TermHandle = Arc<FairMutex<Term<EventProxy>>>;

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

pub struct Emulator {
    term: TermHandle,
    events: flume::Receiver<AlacTermEvent>,
    cols: u16,
    rows: u16,
}

impl Emulator {
    pub fn new(cols: u16, rows: u16) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let (tx, rx) = flume::unbounded();
        // §6.15: OSC-52 suppressed at the emulator itself, in addition to the
        // drain ignoring ClipboardStore/ClipboardLoad.
        let config = Config { osc52: Osc52::Disabled, ..Config::default() };
        let term = Term::new(
            config,
            &GridSize::new(cols as usize, rows as usize),
            EventProxy::new(tx),
        );
        Self { term: Arc::new(FairMutex::new(term)), events: rx, cols, rows }
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
        self.term.lock().resize(GridSize::new(cols as usize, rows as usize));
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
                AlacTermEvent::PtyWrite(text) => write(text.as_bytes()),
                AlacTermEvent::ColorRequest(index, formatter) => {
                    let rgb = { self.term.lock().colors()[index] }
                        .unwrap_or_else(|| default_color(index));
                    write(formatter(rgb).as_bytes());
                }
                // Same reply-required family as ColorRequest (§6.6): a
                // querying TUI can hang if the CSI 18t answer never comes.
                AlacTermEvent::TextAreaSizeRequest(formatter) => {
                    let window_size = WindowSize {
                        num_lines: self.rows,
                        num_cols: self.cols,
                        // We report character cells only (§6.3).
                        cell_width: 0,
                        cell_height: 0,
                    };
                    write(formatter(window_size).as_bytes());
                }
                // §6.15: OSC-52 gated off — never bridge the child and the
                // system clipboard in v1. (Config::osc52 = Disabled should
                // already suppress these; belt and braces.)
                AlacTermEvent::ClipboardStore(..) | AlacTermEvent::ClipboardLoad(..) => {
                    log::debug!("ignoring OSC-52 clipboard event (disabled, §6.15)");
                }
                AlacTermEvent::Title(title) => signals.push(EmulatorSignal::Title(Some(title))),
                AlacTermEvent::ResetTitle => signals.push(EmulatorSignal::Title(None)),
                AlacTermEvent::Bell => signals.push(EmulatorSignal::Bell),
                AlacTermEvent::Wakeup
                | AlacTermEvent::CursorBlinkingChange
                | AlacTermEvent::MouseCursorDirty => signals.push(EmulatorSignal::Redraw),
                // Emitted only by alacritty's own tty/event_loop machinery,
                // which we deliberately don't use (§6.1.1) — our exit path is
                // the pty wait thread + read-loop EOF (§6.7).
                AlacTermEvent::Exit | AlacTermEvent::ChildExit(_) => {}
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
    (
        term.grid().columns().max(1) as u16,
        term.grid().screen_lines().max(1) as u16,
    )
}

/// Free-function variant of [`Emulator::screen_lines`] usable with just a
/// [`TermHandle`].
pub fn screen_lines(term: &TermHandle) -> Vec<String> {
    let term = term.lock();
    let content = term.renderable_content();
    let mut lines: Vec<String> = Vec::new();
    let mut current_line: Option<i32> = None;
    for cell in content.display_iter {
        if current_line != Some(cell.point.line.0) {
            current_line = Some(cell.point.line.0);
            lines.push(String::new());
        }
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue; // trailing half of a double-width glyph (§6.9)
        }
        lines.last_mut().expect("line pushed above").push(cell.c);
    }
    for line in &mut lines {
        line.truncate(line.trim_end().len());
    }
    lines
}

/// Default color for a `ColorRequest` on an index the child never set —
/// standard xterm-256 defaults, reimplemented from the xterm spec (§6.8;
/// the theme-mapped palette is a paint-time concern in `element.rs`).
fn default_color(index: usize) -> Rgb {
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
    let rgb = |(r, g, b): (u8, u8, u8)| Rgb { r, g, b };
    match index {
        0..=15 => rgb(ANSI_16[index]),
        // 6×6×6 color cube: component n ∈ 0..6 → 0 or 55 + 40n.
        16..=231 => {
            let i = index - 16;
            let component = |n: usize| if n == 0 { 0 } else { (55 + 40 * n) as u8 };
            Rgb { r: component(i / 36), g: component((i / 6) % 6), b: component(i % 6) }
        }
        // 24-step grayscale ramp: 8 + 10n.
        232..=255 => {
            let v = (8 + 10 * (index - 232)) as u8;
            Rgb { r: v, g: v, b: v }
        }
        // Specials (NamedColor::Foreground = 256, Background, Cursor, then
        // the dim variants and dim foreground).
        256 => rgb((0xe5, 0xe5, 0xe5)), // foreground
        257 => rgb((0x00, 0x00, 0x00)), // background
        258 => rgb((0xe5, 0xe5, 0xe5)), // cursor
        259..=266 => {
            // Dim variants: 2/3 of the base 8-color table.
            let (r, g, b) = ANSI_16[index - 259];
            Rgb { r: (r as u16 * 2 / 3) as u8, g: (g as u16 * 2 / 3) as u8, b: (b as u16 * 2 / 3) as u8 }
        }
        267 => rgb((0xff, 0xff, 0xff)), // bright foreground
        268 => rgb((0x98, 0x98, 0x98)), // dim foreground
        _ => rgb((0x00, 0x00, 0x00)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vte::ansi::{Processor, StdSyncHandler};

    /// Feed raw bytes straight into the emulator's Term — no PTY needed.
    fn advance(emulator: &Emulator, bytes: &[u8]) {
        // Turbofish REQUIRED (§6.4): the `T: Timeout = StdSyncHandler`
        // default type param does not participate in fn-call inference.
        let mut processor = Processor::<StdSyncHandler>::new();
        let term = emulator.term();
        let mut term = term.lock();
        processor.advance(&mut *term, bytes);
    }

    fn drain(emulator: &mut Emulator) -> (Vec<EmulatorSignal>, Vec<u8>) {
        let mut written = Vec::new();
        let signals = emulator.drain_events(&mut |bytes| written.extend_from_slice(bytes));
        (signals, written)
    }

    #[test]
    fn grid_size_dimensions() {
        let size = GridSize::new(80, 24);
        assert_eq!(size.columns(), 80);
        assert_eq!(size.screen_lines(), 24);
        assert_eq!(size.total_lines(), 24);
    }

    #[test]
    fn plain_text_lands_in_grid() {
        let emulator = Emulator::new(20, 4);
        advance(&emulator, b"hello grid");
        assert_eq!(emulator.screen_lines()[0], "hello grid");
    }

    #[test]
    fn title_events_surface_as_signals() {
        let mut emulator = Emulator::new(20, 4);
        advance(&emulator, b"\x1b]0;my-title\x07");
        let (signals, written) = drain(&mut emulator);
        assert!(signals.contains(&EmulatorSignal::Title(Some("my-title".into()))));
        assert!(written.is_empty());
    }

    #[test]
    fn dsr_cursor_report_is_replied_to_the_writer() {
        let mut emulator = Emulator::new(20, 4);
        advance(&emulator, b"\x1b[6n"); // DSR: report cursor position
        let (_, written) = drain(&mut emulator);
        assert_eq!(written, b"\x1b[1;1R"); // cursor at home
    }

    #[test]
    fn text_area_size_request_is_replied_in_cells() {
        let mut emulator = Emulator::new(80, 24);
        advance(&emulator, b"\x1b[18t"); // report text-area size in chars
        let (_, written) = drain(&mut emulator);
        assert_eq!(written, b"\x1b[8;24;80t");
    }

    #[test]
    fn color_request_replies_with_default_when_unset() {
        let mut emulator = Emulator::new(20, 4);
        advance(&emulator, b"\x1b]4;1;?\x07"); // OSC 4: query color 1 (red)
        let (_, written) = drain(&mut emulator);
        let reply = String::from_utf8(written).expect("utf8 reply");
        assert!(reply.contains("cd00") || reply.contains("cdcd"), "reply: {reply:?}");
    }

    #[test]
    fn osc52_store_is_suppressed_by_config() {
        let mut emulator = Emulator::new(20, 4);
        // OSC 52 copy: would emit ClipboardStore if not disabled (§6.15).
        advance(&emulator, b"\x1b]52;c;aGVsbG8=\x07");
        let (signals, written) = drain(&mut emulator);
        assert!(written.is_empty());
        assert!(!signals.iter().any(|s| matches!(s, EmulatorSignal::Title(_))));
    }

    #[test]
    fn resize_reshapes_the_grid() {
        let mut emulator = Emulator::new(20, 4);
        advance(&emulator, b"before resize");
        emulator.resize(40, 10);
        assert_eq!(emulator.size(), (40, 10));
        assert_eq!(emulator.screen_lines().len(), 10);
        assert!(emulator.screen_lines()[0].contains("before resize"));
    }

    #[test]
    fn default_color_xterm_math() {
        assert_eq!(default_color(1), Rgb { r: 0xcd, g: 0, b: 0 });
        assert_eq!(default_color(16), Rgb { r: 0, g: 0, b: 0 });
        assert_eq!(default_color(231), Rgb { r: 255, g: 255, b: 255 });
        assert_eq!(default_color(232), Rgb { r: 8, g: 8, b: 8 });
        assert_eq!(default_color(255), Rgb { r: 238, g: 238, b: 238 });
        // 196 = 16 + 180 → r=5,g=0,b=0 → (255, 0, 0)
        assert_eq!(default_color(196), Rgb { r: 255, g: 0, b: 0 });
    }
}
