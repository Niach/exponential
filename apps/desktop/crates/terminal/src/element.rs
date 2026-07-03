// Clean reimplementation from the VT spec + alacritty_terminal (Apache-2.0). NOT derived from Zed's GPL terminal crates.
//! The gpui grid element (masterplan-v3 §6.9) + the `TerminalView` entity
//! that owns a [`Terminal`] session on the foreground.
//!
//! `TerminalElement` is a low-level [`gpui::Element`] (not a composed `div()`
//! tree) with the three-phase `request_layout → prepaint → paint` lifecycle,
//! doing all its own painting. Zed's GPL `terminal_element.rs` was studied
//! for *approach only* (§0.7's licensing boundary); everything here is
//! written against the alacritty grid API + the gpui `Element` trait.
//!
//! Responsibilities (§6.9/§6.10 + this step's task list):
//! - cell metrics from the window text system (mono advance of `m`, line
//!   height = font-size × `theme::terminal::LINE_HEIGHT`), device-pixel
//!   snapped origins and the `next_up().floor()` row/col count;
//! - the §6.10 resize triple's first two steps on integer cell change
//!   (element size → `Term::resize` + PTY `TIOCSWINSZ`; the relay resize
//!   frame is §08's wiring on top);
//! - batched paint: merged background quads + same-style text runs shaped
//!   with a **forced cell advance** so glyphs land on exact cell boundaries;
//! - wide/CJK/emoji handling: spacer cells skipped, `WIDE_CHAR` advances two
//!   cells (its glyph is its own run so the forced advance cannot squeeze
//!   it), zero-width/combining marks fold onto their base cell;
//! - cursor block/beam/underline (+ hollow block when unfocused, blink via
//!   the view's blink task), selection bands + clipboard copy, wheel
//!   scrollback through the grid display offset, mouse-mode reporting, IME
//!   input, and the 0-height collapsed-dock guard.

use crate::emulator::{EmulatorSignal, EventProxy};
use crate::keys;
use crate::mouse::{self, MouseEventKind, ViewportCell};
use crate::pty::ChildExit;
use crate::session::Terminal;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::Point as GridPoint;
use alacritty_terminal::selection::{Selection, SelectionRange, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Term, TermMode};
use gpui::{
    div, fill, outline, point, px, relative, App, BorderStyle, Bounds, ClipboardItem, Context,
    CursorStyle as GpuiCursorStyle, DispatchPhase, Element, ElementId, Entity, EventEmitter,
    FocusHandle, Focusable, Font, FontStyle, FontWeight, GlobalElementId, Hitbox, HitboxBehavior,
    Hsla, InputHandler, InspectorElementId, InteractiveElement, IntoElement, KeyDownEvent,
    LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels,
    Point as PixelPoint, Render, ScrollWheelEvent, ShapedLine, SharedString, StrikethroughStyle,
    Style, Styled, Task, TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window,
};
use std::cell::{Cell as StdCell, RefCell};
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};
use theme::terminal::{terminal_palette, TerminalPalette, FONT_FAMILY, FONT_SIZE, LINE_HEIGHT};
use vte::ansi::{Color as AnsiColor, CursorShape, NamedColor, Rgb};

/// Grid inset so the first column/row is not glued to the panel edge. Small
/// and constant — it participates in the cell math AND the mouse mapping.
const PAD_X: f32 = 4.0;
const PAD_Y: f32 = 2.0;

/// Cursor blink half-period (visible ↔ hidden).
const BLINK_INTERVAL: Duration = Duration::from_millis(530);

// ---------------------------------------------------------------------------
// TerminalView — the gpui entity owning the session on the foreground
// ---------------------------------------------------------------------------

/// Outward events for the tab/manager layer (§6.13 consumes these next step).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalViewEvent {
    /// OSC title changed (`None` = reset) — feeds the window/tab title.
    TitleChanged,
    /// Terminal bell (subtle visual bell only, §6.6).
    Bell,
    /// The child exited — play→stop flip; §07 ends the `coding_sessions` row.
    Exited,
}

/// Foreground owner of one [`Terminal`] session: drains the wake channel
/// (damage → `cx.notify`), pumps reply-required events, handles keys /
/// paste / mouse / IME, and renders the grid element.
pub struct TerminalView {
    session: Rc<RefCell<Terminal>>,
    focus_handle: FocusHandle,
    palette: TerminalPalette,
    title: Option<SharedString>,
    exit: Option<ChildExit>,
    /// Option/Alt sends ESC-prefixed bytes (meta). Linux terminals always do
    /// this; macOS defaults to Option-composition like Terminal.app.
    alt_is_meta: bool,
    blink_visible: bool,
    blink_paused_until: Option<Instant>,
    ime_marked: Option<String>,
    selecting: bool,
    mouse_down_reported: bool,
    scroll_accum: f32,
    last_motion_cell: Option<ViewportCell>,
    /// Written by the element during paint; read by the IME handler for the
    /// candidate-window position (`bounds_for_range`).
    cursor_bounds: Rc<StdCell<Option<Bounds<Pixels>>>>,
    _wake_task: Task<()>,
    _blink_task: Task<()>,
}

impl TerminalView {
    pub fn new(session: Terminal, cx: &mut Context<Self>) -> Self {
        let wake_rx = session.wake_rx();
        let session = Rc::new(RefCell::new(session));

        // §6.11: ONE foreground task drains the flume wake channel; bursts
        // are coalesced so a storm of `Wake::Output` costs one notify.
        let wake_task = cx.spawn(async move |this, cx| {
            while let Ok(_wake) = wake_rx.recv_async().await {
                while wake_rx.try_recv().is_ok() {}
                if this.update(cx, |view, cx| view.on_wake(cx)).is_err() {
                    return;
                }
            }
            // Channel closed (threads joined): one final sweep so a fast
            // exit's last events/exit status are not missed.
            let _ = this.update(cx, |view, cx| view.on_wake(cx));
        });

        // Cursor blink (§6.9): ticks only flip state when the emulator's
        // cursor style actually blinks (DECSCUSR / config), so an idle
        // non-blinking terminal causes no repaints.
        let blink_task = cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(BLINK_INTERVAL).await;
            if this.update(cx, |view, cx| view.blink_tick(cx)).is_err() {
                return;
            }
        });

        Self {
            session,
            focus_handle: cx.focus_handle(),
            palette: terminal_palette(),
            title: None,
            exit: None,
            alt_is_meta: cfg!(not(target_os = "macos")),
            blink_visible: true,
            blink_paused_until: None,
            ime_marked: None,
            selecting: false,
            mouse_down_reported: false,
            scroll_accum: 0.0,
            last_motion_cell: None,
            cursor_bounds: Rc::new(StdCell::new(None)),
            _wake_task: wake_task,
            _blink_task: blink_task,
        }
    }

    /// Latest OSC title (feeds the tab strip; `None` until the child sets
    /// one or after a reset).
    pub fn title(&self) -> Option<&SharedString> {
        self.title.as_ref()
    }

    /// Captured child exit (§6.7), `None` while running.
    pub fn exit_status(&self) -> Option<&ChildExit> {
        self.exit.as_ref()
    }

    pub fn is_running(&self) -> bool {
        self.exit.is_none()
    }

    /// The underlying session — the tab/manager and steer layers attach
    /// sinks / kill / shutdown through this.
    pub fn session(&self) -> &Rc<RefCell<Terminal>> {
        &self.session
    }

    /// Option-as-Meta toggle (macOS setting; Linux is always meta).
    pub fn set_alt_is_meta(&mut self, alt_is_meta: bool) {
        self.alt_is_meta = alt_is_meta;
    }

    fn term_mode(&self) -> TermMode {
        let term = self.session.borrow().term();
        let mode = *term.lock().mode();
        mode
    }

    fn on_wake(&mut self, cx: &mut Context<Self>) {
        // Pump answers the §6.6 reply-required events into the PTY writer
        // and hands back the user-facing signals.
        let signals = self.session.borrow_mut().pump();
        for signal in signals {
            match signal {
                EmulatorSignal::Title(title) => {
                    self.title = title.map(SharedString::from);
                    cx.emit(TerminalViewEvent::TitleChanged);
                }
                EmulatorSignal::Bell => cx.emit(TerminalViewEvent::Bell),
                EmulatorSignal::Redraw => {}
            }
        }
        if self.exit.is_none() {
            let exit = self.session.borrow().exit();
            if let Some(exit) = exit {
                self.exit = Some(exit);
                cx.emit(TerminalViewEvent::Exited);
            }
        }
        // Fresh output re-shows the cursor (standard blink behavior).
        self.blink_visible = true;
        cx.notify();
    }

    fn blink_tick(&mut self, cx: &mut Context<Self>) {
        if let Some(until) = self.blink_paused_until {
            if Instant::now() < until {
                return;
            }
            self.blink_paused_until = None;
        }
        let blinking = self.exit.is_none() && {
            let term = self.session.borrow().term();
            let style = term.lock().cursor_style();
            style.blinking
        };
        if blinking {
            self.blink_visible = !self.blink_visible;
            cx.notify();
        } else if !self.blink_visible {
            self.blink_visible = true;
            cx.notify();
        }
    }

    fn pause_blink(&mut self) {
        self.blink_visible = true;
        self.blink_paused_until = Some(Instant::now() + BLINK_INTERVAL);
    }

    /// Local keystrokes (§6.5): copy/paste chords first, then the §6.8 key
    /// table straight to the shared writer.
    fn handle_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.exit.is_some() {
            return;
        }
        let keystroke = &event.keystroke;
        if is_copy_chord(keystroke) {
            if self.copy_selection(cx) {
                cx.stop_propagation();
            }
            return;
        }
        if is_paste_chord(keystroke) {
            self.paste_clipboard(cx);
            cx.stop_propagation();
            return;
        }
        let mode = self.term_mode();
        if let Some(bytes) = keys::to_esc_str(keystroke, &mode, self.alt_is_meta) {
            self.send_input(&bytes, cx);
            cx.stop_propagation();
        }
    }

    /// Write user input: clears the local selection, snaps scrollback to the
    /// bottom, and resets the cursor blink — like every terminal.
    fn send_input(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        {
            let session = self.session.borrow();
            session.write(bytes);
            let term = session.term();
            let mut term = term.lock();
            term.selection = None;
            if term.grid().display_offset() != 0 {
                term.scroll_display(Scroll::Bottom);
            }
        }
        self.selecting = false;
        self.pause_blink();
        cx.notify();
    }

    /// Selection → clipboard (local only — never OSC-52, §6.9/§6.15).
    fn copy_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let text = {
            let term = self.session.borrow().term();
            let text = term.lock().selection_to_string();
            text
        };
        match text {
            Some(text) if !text.is_empty() => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                true
            }
            _ => false,
        }
    }

    /// Paste (§6.5): bracketed when the child requested it — the session
    /// checks `TermMode::BRACKETED_PASTE`.
    fn paste_clipboard(&mut self, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            if !text.is_empty() {
                self.session.borrow().paste(&text);
                self.pause_blink();
                cx.notify();
            }
        }
    }

    /// IME commit (`insertText:`): typed/composed text goes to the PTY like
    /// any keystroke.
    fn ime_commit(&mut self, text: &str, cx: &mut Context<Self>) {
        self.ime_marked = None;
        if !text.is_empty() {
            self.send_input(text.as_bytes(), cx);
        } else {
            cx.notify();
        }
    }

    // -- Mouse (wired by the element's paint-phase listeners) ---------------

    fn mouse_down(
        &mut self,
        event: &MouseDownEvent,
        geometry: &GridGeometry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Click-to-focus, always.
        window.focus(&self.focus_handle, cx);

        let mode = self.term_mode();
        let (cell, grid_point, side) = geometry.hit(event.position);

        if mouse::should_report(MouseEventKind::Press, false, event.modifiers.shift, &mode) {
            if let Some(code) = mouse::button_code(event.button) {
                if let Some(report) =
                    mouse::mouse_report(code, MouseEventKind::Press, cell, &event.modifiers, &mode)
                {
                    self.session.borrow().write(&report);
                }
                self.mouse_down_reported = true;
                self.last_motion_cell = Some(cell);
            }
            return;
        }

        if event.button == MouseButton::Left {
            let ty = match event.click_count {
                1 => SelectionType::Simple,
                2 => SelectionType::Semantic,
                _ => SelectionType::Lines,
            };
            {
                let term = self.session.borrow().term();
                term.lock().selection = Some(Selection::new(ty, grid_point, side));
            }
            self.selecting = true;
            cx.notify();
        }
    }

    fn mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        geometry: &GridGeometry,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        // Local selection drag wins (it only starts when reporting is off).
        if self.selecting && event.pressed_button == Some(MouseButton::Left) {
            let (_, grid_point, side) = geometry.hit(event.position);
            {
                let term = self.session.borrow().term();
                let mut term = term.lock();
                if let Some(selection) = term.selection.as_mut() {
                    selection.update(grid_point, side);
                }
            }
            cx.notify();
            return;
        }

        if !hovered {
            return;
        }
        let mode = self.term_mode();
        let button_held = event.pressed_button.is_some();
        if mouse::should_report(MouseEventKind::Motion, button_held, event.modifiers.shift, &mode) {
            let (cell, _, _) = geometry.hit(event.position);
            if self.last_motion_cell != Some(cell) {
                // Motion without a button reports code 3 (xterm any-motion).
                let code = event
                    .pressed_button
                    .and_then(mouse::button_code)
                    .unwrap_or(3);
                if let Some(report) =
                    mouse::mouse_report(code, MouseEventKind::Motion, cell, &event.modifiers, &mode)
                {
                    self.session.borrow().write(&report);
                }
                self.last_motion_cell = Some(cell);
            }
        }
    }

    fn mouse_up(
        &mut self,
        event: &MouseUpEvent,
        geometry: &GridGeometry,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if self.mouse_down_reported {
            let mode = self.term_mode();
            if mode.intersects(TermMode::MOUSE_MODE) {
                let (cell, _, _) = geometry.hit(event.position);
                if let Some(code) = mouse::button_code(event.button) {
                    if let Some(report) = mouse::mouse_report(
                        code,
                        MouseEventKind::Release,
                        cell,
                        &event.modifiers,
                        &mode,
                    ) {
                        self.session.borrow().write(&report);
                    }
                }
            }
            self.mouse_down_reported = false;
            self.last_motion_cell = None;
        }
        self.selecting = false;
    }

    /// Wheel: scrollback via the grid display offset (§6.9), wheel reports
    /// when the TUI grabbed the mouse, alternate-scroll arrows in the alt
    /// screen — shift always forces local scrollback.
    fn scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        geometry: &GridGeometry,
        cx: &mut Context<Self>,
    ) {
        let line_height = geometry.line_height;
        let delta = event.delta.pixel_delta(line_height).y;
        self.scroll_accum += f32::from(delta) / f32::from(line_height).max(1.0);
        let lines = self.scroll_accum.trunc() as i32;
        if lines == 0 {
            return;
        }
        self.scroll_accum -= lines as f32;

        let mode = self.term_mode();
        let shift = event.modifiers.shift;
        if !shift && mode.intersects(TermMode::MOUSE_MODE) {
            let (cell, _, _) = geometry.hit(event.position);
            let reports = mouse::wheel_reports(lines, cell, &event.modifiers, &mode);
            if !reports.is_empty() {
                self.session.borrow().write(&reports);
            }
        } else if !shift
            && mode.contains(TermMode::ALT_SCREEN)
            && mode.contains(TermMode::ALTERNATE_SCROLL)
        {
            let reports = mouse::alt_scroll_reports(lines, &mode);
            self.session.borrow().write(&reports);
        } else {
            let term = self.session.borrow().term();
            term.lock().scroll_display(Scroll::Delta(lines));
            cx.notify();
        }
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EventEmitter<TerminalViewEvent> for TerminalView {}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.focus_handle.is_focused(window);
        div()
            .id("terminal-view")
            .key_context("Terminal")
            .track_focus(&self.focus_handle)
            .size_full()
            .bg(self.palette.background)
            .on_key_down(cx.listener(Self::handle_key_down))
            .child(TerminalElement {
                view: cx.entity(),
                session: self.session.clone(),
                focus_handle: self.focus_handle.clone(),
                palette: self.palette,
                focused,
                cursor_blink_show: self.blink_visible,
                ime_marked: self.ime_marked.clone(),
                cursor_bounds_slot: self.cursor_bounds.clone(),
            })
    }
}

/// Copy chord: cmd-c on macOS, ctrl-shift-c elsewhere (ctrl-c must stay
/// SIGINT).
fn is_copy_chord(keystroke: &gpui::Keystroke) -> bool {
    if cfg!(target_os = "macos") {
        keystroke.modifiers.platform && !keystroke.modifiers.shift && keystroke.key == "c"
    } else {
        keystroke.modifiers.control && keystroke.modifiers.shift && keystroke.key == "c"
    }
}

/// Paste chord: cmd-v on macOS, ctrl-shift-v elsewhere.
fn is_paste_chord(keystroke: &gpui::Keystroke) -> bool {
    if cfg!(target_os = "macos") {
        keystroke.modifiers.platform && !keystroke.modifiers.shift && keystroke.key == "v"
    } else {
        keystroke.modifiers.control && keystroke.modifiers.shift && keystroke.key == "v"
    }
}

// ---------------------------------------------------------------------------
// Grid geometry (shared by layout, mouse mapping, and the IME handler)
// ---------------------------------------------------------------------------

/// One frame's grid metrics — everything needed to map pixels ↔ cells.
#[derive(Debug, Clone, Copy)]
pub struct GridGeometry {
    pub origin: PixelPoint<Pixels>,
    pub cell_width: Pixels,
    pub line_height: Pixels,
    pub cols: usize,
    pub rows: usize,
    pub display_offset: usize,
}

impl GridGeometry {
    fn hit(&self, position: PixelPoint<Pixels>) -> (ViewportCell, GridPoint, alacritty_terminal::index::Side) {
        mouse::grid_cell(
            position,
            self.origin,
            self.cell_width,
            self.line_height,
            self.cols,
            self.rows,
            self.display_offset,
        )
    }

    /// Device-pixel-snapped x of a column boundary (§6.9: never let a cell
    /// origin land on a fractional pixel).
    fn snap_x(&self, col: usize) -> Pixels {
        px((f32::from(self.origin.x) + col as f32 * f32::from(self.cell_width)).floor())
    }

    fn snap_y(&self, row: usize) -> Pixels {
        px((f32::from(self.origin.y) + row as f32 * f32::from(self.line_height)).floor())
    }

    /// Snapped bounds spanning `width_cells` columns at (col, row). The right
    /// edge is the *next* snapped boundary so adjacent quads never gap.
    fn cell_bounds(&self, col: usize, row: usize, width_cells: usize) -> Bounds<Pixels> {
        Bounds::from_corners(
            point(self.snap_x(col), self.snap_y(row)),
            point(self.snap_x(col + width_cells), self.snap_y(row + 1)),
        )
    }
}

/// Integer grid dimensions from a pixel box (§6.9's cell layout math with
/// the `next_up().floor()` snap guarding 1-ulp float loss on exact
/// multiples). Returns (cols, rows); either may be 0 — the collapsed-dock
/// guard (§6.9) skips resize/paint instead of clamping.
pub fn grid_dims(width: f32, height: f32, cell_width: f32, line_height: f32) -> (usize, usize) {
    if width <= 0.0 || height <= 0.0 || cell_width <= 0.0 || line_height <= 0.0 {
        return (0, 0);
    }
    let cols = (width / cell_width).next_up().floor().max(0.0) as usize;
    let rows = (height / line_height).next_up().floor().max(0.0) as usize;
    (cols, rows)
}

// ---------------------------------------------------------------------------
// Content snapshot (taken under the Term lock) + pure layout passes
// ---------------------------------------------------------------------------

/// One glyph-bearing (or colored-background) cell, colors already resolved.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CellSpec {
    row: usize,
    col: usize,
    /// Columns covered: 1, or 2 for a `WIDE_CHAR` base cell (the spacer cell
    /// is dropped at snapshot time, §6.9).
    width: usize,
    /// Base char + any zero-width/combining marks folded onto it.
    text: String,
    fg: Hsla,
    /// `None` = default background (covered by the element clear).
    bg: Option<Hsla>,
    bold: bool,
    italic: bool,
    underline: bool,
    undercurl: bool,
    strikethrough: bool,
    /// `Flags::HIDDEN`: keep the background, skip the glyph.
    hidden: bool,
}

impl CellSpec {
    /// Whether this cell contributes glyphs/decorations (vs background only).
    fn has_ink(&self) -> bool {
        if self.hidden {
            return false;
        }
        self.text.trim_start().chars().next().is_some()
            || self.underline
            || self.undercurl
            || self.strikethrough
    }

    fn style_key(&self) -> (Hsla, bool, bool, bool, bool, bool) {
        (
            self.fg,
            self.bold,
            self.italic,
            self.underline,
            self.undercurl,
            self.strikethrough,
        )
    }
}

struct ContentSnapshot {
    display_offset: usize,
    selection: Option<SelectionRange>,
    cells: Vec<CellSpec>,
    /// Viewport (col, row), shape, wide flag, and the glyph under the cursor
    /// (for block inversion). `None` when hidden or scrolled off-screen.
    cursor: Option<(usize, usize, CursorShape, bool, char)>,
}

/// Copy the visible grid out of the emulator — called under the `FairMutex`,
/// kept cheap (no shaping, no allocation beyond the cell vec) so the read
/// thread is never starved (§6.11).
fn snapshot_content(
    term: &Term<EventProxy>,
    palette: &TerminalPalette,
    rows: usize,
) -> ContentSnapshot {
    let content = term.renderable_content();
    let display_offset = content.display_offset;
    let selection = content.selection;
    let cursor_point = content.cursor.point;
    let cursor_shape = content.cursor.shape;

    let mut cells = Vec::new();
    let mut cursor_char = ' ';
    let mut cursor_wide = false;

    for indexed in content.display_iter {
        let cell = indexed.cell;
        let flags = cell.flags;
        // Trailing half of a double-width glyph — never emit a glyph or
        // advance for it (§6.9); the base cell carries width 2.
        if flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let Some(row) = viewport_row(indexed.point.line.0, display_offset, rows) else {
            continue;
        };
        if indexed.point == cursor_point {
            cursor_char = cell.c;
            cursor_wide = flags.contains(Flags::WIDE_CHAR);
        }

        let inverse = flags.contains(Flags::INVERSE);
        let mut fg = resolve_color(&cell.fg, term, palette);
        let mut bg = resolve_color(&cell.bg, term, palette);
        if inverse {
            std::mem::swap(&mut fg, &mut bg);
        }
        if flags.intersects(Flags::DIM) {
            fg = Hsla {
                l: fg.l * (2.0 / 3.0),
                ..fg
            };
        }
        let bg = (bg != palette.background).then_some(bg);

        // A wrapped wide char leaves a blank leading spacer at line end.
        let leading_spacer = flags.contains(Flags::LEADING_WIDE_CHAR_SPACER);

        let mut text = String::new();
        if !leading_spacer {
            text.push(cell.c);
            if let Some(marks) = cell.zerowidth() {
                text.extend(marks);
            }
        }

        let spec = CellSpec {
            row,
            col: indexed.point.column.0,
            width: if flags.contains(Flags::WIDE_CHAR) { 2 } else { 1 },
            text,
            fg,
            bg,
            bold: flags.intersects(Flags::BOLD),
            italic: flags.contains(Flags::ITALIC),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            undercurl: flags.contains(Flags::UNDERCURL),
            strikethrough: flags.contains(Flags::STRIKEOUT),
            hidden: flags.contains(Flags::HIDDEN) || leading_spacer,
        };
        // Blank default-background cells with no decorations draw nothing.
        if spec.bg.is_none() && !spec.has_ink() {
            continue;
        }
        cells.push(spec);
    }

    let cursor = if cursor_shape == CursorShape::Hidden {
        None
    } else {
        viewport_row(cursor_point.line.0, display_offset, rows)
            .map(|row| (cursor_point.column.0, row, cursor_shape, cursor_wide, cursor_char))
    };

    ContentSnapshot {
        display_offset,
        selection,
        cells,
        cursor,
    }
}

/// Buffer line (scrollback-relative) → viewport row, `None` off-screen.
pub(crate) fn viewport_row(line: i32, display_offset: usize, rows: usize) -> Option<usize> {
    let row = line + display_offset as i32;
    (0..rows as i32).contains(&row).then_some(row as usize)
}

/// Resolve an ANSI color through the runtime color table (OSC 4/10/11
/// overrides) then the theme palette (§6.8's table).
fn resolve_color(color: &AnsiColor, term: &Term<EventProxy>, palette: &TerminalPalette) -> Hsla {
    match color {
        AnsiColor::Spec(rgb) => rgb_to_hsla(*rgb),
        AnsiColor::Named(named) => {
            if let Some(rgb) = term.colors()[*named as usize] {
                return rgb_to_hsla(rgb);
            }
            named_color(*named, palette)
        }
        AnsiColor::Indexed(index) => {
            let index = *index as usize;
            if let Some(rgb) = term.colors()[index] {
                return rgb_to_hsla(rgb);
            }
            indexed_color(index, palette)
        }
    }
}

/// `NamedColor` → theme token (§6.8's color table).
pub(crate) fn named_color(named: NamedColor, palette: &TerminalPalette) -> Hsla {
    let index = named as usize;
    match named {
        NamedColor::Foreground => palette.foreground,
        NamedColor::Background => palette.background,
        NamedColor::Cursor => palette.cursor,
        NamedColor::BrightForeground => palette.bright_foreground,
        NamedColor::DimForeground => palette.dim_foreground,
        _ if index < 16 => palette.ansi(index),
        // Dim black..dim white sit at a fixed offset in the enum.
        _ => {
            let dim_index = index.saturating_sub(NamedColor::DimBlack as usize);
            palette.dim[dim_index.min(7)]
        }
    }
}

/// xterm-256 math for `Color::Indexed` (§6.8): 16..232 = 6×6×6 cube,
/// 232..256 = 24-step grayscale ramp — reimplemented from the xterm spec.
pub(crate) fn indexed_color(index: usize, palette: &TerminalPalette) -> Hsla {
    match index {
        0..=15 => palette.ansi(index),
        16..=231 => {
            let i = index - 16;
            let channel = |n: usize| {
                if n == 0 {
                    0u8
                } else {
                    (55 + 40 * n) as u8
                }
            };
            rgb_to_hsla(Rgb {
                r: channel(i / 36),
                g: channel((i / 6) % 6),
                b: channel(i % 6),
            })
        }
        232..=255 => {
            let v = (8 + 10 * (index - 232)) as u8;
            rgb_to_hsla(Rgb { r: v, g: v, b: v })
        }
        _ => palette.foreground,
    }
}

fn rgb_to_hsla(rgb: Rgb) -> Hsla {
    gpui::Rgba {
        r: rgb.r as f32 / 255.0,
        g: rgb.g as f32 / 255.0,
        b: rgb.b as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

/// A horizontal run of same-background cells → one quad (§6.9
/// `BackgroundRegions`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BgRun {
    pub row: usize,
    pub col: usize,
    pub width: usize,
    pub color: Hsla,
}

/// Merge horizontally-adjacent same-bg cells; far fewer quads than
/// one-per-cell. Pure — unit-tested without a window.
pub(crate) fn merge_bg_runs(cells: &[CellSpec]) -> Vec<BgRun> {
    let mut runs: Vec<BgRun> = Vec::new();
    let mut open: Option<BgRun> = None;
    for cell in cells {
        let Some(color) = cell.bg else {
            if let Some(run) = open.take() {
                runs.push(run);
            }
            continue;
        };
        match &mut open {
            Some(run)
                if run.row == cell.row && run.col + run.width == cell.col && run.color == color =>
            {
                run.width += cell.width;
            }
            _ => {
                if let Some(run) = open.take() {
                    runs.push(run);
                }
                open = Some(BgRun {
                    row: cell.row,
                    col: cell.col,
                    width: cell.width,
                    color,
                });
            }
        }
    }
    if let Some(run) = open.take() {
        runs.push(run);
    }
    runs
}

/// A batched same-style text run (§6.9 `BatchedTextRuns`), ready to shape
/// with the forced cell advance.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GlyphRun {
    pub row: usize,
    pub col: usize,
    pub text: String,
    pub fg: Hsla,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub undercurl: bool,
    pub strikethrough: bool,
}

/// Batch consecutive same-style cells into runs. Wide (CJK/emoji) cells are
/// emitted as their **own single-glyph run**: the forced `Some(cell_width)`
/// advance would otherwise squeeze a 2-cell glyph's successor into 1 cell.
/// Pure — unit-tested without a window.
pub(crate) fn batch_glyph_runs(cells: &[CellSpec]) -> Vec<GlyphRun> {
    let mut runs: Vec<GlyphRun> = Vec::new();
    let mut open: Option<(GlyphRun, usize)> = None; // (run, next expected col)

    for cell in cells {
        if !cell.has_ink() {
            // Background-only cell: a style gap — close any open run.
            if let Some((run, _)) = open.take() {
                runs.push(run);
            }
            continue;
        }
        let is_wide = cell.width == 2;
        let matches_open = open.as_ref().is_some_and(|(run, next_col)| {
            !is_wide
                && run.row == cell.row
                && *next_col == cell.col
                && (run.fg, run.bold, run.italic, run.underline, run.undercurl, run.strikethrough)
                    == cell.style_key()
        });
        if matches_open {
            let (run, next_col) = open.as_mut().expect("checked above");
            run.text.push_str(&cell.text);
            *next_col += cell.width;
        } else {
            if let Some((run, _)) = open.take() {
                runs.push(run);
            }
            let run = GlyphRun {
                row: cell.row,
                col: cell.col,
                text: cell.text.clone(),
                fg: cell.fg,
                bold: cell.bold,
                italic: cell.italic,
                underline: cell.underline,
                undercurl: cell.undercurl,
                strikethrough: cell.strikethrough,
            };
            if is_wide {
                // Isolate the wide glyph; the next run starts fresh after it.
                runs.push(run);
            } else {
                open = Some((run, cell.col + cell.width));
            }
        }
    }
    if let Some((run, _)) = open.take() {
        runs.push(run);
    }
    runs
}

/// Selection → per-row (row, start col, inclusive end col) bands.
/// Pure — unit-tested without a window.
pub(crate) fn selection_row_spans(
    selection: &SelectionRange,
    display_offset: usize,
    cols: usize,
    rows: usize,
) -> Vec<(usize, usize, usize)> {
    let mut spans = Vec::new();
    if cols == 0 {
        return spans;
    }
    for row in 0..rows {
        let line = row as i32 - display_offset as i32;
        if line < selection.start.line.0 || line > selection.end.line.0 {
            continue;
        }
        let start = if selection.is_block || line == selection.start.line.0 {
            selection.start.column.0
        } else {
            0
        };
        let end = if selection.is_block || line == selection.end.line.0 {
            selection.end.column.0
        } else {
            cols - 1
        };
        if start <= end {
            spans.push((row, start.min(cols - 1), end.min(cols - 1)));
        }
    }
    spans
}

// ---------------------------------------------------------------------------
// The Element
// ---------------------------------------------------------------------------

/// The §6.9 grid element. Constructed fresh every `TerminalView::render`.
pub struct TerminalElement {
    view: Entity<TerminalView>,
    session: Rc<RefCell<Terminal>>,
    focus_handle: FocusHandle,
    palette: TerminalPalette,
    focused: bool,
    /// Blink-resolved: false = the blink task currently hides the cursor.
    cursor_blink_show: bool,
    ime_marked: Option<String>,
    cursor_bounds_slot: Rc<StdCell<Option<Bounds<Pixels>>>>,
}

struct CursorLayout {
    /// Shape-specific bounds (2px bar for beam/underline, cell for block).
    bounds: Bounds<Pixels>,
    /// Full cell bounds — the hollow unfocused cursor always outlines these.
    cell_bounds: Bounds<Pixels>,
    shape: CursorShape,
    /// Block-cursor glyph repainted in the inverted color (§6.9).
    glyph: Option<(PixelPoint<Pixels>, ShapedLine)>,
}

/// Everything computed in prepaint, consumed by paint.
pub struct TerminalLayout {
    hitbox: Hitbox,
    geometry: GridGeometry,
    bg_quads: Vec<(Bounds<Pixels>, Hsla)>,
    selection_quads: Vec<Bounds<Pixels>>,
    text_runs: Vec<(PixelPoint<Pixels>, ShapedLine)>,
    cursor: Option<CursorLayout>,
    ime: Option<(Bounds<Pixels>, PixelPoint<Pixels>, ShapedLine)>,
}

impl TerminalElement {
    fn base_font(&self) -> Font {
        Font {
            family: FONT_FAMILY.into(),
            features: Default::default(),
            fallbacks: None,
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        }
    }
}

fn run_font(base: &Font, bold: bool, italic: bool) -> Font {
    let mut font = base.clone();
    if bold {
        font.weight = FontWeight::BOLD;
    }
    if italic {
        font.style = FontStyle::Italic;
    }
    font
}

impl IntoElement for TerminalElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TerminalElement {
    type RequestLayoutState = ();
    type PrepaintState = TerminalLayout;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // Fill whatever box the dock/tab gives us; the grid adapts (§6.10).
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();
        style.flex_grow = 1.0;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let text_system = window.text_system().clone();

        // -- Cell metrics (§6.9) --------------------------------------------
        let base_font = self.base_font();
        let font_size = px(FONT_SIZE);
        let font_id = text_system.resolve_font(&base_font);
        let cell_width = text_system
            .advance(font_id, font_size, 'm')
            .map(|advance| advance.width)
            .unwrap_or(font_size * 0.6);
        let line_height = px((FONT_SIZE * LINE_HEIGHT).round());

        let origin = bounds.origin + point(px(PAD_X), px(PAD_Y));
        let content_width = f32::from(bounds.size.width) - PAD_X * 2.0;
        let content_height = f32::from(bounds.size.height) - PAD_Y * 2.0;
        let (cols, rows) = grid_dims(
            content_width,
            content_height,
            f32::from(cell_width),
            f32::from(line_height),
        );

        let mut geometry = GridGeometry {
            origin,
            cell_width,
            line_height,
            cols,
            rows,
            display_offset: 0,
        };

        // §6.9: 0-height docked panel (collapsed dock) — skip the PTY resize
        // and paint nothing rather than thrash the child with a zero grid.
        if cols == 0 || rows == 0 {
            return TerminalLayout {
                hitbox,
                geometry,
                bg_quads: Vec::new(),
                selection_quads: Vec::new(),
                text_runs: Vec::new(),
                cursor: None,
                ime: None,
            };
        }

        // -- §6.10 resize pair: Term reflow + PTY TIOCSWINSZ (SIGWINCH), only
        //    on integer cell change (Terminal::resize dedupes) ---------------
        {
            let mut session = self.session.borrow_mut();
            if let Err(error) = session.resize(cols as u16, rows as u16) {
                log::warn!("terminal resize to {cols}x{rows}: {error}");
            }
        }

        // -- Snapshot the grid under the FairMutex (held briefly, §6.11) ----
        let snapshot = {
            let term = self.session.borrow().term();
            let term = term.lock();
            snapshot_content(&term, &self.palette, rows)
        };
        geometry.display_offset = snapshot.display_offset;

        // -- Batched draw lists (§6.9) --------------------------------------
        let bg_quads = merge_bg_runs(&snapshot.cells)
            .into_iter()
            .map(|run| (geometry.cell_bounds(run.col, run.row, run.width), run.color))
            .collect();

        let selection_quads = snapshot
            .selection
            .as_ref()
            .map(|selection| {
                selection_row_spans(selection, snapshot.display_offset, cols, rows)
                    .into_iter()
                    .map(|(row, start, end)| geometry.cell_bounds(start, row, end - start + 1))
                    .collect()
            })
            .unwrap_or_default();

        let mut text_runs = Vec::new();
        for run in batch_glyph_runs(&snapshot.cells) {
            let font = run_font(&base_font, run.bold, run.italic);
            let underline = run.underline.then(|| UnderlineStyle {
                thickness: px(1.0),
                color: Some(run.fg),
                wavy: run.undercurl,
            });
            let strikethrough = run.strikethrough.then(|| StrikethroughStyle {
                thickness: px(1.0),
                color: Some(run.fg),
            });
            let text: SharedString = run.text.into();
            let text_run = TextRun {
                len: text.len(),
                font,
                color: run.fg,
                background_color: None,
                underline,
                strikethrough,
            };
            // `Some(cell_width)` forces the monospace advance so glyphs land
            // on exact cell boundaries (§6.9) — without it, fallback-font
            // glyphs drift and the grid smears.
            let line = text_system.shape_line(text, font_size, &[text_run], Some(cell_width));
            text_runs.push((geometry.cell_bounds(run.col, run.row, 1).origin, line));
        }

        // -- Cursor (§6.9): block/beam/underline, hollow when unfocused -----
        let cursor = snapshot.cursor.map(|(col, row, shape, wide, ch)| {
            let width_cells = if wide { 2 } else { 1 };
            let cell = geometry.cell_bounds(col, row, width_cells);
            let bounds = match shape {
                CursorShape::Beam => Bounds::new(cell.origin, gpui::size(px(2.0), cell.size.height)),
                CursorShape::Underline => Bounds::new(
                    point(cell.origin.x, cell.origin.y + cell.size.height - px(2.0)),
                    gpui::size(cell.size.width, px(2.0)),
                ),
                _ => cell,
            };
            // The glyph under a filled block is repainted inverted.
            let glyph = (shape == CursorShape::Block && self.focused && ch != ' ')
                .then(|| {
                    let text: SharedString = ch.to_string().into();
                    let text_run = TextRun {
                        len: text.len(),
                        font: base_font.clone(),
                        color: self.palette.cursor_text,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line =
                        text_system.shape_line(text, font_size, &[text_run], Some(cell_width));
                    (cell.origin, line)
                });
            CursorLayout {
                bounds,
                cell_bounds: cell,
                shape,
                glyph,
            }
        });

        // -- IME composing text, drawn at the cursor (§6.9 input) -----------
        let ime = self
            .ime_marked
            .as_ref()
            .filter(|marked| !marked.is_empty())
            .and_then(|marked| {
                let (col, row, ..) = snapshot.cursor?;
                let text: SharedString = SharedString::from(marked.clone());
                let text_run = TextRun {
                    len: text.len(),
                    font: base_font.clone(),
                    color: self.palette.foreground,
                    background_color: None,
                    underline: Some(UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(self.palette.foreground),
                        wavy: false,
                    }),
                    strikethrough: None,
                };
                let line = text_system.shape_line(text, font_size, &[text_run], Some(cell_width));
                let cells = (f32::from(line.width()) / f32::from(cell_width)).ceil() as usize;
                let origin = geometry.cell_bounds(col, row, 1).origin;
                let bounds = geometry.cell_bounds(col, row, cells.max(1).min(cols - col.min(cols - 1)));
                Some((bounds, origin, line))
            });

        TerminalLayout {
            hitbox,
            geometry,
            bg_quads,
            selection_quads,
            text_runs,
            cursor,
            ime,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let palette = self.palette;
        let line_height = layout.geometry.line_height;

        // Clear to the terminal background (the default-bg cells rely on it).
        window.paint_quad(fill(bounds, palette.background));

        if layout.geometry.cols == 0 || layout.geometry.rows == 0 {
            return; // collapsed dock — nothing else to do (§6.9)
        }

        for (quad_bounds, color) in &layout.bg_quads {
            window.paint_quad(fill(*quad_bounds, *color));
        }
        for selection_bounds in &layout.selection_quads {
            window.paint_quad(fill(*selection_bounds, palette.selection));
        }
        for (origin, line) in &layout.text_runs {
            let _ = line.paint(*origin, line_height, TextAlign::default(), None, window, cx);
        }

        // Cursor: filled when focused & blink-visible, hollow when unfocused.
        let mut cursor_pixel_bounds = None;
        if let Some(cursor) = &layout.cursor {
            cursor_pixel_bounds = Some(cursor.bounds);
            let show_filled = self.focused && self.cursor_blink_show;
            match cursor.shape {
                CursorShape::Hidden => {}
                // Unfocused terminal: hollow the block — always the full
                // cell, whatever shape the child asked for (§6.9).
                _ if !self.focused => {
                    window.paint_quad(outline(
                        cursor.cell_bounds,
                        palette.cursor,
                        BorderStyle::Solid,
                    ));
                }
                CursorShape::HollowBlock => {
                    window.paint_quad(outline(
                        cursor.cell_bounds,
                        palette.cursor,
                        BorderStyle::Solid,
                    ));
                }
                _ if show_filled => {
                    window.paint_quad(fill(cursor.bounds, palette.cursor));
                    if let Some((origin, line)) = &cursor.glyph {
                        let _ =
                            line.paint(*origin, line_height, TextAlign::default(), None, window, cx);
                    }
                }
                // Blink-hidden phase.
                _ => {}
            }
        }

        // IME composition overlay at the cursor.
        if let Some((ime_bounds, origin, line)) = &layout.ime {
            window.paint_quad(fill(*ime_bounds, palette.background));
            let _ = line.paint(*origin, line_height, TextAlign::default(), None, window, cx);
            cursor_pixel_bounds = Some(*ime_bounds);
        }

        // IME candidate-window anchor (bounds_for_range).
        self.cursor_bounds_slot.set(cursor_pixel_bounds);

        window.set_cursor_style(GpuiCursorStyle::IBeam, &layout.hitbox);

        // Route platform text input (typed chars + IME composition) to the
        // PTY while this terminal owns focus.
        window.handle_input(
            &self.focus_handle,
            TerminalInputHandler {
                view: self.view.clone(),
                cursor_bounds: self.cursor_bounds_slot.clone(),
            },
            cx,
        );

        // -- Mouse (paint-phase listeners, §6.8 mouse subsection) -----------
        let geometry = layout.geometry;

        let view = self.view.clone();
        let hitbox = layout.hitbox.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                return;
            }
            view.update(cx, |view, cx| view.mouse_down(event, &geometry, window, cx));
        });

        let view = self.view.clone();
        let hitbox = layout.hitbox.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            let hovered = hitbox.is_hovered(window);
            view.update(cx, |view, cx| view.mouse_move(event, &geometry, hovered, cx));
        });

        let view = self.view.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            view.update(cx, |view, cx| view.mouse_up(event, &geometry, window, cx));
        });

        let view = self.view.clone();
        let hitbox = layout.hitbox.clone();
        window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !hitbox.is_hovered(window) {
                return;
            }
            view.update(cx, |view, cx| view.scroll_wheel(event, &geometry, cx));
        });
    }
}

// ---------------------------------------------------------------------------
// IME / platform text input (§6.9 input, task item 4)
// ---------------------------------------------------------------------------

/// Minimal terminal [`InputHandler`]: a terminal has no addressable document,
/// so ranges are `None`; committed text goes to the PTY, composing (marked)
/// text is drawn at the cursor by the element, and `bounds_for_range` anchors
/// the IME candidate window at the cursor. `prefers_ime_for_printable_keys`
/// keeps its default `false` so raw keys reach the child (the gpui trait
/// documents exactly this terminal case).
struct TerminalInputHandler {
    view: Entity<TerminalView>,
    cursor_bounds: Rc<StdCell<Option<Bounds<Pixels>>>>,
}

impl InputHandler for TerminalInputHandler {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<UTF16Selection> {
        None
    }

    fn marked_text_range(&mut self, _window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.view
            .read(cx)
            .ime_marked
            .as_ref()
            .map(|marked| 0..marked.encode_utf16().count())
    }

    fn text_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<String> {
        None
    }

    fn replace_text_in_range(
        &mut self,
        _replacement_range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut App,
    ) {
        let text = text.to_owned();
        self.view.update(cx, |view, cx| view.ime_commit(&text, cx));
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range_utf16: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) {
        let marked = (!new_text.is_empty()).then(|| new_text.to_owned());
        self.view.update(cx, |view, cx| {
            view.ime_marked = marked;
            cx.notify();
        });
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut App) {
        self.view.update(cx, |view, cx| {
            view.ime_marked = None;
            cx.notify();
        });
    }

    fn bounds_for_range(
        &mut self,
        _range_utf16: Range<usize>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.cursor_bounds.get()
    }

    fn character_index_for_point(
        &mut self,
        _point: PixelPoint<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        None
    }
}

// ---------------------------------------------------------------------------
// Tests — pure cell math / batching / color mapping (no Window required)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::index::{Column, Line};

    fn spec(row: usize, col: usize, ch: char, fg: Hsla, bg: Option<Hsla>) -> CellSpec {
        CellSpec {
            row,
            col,
            width: 1,
            text: ch.to_string(),
            fg,
            bg,
            bold: false,
            italic: false,
            underline: false,
            undercurl: false,
            strikethrough: false,
            hidden: false,
        }
    }

    fn white() -> Hsla {
        gpui::white()
    }

    fn red() -> Hsla {
        gpui::red()
    }

    #[test]
    fn grid_dims_floors_partial_cells() {
        assert_eq!(grid_dims(800.0, 240.0, 8.0, 16.0), (100, 15));
        assert_eq!(grid_dims(639.9, 240.0, 8.0, 16.0), (79, 15));
        assert_eq!(grid_dims(647.9, 240.0, 8.0, 16.0), (80, 15));
    }

    #[test]
    fn grid_dims_exact_multiples_survive_f32_loss() {
        // §6.9 device-pixel snapping: the next_up() guard keeps an exact
        // multiple from losing a row/column to 1-ulp float error.
        for n in 1..200usize {
            let cell = 7.2f32;
            let width = cell * n as f32;
            let (cols, _) = grid_dims(width, 100.0, cell, 16.0);
            assert_eq!(cols, n, "width {width} / cell {cell}");
        }
    }

    #[test]
    fn grid_dims_zero_box_is_zero_not_clamped() {
        assert_eq!(grid_dims(0.0, 100.0, 8.0, 16.0), (0, 0));
        assert_eq!(grid_dims(100.0, 0.0, 8.0, 16.0), (0, 0));
        assert_eq!(grid_dims(100.0, 10.0, 8.0, 16.0), (12, 0));
    }

    #[test]
    fn viewport_row_maps_scrollback() {
        // No scrollback: buffer line == viewport row.
        assert_eq!(viewport_row(0, 0, 24), Some(0));
        assert_eq!(viewport_row(23, 0, 24), Some(23));
        assert_eq!(viewport_row(24, 0, 24), None);
        // Scrolled back 5: history line -5 is the top row.
        assert_eq!(viewport_row(-5, 5, 24), Some(0));
        assert_eq!(viewport_row(0, 5, 24), Some(5));
        // The active bottom rows fall off-screen while scrolled back.
        assert_eq!(viewport_row(20, 5, 24), None);
    }

    #[test]
    fn merge_bg_adjacent_same_color() {
        let bg = Some(red());
        let cells = vec![
            spec(0, 0, 'a', white(), bg),
            spec(0, 1, 'b', white(), bg),
            spec(0, 2, 'c', white(), bg),
        ];
        let runs = merge_bg_runs(&cells);
        assert_eq!(runs.len(), 1);
        assert_eq!((runs[0].col, runs[0].width), (0, 3));
    }

    #[test]
    fn merge_bg_breaks_on_gap_color_and_row() {
        let cells = vec![
            spec(0, 0, 'a', white(), Some(red())),
            spec(0, 1, 'b', white(), None), // default bg gap
            spec(0, 2, 'c', white(), Some(red())),
            spec(0, 3, 'd', white(), Some(white())), // color change
            spec(1, 4, 'e', white(), Some(white())), // row change
        ];
        let runs = merge_bg_runs(&cells);
        assert_eq!(runs.len(), 4);
        assert_eq!((runs[0].col, runs[0].width), (0, 1));
        assert_eq!((runs[1].col, runs[1].width), (2, 1));
        assert_eq!((runs[2].col, runs[2].width), (3, 1));
        assert_eq!((runs[3].row, runs[3].col), (1, 4));
    }

    #[test]
    fn merge_bg_wide_cell_covers_two_columns() {
        let mut wide = spec(0, 0, '你', white(), Some(red()));
        wide.width = 2;
        let cells = vec![wide, spec(0, 2, 'a', white(), Some(red()))];
        let runs = merge_bg_runs(&cells);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].width, 3);
    }

    #[test]
    fn batch_same_style_merges_and_style_change_splits() {
        let cells = vec![
            spec(0, 0, 'h', white(), None),
            spec(0, 1, 'i', white(), None),
            spec(0, 2, '!', red(), None), // fg change
        ];
        let runs = batch_glyph_runs(&cells);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "hi");
        assert_eq!((runs[0].row, runs[0].col), (0, 0));
        assert_eq!(runs[1].text, "!");
        assert_eq!(runs[1].col, 2);
    }

    #[test]
    fn batch_bold_splits_run() {
        let mut bold = spec(0, 1, 'b', white(), None);
        bold.bold = true;
        let cells = vec![spec(0, 0, 'a', white(), None), bold];
        let runs = batch_glyph_runs(&cells);
        assert_eq!(runs.len(), 2);
        assert!(runs[1].bold);
    }

    #[test]
    fn batch_wide_char_is_isolated_and_next_run_starts_after_spacer() {
        // "你a" — the wide char covers cols 0..2, 'a' sits at col 2.
        let mut wide = spec(0, 0, '你', white(), None);
        wide.width = 2;
        let cells = vec![wide, spec(0, 2, 'a', white(), None)];
        let runs = batch_glyph_runs(&cells);
        assert_eq!(runs.len(), 2, "wide glyph must be its own run (§6.9)");
        assert_eq!(runs[0].text, "你");
        assert_eq!(runs[0].col, 0);
        assert_eq!(runs[1].text, "a");
        assert_eq!(runs[1].col, 2);
    }

    #[test]
    fn batch_folds_combining_marks_onto_base() {
        let mut base = spec(0, 0, 'e', white(), None);
        base.text.push('\u{0301}'); // combining acute
        let cells = vec![base, spec(0, 1, 'x', white(), None)];
        let runs = batch_glyph_runs(&cells);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "e\u{0301}x");
    }

    #[test]
    fn batch_row_change_splits() {
        let cells = vec![spec(0, 0, 'a', white(), None), spec(1, 0, 'b', white(), None)];
        let runs = batch_glyph_runs(&cells);
        assert_eq!(runs.len(), 2);
    }

    #[test]
    fn batch_column_gap_splits() {
        let cells = vec![spec(0, 0, 'a', white(), None), spec(0, 5, 'b', white(), None)];
        let runs = batch_glyph_runs(&cells);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[1].col, 5);
    }

    #[test]
    fn hidden_cells_have_no_ink() {
        let mut hidden = spec(0, 0, 'x', white(), Some(red()));
        hidden.hidden = true;
        assert!(!hidden.has_ink());
        let runs = batch_glyph_runs(&[hidden.clone()]);
        assert!(runs.is_empty());
        // …but the background still paints.
        assert_eq!(merge_bg_runs(&[hidden]).len(), 1);
    }

    #[test]
    fn selection_spans_single_and_multi_row() {
        let sel = SelectionRange::new(
            GridPoint::new(Line(0), Column(2)),
            GridPoint::new(Line(2), Column(4)),
            false,
        );
        let spans = selection_row_spans(&sel, 0, 10, 24);
        assert_eq!(
            spans,
            vec![(0, 2, 9), (1, 0, 9), (2, 0, 4)],
            "first row from start col, middle rows full, last row to end col"
        );

        let block = SelectionRange::new(
            GridPoint::new(Line(1), Column(2)),
            GridPoint::new(Line(3), Column(5)),
            true,
        );
        let spans = selection_row_spans(&block, 0, 10, 24);
        assert_eq!(spans, vec![(1, 2, 5), (2, 2, 5), (3, 2, 5)]);
    }

    #[test]
    fn selection_spans_respect_display_offset() {
        // Selection on buffer line 0 while scrolled back 3 → viewport row 3.
        let sel = SelectionRange::new(
            GridPoint::new(Line(0), Column(0)),
            GridPoint::new(Line(0), Column(2)),
            false,
        );
        let spans = selection_row_spans(&sel, 3, 10, 24);
        assert_eq!(spans, vec![(3, 0, 2)]);
    }

    #[test]
    fn indexed_color_cube_and_grayscale() {
        let palette = terminal_palette();
        // 196 = pure red corner of the 6×6×6 cube.
        let red_hsla = indexed_color(196, &palette);
        let expected: Hsla = gpui::Rgba {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }
        .into();
        assert_eq!(red_hsla, expected);
        // 232 = darkest gray (8,8,8): near-black, no saturation.
        let gray = indexed_color(232, &palette);
        assert!(gray.s < 1e-4 && gray.l < 0.05);
        // 0..15 defer to the theme table.
        assert_eq!(indexed_color(1, &palette), palette.ansi(1));
    }

    #[test]
    fn named_color_maps_through_palette() {
        let palette = terminal_palette();
        assert_eq!(named_color(NamedColor::Foreground, &palette), palette.foreground);
        assert_eq!(named_color(NamedColor::Background, &palette), palette.background);
        assert_eq!(named_color(NamedColor::Cursor, &palette), palette.cursor);
        assert_eq!(named_color(NamedColor::Red, &palette), palette.ansi(1));
        assert_eq!(named_color(NamedColor::BrightBlue, &palette), palette.ansi(12));
        assert_eq!(named_color(NamedColor::DimRed, &palette), palette.dim[1]);
    }

    #[test]
    fn snapshot_skips_wide_spacers_and_resolves_colors() {
        // Integration-ish: feed the emulator CJK + colored text and check
        // the snapshot the element would lay out (no Window involved).
        use crate::emulator::Emulator;
        use vte::ansi::{Processor, StdSyncHandler};

        let emulator = Emulator::new(20, 4);
        {
            let term = emulator.term();
            let mut term = term.lock();
            let mut processor = Processor::<StdSyncHandler>::new();
            processor.advance(&mut *term, b"\x1b[31mred\x1b[0m \xe4\xbd\xa0a");
        }
        let palette = terminal_palette();
        let term = emulator.term();
        let term = term.lock();
        let snapshot = snapshot_content(&term, &palette, 4);

        let runs = batch_glyph_runs(&snapshot.cells);
        // "red" (colored) / "你" (wide, isolated) / "a" (after the spacer).
        assert_eq!(runs.len(), 3, "runs: {runs:?}");
        assert_eq!(runs[0].text, "red");
        assert_eq!(runs[0].fg, palette.ansi(1));
        assert_eq!(runs[1].text, "你");
        assert_eq!(runs[1].col, 4);
        assert_eq!(runs[2].text, "a");
        assert_eq!(runs[2].col, 6, "glyph after the wide char sits 2 cells on");

        // Cursor: after 'a' at col 7, row 0, default block.
        let (col, row, shape, wide, _) = snapshot.cursor.expect("cursor visible");
        assert_eq!((col, row), (7, 0));
        assert_eq!(shape, CursorShape::Block);
        assert!(!wide);
    }

    #[test]
    fn snapshot_inverse_swaps_colors() {
        use crate::emulator::Emulator;
        use vte::ansi::{Processor, StdSyncHandler};

        let emulator = Emulator::new(20, 4);
        {
            let term = emulator.term();
            let mut term = term.lock();
            let mut processor = Processor::<StdSyncHandler>::new();
            processor.advance(&mut *term, b"\x1b[7mX");
        }
        let palette = terminal_palette();
        let term = emulator.term();
        let term = term.lock();
        let snapshot = snapshot_content(&term, &palette, 4);
        let cell = snapshot
            .cells
            .iter()
            .find(|c| c.text == "X")
            .expect("inverse cell present");
        assert_eq!(cell.fg, palette.background);
        assert_eq!(cell.bg, Some(palette.foreground));
    }
}
