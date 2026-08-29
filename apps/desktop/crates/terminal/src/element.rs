// Clean reimplementation from the VT spec + rio-vt (MIT). NOT derived from Zed's GPL terminal crates.
//! The gpui grid element (masterplan-v3 §6.9) + the `TerminalView` entity
//! that owns a [`Terminal`] session on the foreground.
//!
//! `TerminalElement` is a low-level [`gpui::Element`] (not a composed `div()`
//! tree) with the three-phase `request_layout → prepaint → paint` lifecycle,
//! doing all its own painting. Zed's GPL `terminal_element.rs` was studied
//! for *approach only* (§0.7's licensing boundary); everything here is
//! written against the rio-vt grid API + the gpui `Element` trait.
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
//!   input, and the 0-height collapsed-dock guard;
//! - inline images (EXP-636): sixel/iTerm2 and kitty placements from the
//!   emulator's graphics store, resolved against this frame's cell stride
//!   and painted as textured quads under (sixel, kitty `z < 0`) or over
//!   (kitty `z >= 0`) the text, clipped to the element.

use crate::emulator::{EmulatorSignal, GraphicsUpdate, Term, TermMode};
use crate::keys;
use crate::mouse::{self, MouseEventKind, ViewportCell};
use crate::pty::ChildExit;
use crate::session::Terminal;
use gpui::{
    div, fill, outline, point, px, relative, App, BorderStyle, Bounds, ClipboardItem, ContentMask,
    Context, Corners, CursorStyle as GpuiCursorStyle, DispatchPhase, Element, ElementId, Entity,
    EventEmitter, FocusHandle, Focusable, Font, FontStyle, FontWeight, GlobalElementId, Hitbox,
    HitboxBehavior, Hsla, ImageId, InputHandler, InspectorElementId, InteractiveElement, IntoElement,
    KeyBinding, KeyDownEvent, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    NoAction, ParentElement, Pixels, Point as PixelPoint, Render, RenderImage, ScrollWheelEvent,
    ShapedLine, SharedString, StrikethroughStyle, Style, Styled, Task, TextAlign, TextRun,
    UTF16Selection, UnderlineStyle, Window,
};
use rio_graphics::{kitty_image_key, ColorType, GraphicData};
use rio_vt::ansi::graphics::{atlas_overlay_geometry, kitty_overlay_geometry, OverlayViewport};
use rio_vt::ansi::CursorShape;
use rio_vt::config::colors::{AnsiColor, ColorRgb, NamedColor};
use rio_vt::crosswords::grid::Scroll;
use rio_vt::crosswords::pos::{Pos, Side};
use rio_vt::crosswords::style::StyleFlags;
use rio_vt::selection::{Selection, SelectionRange, SelectionType};
use std::cell::{Cell as StdCell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use theme::terminal::{terminal_palette, TerminalPalette, FONT_FAMILY, FONT_SIZE, LINE_HEIGHT};

/// Grid inset so the first column/row is not glued to the panel edge. Small
/// and constant — it participates in the cell math AND the mouse mapping.
const PAD_X: f32 = 4.0;
const PAD_Y: f32 = 2.0;

/// Cursor blink half-period (visible ↔ hidden).
const BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// The terminal view's key context (EXP-71 shadowing target).
const KEY_CONTEXT: &str = "Terminal";

/// Shadow gpui-component `Root`'s window-wide `tab`/`shift-tab` focus-cycle
/// bindings inside the terminal (EXP-71). gpui dispatches keymap bindings
/// BEFORE `on_key_down` listeners, so without this a focused terminal never
/// sees tab/shift+tab — Root's focus traversal ate them (shift+tab is how
/// Claude cycles its modes). A `NoAction` binding in the deeper `Terminal`
/// context halts the binding search entirely, letting the raw key event fall
/// through to `handle_key_down` → `keys::to_esc_str` (`\t` / CSI Z). Must run
/// once at bootstrap, after `gpui_component::init(cx)`.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", NoAction, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-tab", NoAction, Some(KEY_CONTEXT)),
    ]);
}

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
    /// Uploaded inline-image textures keyed by rio's texture key (EXP-636);
    /// fed from the wake drain, read by the element in prepaint.
    images: Rc<RefCell<ImageCache>>,
    _wake_task: Task<()>,
    _blink_task: Task<()>,
}

impl TerminalView {
    pub fn new(mut session: Terminal, cx: &mut Context<Self>) -> Self {
        // This view paints, so the emulator may retain decoded image pixels.
        session.enable_graphics();
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
            images: Rc::new(RefCell::new(ImageCache::default())),
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
        let mode = term.lock().mode();
        mode
    }

    fn on_wake(&mut self, cx: &mut Context<Self>) {
        // Pump answers the §6.6 reply-required events into the PTY writer
        // and hands back the user-facing signals.
        let signals = self.session.borrow_mut().pump();
        for update in self.session.borrow_mut().take_graphics() {
            self.images.borrow_mut().apply(update);
        }
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
            let blinking = term.lock().blinking_cursor;
            blinking
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
            if term.display_offset() != 0 {
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
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .size_full()
            // EXP-285: no background paint — default-bg cells skip painting
            // (see `bg != palette.background` in layout), so the window's
            // page gradient shows through and the terminal blends into the
            // one glassy surface. `palette.background` stays meaningful as
            // the default-bg sentinel + OSC color-report value.
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
                images: self.images.clone(),
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
    fn hit(&self, position: PixelPoint<Pixels>) -> (ViewportCell, Pos, Side) {
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

/// One inline image placement resolved against this frame's geometry.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImagePlacement {
    /// rio texture key (`kitty_image_key` / `atlas_image_key`).
    key: u64,
    bounds: Bounds<Pixels>,
    /// Normalised (u0, v0, u1, v1) crop of the source texture.
    source_rect: [f32; 4],
    /// Kitty `z >= 0` paints over the text; sixel/iTerm2 (DEC semantics)
    /// and negative kitty z paint under it.
    above_text: bool,
}

struct ContentSnapshot {
    display_offset: usize,
    selection: Option<SelectionRange>,
    cells: Vec<CellSpec>,
    /// Viewport (col, row), shape, wide flag, and the glyph under the cursor
    /// (for block inversion). `None` when hidden or scrolled off-screen.
    cursor: Option<(usize, usize, CursorShape, bool, char)>,
    images: Vec<ImagePlacement>,
    /// Kitty images the paint cache has no texture for yet — transmitted
    /// before this view existed, or brought back by an alt-screen swap.
    /// (Sixel pixels only ever travel through `UpdateGraphics`, so a missing
    /// atlas key simply stays unpainted.)
    missing_images: Vec<(u64, GraphicData)>,
}

/// Copy the visible grid out of the emulator — called under the `FairMutex`,
/// kept cheap (no shaping, no allocation beyond the cell vec) so the read
/// thread is never starved (§6.11). `cached` answers whether the paint side
/// already holds a texture for a key.
fn snapshot_content(
    term: &Term,
    palette: &TerminalPalette,
    geometry: &GridGeometry,
    rows: usize,
    cached: &dyn Fn(u64) -> bool,
) -> ContentSnapshot {
    let display_offset = term.display_offset();
    let selection = term.selection.as_ref().and_then(|selection| selection.to_range(term));
    let cursor_state = term.cursor();
    let cursor_point = cursor_state.pos;
    let cursor_shape = cursor_state.content;

    let mut cells = Vec::new();
    let mut cursor_char = ' ';
    let mut cursor_wide = false;

    for indexed in term.grid.display_iter() {
        let square = *indexed.square;
        // Trailing half of a double-width glyph — never emit a glyph or
        // advance for it (§6.9); the base cell carries width 2.
        if square.is_spacer() {
            continue;
        }
        let Some(row) = viewport_row(indexed.pos.row.0, display_offset, rows) else {
            continue;
        };
        // rio's empty square reads back as NUL; normalise to a space BEFORE
        // `has_ink` (NUL is not whitespace, so every blank cell would
        // otherwise shape a glyph run).
        let base = match square.c() {
            '\0' => ' ',
            c => c,
        };
        if indexed.pos == cursor_point {
            cursor_char = base;
            cursor_wide = square.is_wide();
        }

        let style = term.grid.style_of(&square);
        let flags = style.flags;
        let inverse = flags.contains(StyleFlags::INVERSE);
        let mut fg = resolve_color(&style.fg, term, palette);
        let mut bg = resolve_color(&style.bg, term, palette);
        if inverse {
            std::mem::swap(&mut fg, &mut bg);
        }
        if flags.contains(StyleFlags::DIM) {
            fg = Hsla {
                l: fg.l * (2.0 / 3.0),
                ..fg
            };
        }
        let bg = (bg != palette.background).then_some(bg);

        // A wrapped wide char leaves a blank leading spacer at line end.
        let leading_spacer = square.is_leading_spacer();

        let mut text = String::new();
        if !leading_spacer {
            text.push(base);
            // Zero-width/combining marks folded onto the base cell.
            text.extend(term.grid.cell_text(indexed.pos).skip(1));
        }

        let spec = CellSpec {
            row,
            col: indexed.pos.col.0,
            width: if square.is_wide() { 2 } else { 1 },
            text,
            fg,
            bg,
            bold: flags.contains(StyleFlags::BOLD),
            italic: flags.contains(StyleFlags::ITALIC),
            underline: flags.intersects(StyleFlags::ALL_UNDERLINES),
            undercurl: flags.contains(StyleFlags::UNDERCURL),
            strikethrough: flags.contains(StyleFlags::STRIKEOUT),
            hidden: flags.contains(StyleFlags::HIDDEN) || leading_spacer,
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
        viewport_row(cursor_point.row.0, display_offset, rows)
            .map(|row| (cursor_point.col.0, row, cursor_shape, cursor_wide, cursor_char))
    };

    // -- Inline images (EXP-636): geometry is resolved per frame against the
    //    SAME cell stride the text paints with, so images and glyphs stay in
    //    lockstep through scroll, resize and retransmit.
    let viewport = OverlayViewport {
        cell_width: f32::from(geometry.cell_width),
        cell_height: f32::from(geometry.line_height),
        origin_x: f32::from(geometry.origin.x),
        origin_y: f32::from(geometry.origin.y),
        // Placements anchor in rio's ABSOLUTE row space, which starts at
        // the lines the grid has EVICTED (scrolled past its scrollback cap
        // or, on the alt screen with no scrollback at all, scrolled off),
        // not at the live history: `dest_row = lines_evicted + history +
        // row`. Feeding history alone put a full-screen TUI's image
        // `lines_evicted` rows below the viewport once its startup log had
        // scrolled the alt screen (terminal-doom → black pane).
        history_size: term.grid.lines_evicted() as i64 + term.history_size() as i64,
        display_offset: display_offset as i64,
        screen_lines: rows as i64,
    };
    let mut images = Vec::new();
    let mut missing_images: Vec<(u64, GraphicData)> = Vec::new();
    for placement in term.graphics.kitty_placements.values() {
        let Some(image) = term.graphics.kitty_images.get(&placement.image_id) else {
            continue;
        };
        let Some(overlay) = kitty_overlay_geometry(
            placement,
            image.data.width,
            image.data.height,
            &viewport,
        ) else {
            continue;
        };
        let key = kitty_image_key(placement.image_id);
        if !cached(key) && !missing_images.iter().any(|(k, _)| *k == key) {
            missing_images.push((key, image.data.clone()));
        }
        images.push(ImagePlacement {
            key,
            bounds: overlay_bounds(overlay.x, overlay.y, overlay.width, overlay.height),
            source_rect: overlay.source_rect,
            above_text: placement.z_index >= 0,
        });
    }
    for placement in &term.graphics.atlas_placements {
        let Some(overlay) = atlas_overlay_geometry(placement, &viewport) else {
            continue;
        };
        images.push(ImagePlacement {
            key: placement.image_key,
            bounds: overlay_bounds(overlay.x, overlay.y, overlay.width, overlay.height),
            source_rect: overlay.source_rect,
            above_text: false,
        });
    }

    ContentSnapshot {
        display_offset,
        selection,
        cells,
        cursor,
        images,
        missing_images,
    }
}

fn overlay_bounds(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), gpui::size(px(width), px(height)))
}

// ---------------------------------------------------------------------------
// Inline-image textures (EXP-636)
// ---------------------------------------------------------------------------

/// Decoded image → a gpui frame. gpui's sprite pipeline expects straight-alpha
/// **BGRA** (the same byte swap `gpui::img` applies to decoded PNGs), so RGB
/// is expanded and the channels swapped here, once, at upload.
pub(crate) fn graphic_to_frame(data: &GraphicData) -> Option<image::Frame> {
    let (width, height) = (data.width as u32, data.height as u32);
    let mut pixels: Vec<u8> = match data.color_type {
        ColorType::Rgba => data.pixels.clone(),
        ColorType::Rgb => data
            .pixels
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
    };
    if pixels.len() != data.width * data.height * 4 {
        return None;
    }
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    Some(image::Frame::new(image::RgbaImage::from_raw(width, height, pixels)?))
}

/// A texture rio freed whose sprite-atlas tiles are still to be released:
/// exactly what [`Window::drop_image`] keys on (gpui image id + frame
/// count), never the pixels. EXP-675: the wake drain runs for a HIDDEN tab
/// but prepaint does not, so parking the `Arc<RenderImage>` itself here
/// kept every decoded BGRA frame of a background frame-per-image client
/// alive until the tab was next shown; the Arc now drops at free time and
/// this 16-byte record is all a hidden tab accumulates per freed texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DroppedTexture {
    pub(crate) id: ImageId,
    pub(crate) frames: usize,
}

impl DroppedTexture {
    fn of(image: &RenderImage) -> Self {
        Self { id: image.id, frames: image.frame_count() }
    }

    /// A pixel-free stand-in [`Window::drop_image`] accepts in place of the
    /// freed texture: the same id and frame count over 1×1 frames, so the
    /// atlas removes the very keys the original's paints inserted.
    pub(crate) fn tombstone(self) -> Arc<RenderImage> {
        let frames: Vec<image::Frame> =
            (0..self.frames).map(|_| image::Frame::new(image::RgbaImage::new(1, 1))).collect();
        let mut tombstone = RenderImage::new(frames);
        tombstone.id = self.id;
        Arc::new(tombstone)
    }
}

/// The paint side's texture store, keyed by rio texture key. Fed by
/// [`GraphicsUpdate`]s (uploads + frees) and by kitty images the snapshot
/// found missing; sized by rio's own image budget plus one BGRA copy each.
#[derive(Default)]
pub(crate) struct ImageCache {
    images: HashMap<u64, Arc<RenderImage>>,
    /// Textures rio freed (its 320MB kitty quota evicting, `a=d,d=I`, a
    /// retransmit under the same id) that the sprite atlas still holds
    /// until [`Window::drop_image`] runs at the next prepaint. Without this
    /// hand-off a frame-per-image client (terminal-doom uploads a fresh id
    /// ~35×/s and never deletes) grows the atlas without bound. Holds ids
    /// only (see [`DroppedTexture`]) so a hidden tab retains no pixels.
    dropped: Vec<DroppedTexture>,
    /// The highest [`ImageId`] this cache held at the LAST prepaint drain —
    /// the newest texture the paint side can possibly have given atlas tiles.
    /// gpui ids come off one global monotonic counter, so anything above it
    /// was uploaded AFTER that prepaint; freed before the next one, it was
    /// never painted and its tombstone would remove nothing. Skipping those
    /// is what BOUNDS `dropped` on a hidden tab, where the wake drain keeps
    /// feeding uploads and frees while prepaint never runs (EXP-675).
    painted_max: Option<ImageId>,
}

impl ImageCache {
    pub(crate) fn apply(&mut self, update: GraphicsUpdate) {
        for key in update.removed {
            if let Some(image) = self.images.remove(&key) {
                self.free(&image);
            }
        }
        for (key, data) in &update.images {
            self.insert(*key, data);
        }
    }

    /// Bank a freed texture's atlas identity for the paint side — unless it
    /// cannot have one: an id above [`Self::painted_max`] was uploaded since
    /// the last prepaint, so no paint ever inserted an atlas key for it.
    fn free(&mut self, image: &RenderImage) {
        if self.painted_max.is_some_and(|max| image.id <= max) {
            self.dropped.push(DroppedTexture::of(image));
        }
    }

    /// Textures to release from the sprite atlas, drained by the paint side.
    /// Called once per prepaint, AFTER that frame's late uploads and
    /// placement lookups — so the live set's high-water id is exactly the
    /// newest texture the frame can have painted ([`Self::painted_max`]).
    pub(crate) fn take_dropped(&mut self) -> Vec<DroppedTexture> {
        self.painted_max = self.images.values().map(|image| image.id).max();
        std::mem::take(&mut self.dropped)
    }

    pub(crate) fn insert(&mut self, key: u64, data: &GraphicData) {
        let Some(frame) = graphic_to_frame(data) else {
            log::debug!("terminal image {key}: unexpected pixel buffer, skipped");
            return;
        };
        if let Some(previous) = self.images.insert(key, Arc::new(RenderImage::new(vec![frame]))) {
            self.free(&previous);
        }
    }

    pub(crate) fn contains(&self, key: u64) -> bool {
        self.images.contains_key(&key)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.images.len()
    }

    /// Decoded pixel bytes this cache keeps alive — live textures only; the
    /// dropped list holds none by construction.
    #[cfg(test)]
    pub(crate) fn retained_bytes(&self) -> usize {
        self.images
            .values()
            .map(|image| (0..image.frame_count()).map(|i| image.as_bytes(i).map_or(0, <[u8]>::len)).sum::<usize>())
            .sum()
    }

    #[cfg(test)]
    pub(crate) fn pending_drops(&self) -> usize {
        self.dropped.len()
    }

    pub(crate) fn get(&self, key: u64) -> Option<Arc<RenderImage>> {
        self.images.get(&key).cloned()
    }
}

/// The quad the FULL texture would occupy so that `bounds` shows exactly its
/// normalised `source_rect` (u0, v0, u1, v1): gpui's `paint_image` scales the
/// texture into `image_bounds` and renders `bounds ∩ image_bounds`, which is
/// a source sub-rect without any pixel copies.
pub(crate) fn full_image_bounds(bounds: Bounds<Pixels>, [u0, v0, u1, v1]: [f32; 4]) -> Bounds<Pixels> {
    let du = (u1 - u0).max(f32::EPSILON);
    let dv = (v1 - v0).max(f32::EPSILON);
    let width = f32::from(bounds.size.width) / du;
    let height = f32::from(bounds.size.height) / dv;
    let x = f32::from(bounds.origin.x) - u0 * width;
    let y = f32::from(bounds.origin.y) - v0 * height;
    Bounds::new(point(px(x), px(y)), gpui::size(px(width), px(height)))
}

/// One resolved image quad: (visible bounds, full-texture bounds, texture).
type ImageQuad = (Bounds<Pixels>, Bounds<Pixels>, Arc<RenderImage>);

/// Paint one image layer, clipped to the element (partially visible
/// placements keep their full quad and the mask clips them).
fn paint_images(window: &mut Window, clip: Bounds<Pixels>, images: &[ImageQuad]) {
    if images.is_empty() {
        return;
    }
    window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
        for (bounds, image_bounds, image) in images {
            if let Err(error) = window.paint_image(
                *bounds,
                *image_bounds,
                Corners::default(),
                image.clone(),
                0,
                false,
            ) {
                log::debug!("terminal inline image paint failed: {error}");
            }
        }
    });
}

/// Buffer line (scrollback-relative) → viewport row, `None` off-screen.
pub(crate) fn viewport_row(line: i32, display_offset: usize, rows: usize) -> Option<usize> {
    let row = line + display_offset as i32;
    (0..rows as i32).contains(&row).then_some(row as usize)
}

/// Resolve an ANSI color through the runtime color table (OSC 4/10/11
/// overrides) then the theme palette (§6.8's table).
fn resolve_color(color: &AnsiColor, term: &Term, palette: &TerminalPalette) -> Hsla {
    match color {
        AnsiColor::Spec(rgb) => rgb_to_hsla(*rgb),
        AnsiColor::Named(named) => {
            if let Some(rgb) = term.colors()[*named as usize] {
                return rgb_to_hsla(ColorRgb::from_color_arr(rgb));
            }
            named_color(*named, palette)
        }
        AnsiColor::Indexed(index) => {
            let index = *index as usize;
            if let Some(rgb) = term.colors()[index] {
                return rgb_to_hsla(ColorRgb::from_color_arr(rgb));
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
        NamedColor::LightForeground => palette.bright_foreground,
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
            rgb_to_hsla(ColorRgb {
                r: channel(i / 36),
                g: channel((i / 6) % 6),
                b: channel(i % 6),
            })
        }
        232..=255 => {
            let v = (8 + 10 * (index - 232)) as u8;
            rgb_to_hsla(ColorRgb { r: v, g: v, b: v })
        }
        _ => palette.foreground,
    }
}

fn rgb_to_hsla(rgb: ColorRgb) -> Hsla {
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
        if line < selection.start.row.0 || line > selection.end.row.0 {
            continue;
        }
        let start = if selection.is_block || line == selection.start.row.0 {
            selection.start.col.0
        } else {
            0
        };
        let end = if selection.is_block || line == selection.end.row.0 {
            selection.end.col.0
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
    images: Rc<RefCell<ImageCache>>,
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
    /// Element bounds — the clip for inline images.
    clip: Bounds<Pixels>,
    bg_quads: Vec<(Bounds<Pixels>, Hsla)>,
    selection_quads: Vec<Bounds<Pixels>>,
    /// Sixel/iTerm2 + kitty `z < 0`: under the text.
    images_below: Vec<ImageQuad>,
    text_runs: Vec<(PixelPoint<Pixels>, ShapedLine)>,
    /// Kitty `z >= 0`: over the text, under the cursor and IME.
    images_above: Vec<ImageQuad>,
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
                clip: bounds,
                bg_quads: Vec::new(),
                selection_quads: Vec::new(),
                images_below: Vec::new(),
                text_runs: Vec::new(),
                images_above: Vec::new(),
                cursor: None,
                ime: None,
            };
        }

        // -- §6.10 resize pair: Term reflow + PTY TIOCSWINSZ (SIGWINCH), only
        //    on integer cell change (Terminal::resize dedupes) ---------------
        {
            let mut session = self.session.borrow_mut();
            // EXP-636: real cell metrics first — they feed the CSI 14t pixel
            // reply and rio's image placement math.
            session.set_cell_px(
                f32::from(cell_width).round().max(1.0) as u32,
                f32::from(line_height).round().max(1.0) as u32,
            );
            if let Err(error) = session.resize(cols as u16, rows as u16) {
                log::warn!("terminal resize to {cols}x{rows}: {error}");
            }
        }

        // -- Snapshot the grid under the FairMutex (held briefly, §6.11) ----
        let snapshot = {
            let images = self.images.borrow();
            let term = self.session.borrow().term();
            let term = term.lock();
            snapshot_content(&term, &self.palette, &geometry, rows, &|key| images.contains(key))
        };
        geometry.display_offset = snapshot.display_offset;

        // -- Inline images: upload late arrivals, resolve textures per layer --
        let (images_below, images_above) = {
            let mut cache = self.images.borrow_mut();
            for (key, data) in &snapshot.missing_images {
                cache.insert(*key, data);
            }
            let mut below = Vec::new();
            let mut above = Vec::new();
            for placement in &snapshot.images {
                if let Some(image) = cache.get(placement.key) {
                    let layer = if placement.above_text { &mut above } else { &mut below };
                    let image_bounds = full_image_bounds(placement.bounds, placement.source_rect);
                    layer.push((placement.bounds, image_bounds, image));
                }
            }
            // Release atlas tiles for textures rio has freed since last frame.
            for dropped in cache.take_dropped() {
                if let Err(error) = window.drop_image(dropped.tombstone()) {
                    log::debug!("terminal inline image drop failed: {error}");
                }
            }
            (below, above)
        };

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
            clip: bounds,
            bg_quads,
            selection_quads,
            images_below,
            text_runs,
            images_above,
            cursor,
            ime,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let palette = self.palette;
        let line_height = layout.geometry.line_height;

        // EXP-285: no background clear — default-bg cells paint nothing and
        // the window's page gradient shows through (one glassy surface).
        if layout.geometry.cols == 0 || layout.geometry.rows == 0 {
            return; // collapsed dock — nothing else to do (§6.9)
        }

        for (quad_bounds, color) in &layout.bg_quads {
            window.paint_quad(fill(*quad_bounds, *color));
        }
        for selection_bounds in &layout.selection_quads {
            window.paint_quad(fill(*selection_bounds, palette.selection));
        }
        paint_images(window, layout.clip, &layout.images_below);
        for (origin, line) in &layout.text_runs {
            let _ = line.paint(*origin, line_height, TextAlign::default(), None, window, cx);
        }
        paint_images(window, layout.clip, &layout.images_above);

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

        // IME composition overlay at the cursor. This quad must stay OPAQUE —
        // it occludes the grid cells the composition text is drawn over (IME
        // mid-line would otherwise render two strings on top of each other).
        // EXP-285: with the grid no longer clearing to `palette.background`,
        // that flat fill banded against the page gradient, so sample the
        // gradient at this quad's own y instead.
        if let Some((ime_bounds, origin, line)) = &layout.ime {
            let viewport_h = f32::from(window.viewport_size().height);
            let ime_t = if viewport_h > 0. {
                f32::from(ime_bounds.origin.y) / viewport_h
            } else {
                0.
            };
            window.paint_quad(fill(
                *ime_bounds,
                theme::background_gradient_color_at(ime_t),
            ));
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
    use crate::emulator::Emulator;
    use gpui::{KeyContext, Keymap, Keystroke};
    use rio_vt::crosswords::pos::{Column, Line};

    /// 20×4 grid at the origin with 8×16 cells — the geometry the snapshot
    /// tests resolve images against.
    fn test_geometry() -> GridGeometry {
        GridGeometry {
            origin: point(px(0.0), px(0.0)),
            cell_width: px(8.0),
            line_height: px(16.0),
            cols: 20,
            rows: 4,
            display_offset: 0,
        }
    }

    fn snapshot(emulator: &Emulator) -> ContentSnapshot {
        let palette = terminal_palette();
        let term = emulator.term();
        let term = term.lock();
        snapshot_content(&term, &palette, &test_geometry(), 4, &|_| false)
    }

    /// The 2×2 opaque red kitty image from the emulator tests, placed over
    /// 4 columns × 2 rows at the cursor.
    const KITTY_RED_4X2_CELLS: &[u8] =
        b"\x1b_Gf=32,s=2,v=2,a=T,i=1,c=4,r=2;/wAA//8AAP//AAD//wAA/w==\x1b\\";

    gpui::actions!(terminal_element_tests, [FakeRootTab, FakeRootTabPrev]);

    /// EXP-71: [`init`]'s `NoAction` bindings in the deeper `Terminal` context
    /// must shadow Root-level tab/shift-tab focus-traversal bindings, leaving
    /// NO matched binding — so the raw key event falls through to
    /// `handle_key_down` and reaches the PTY as `\t` / CSI Z.
    #[test]
    fn tab_bindings_shadow_root_focus_traversal() {
        let root_bindings = vec![
            KeyBinding::new("tab", FakeRootTab, Some("Root")),
            KeyBinding::new("shift-tab", FakeRootTabPrev, Some("Root")),
        ];
        let stack = [
            KeyContext::parse("Root").unwrap(),
            KeyContext::parse(KEY_CONTEXT).unwrap(),
        ];
        let tab = [Keystroke::parse("tab").unwrap()];
        let shift_tab = [Keystroke::parse("shift-tab").unwrap()];

        // Without the shadow: Root's focus-cycle bindings match inside the
        // terminal (the regression this fix addresses).
        let unshadowed = Keymap::new(root_bindings.clone());
        assert!(!unshadowed.bindings_for_input(&tab, &stack).0.is_empty());
        assert!(!unshadowed.bindings_for_input(&shift_tab, &stack).0.is_empty());

        // With init()'s shadow bindings: nothing matches, the event falls
        // through to the key listener. ctrl-tab (dock tab-switch) untouched.
        let mut keymap = Keymap::new(root_bindings);
        keymap.add_bindings(vec![
            KeyBinding::new("tab", NoAction, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-tab", NoAction, Some(KEY_CONTEXT)),
        ]);
        let (bindings, pending) = keymap.bindings_for_input(&shift_tab, &stack);
        assert!(bindings.is_empty());
        assert!(!pending);
        let (bindings, pending) = keymap.bindings_for_input(&tab, &stack);
        assert!(bindings.is_empty());
        assert!(!pending);
        let ctrl_tab = [Keystroke::parse("ctrl-tab").unwrap()];
        let dock_stack = [KeyContext::parse("Root").unwrap()];
        let mut with_dock = Keymap::new(vec![KeyBinding::new(
            "ctrl-tab",
            FakeRootTab,
            Some("Root"),
        )]);
        with_dock.add_bindings(vec![
            KeyBinding::new("tab", NoAction, Some(KEY_CONTEXT)),
            KeyBinding::new("shift-tab", NoAction, Some(KEY_CONTEXT)),
        ]);
        assert!(
            !with_dock
                .bindings_for_input(&ctrl_tab, &dock_stack)
                .0
                .is_empty()
        );
    }

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
            Pos::new(Line(0), Column(2)),
            Pos::new(Line(2), Column(4)),
            false,
        );
        let spans = selection_row_spans(&sel, 0, 10, 24);
        assert_eq!(
            spans,
            vec![(0, 2, 9), (1, 0, 9), (2, 0, 4)],
            "first row from start col, middle rows full, last row to end col"
        );

        let block = SelectionRange::new(
            Pos::new(Line(1), Column(2)),
            Pos::new(Line(3), Column(5)),
            true,
        );
        let spans = selection_row_spans(&block, 0, 10, 24);
        assert_eq!(spans, vec![(1, 2, 5), (2, 2, 5), (3, 2, 5)]);
    }

    #[test]
    fn selection_spans_respect_display_offset() {
        // Selection on buffer line 0 while scrolled back 3 → viewport row 3.
        let sel = SelectionRange::new(
            Pos::new(Line(0), Column(0)),
            Pos::new(Line(0), Column(2)),
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
        assert_eq!(named_color(NamedColor::LightBlue, &palette), palette.ansi(12));
        assert_eq!(named_color(NamedColor::DimRed, &palette), palette.dim[1]);
    }

    #[test]
    fn snapshot_skips_wide_spacers_and_resolves_colors() {
        // Integration-ish: feed the emulator CJK + colored text and check
        // the snapshot the element would lay out (no Window involved).
        let emulator = Emulator::new(20, 4);
        emulator.advance_bytes(b"\x1b[31mred\x1b[0m \xe4\xbd\xa0a");
        let palette = terminal_palette();
        let snapshot = snapshot(&emulator);

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
        let emulator = Emulator::new(20, 4);
        emulator.advance_bytes(b"\x1b[7mX");
        let palette = terminal_palette();
        let snapshot = snapshot(&emulator);
        let cell = snapshot
            .cells
            .iter()
            .find(|c| c.text == "X")
            .expect("inverse cell present");
        assert_eq!(cell.fg, palette.background);
        assert_eq!(cell.bg, Some(palette.foreground));
    }

    #[test]
    fn snapshot_blank_cells_emit_no_specs() {
        // rio's empty squares are NUL, not spaces: without the normalisation
        // every blank cell would count as ink and shape a glyph run.
        let emulator = Emulator::new(20, 4);
        emulator.advance_bytes(b"\x1b[4Ga");
        let snapshot = snapshot(&emulator);
        assert_eq!(snapshot.cells.len(), 1, "cells: {:?}", snapshot.cells);
        assert_eq!((snapshot.cells[0].col, snapshot.cells[0].text.as_str()), (3, "a"));
        let (.., ch) = snapshot.cursor.expect("cursor visible");
        assert_eq!(ch, ' ', "cursor over an empty square reads as a space");
    }

    #[test]
    fn kitty_placement_yields_cell_aligned_bounds() {
        let mut emulator = Emulator::new(20, 4);
        emulator.enable_graphics();
        emulator.advance_bytes(KITTY_RED_4X2_CELLS);
        let snapshot = snapshot(&emulator);
        assert_eq!(snapshot.images.len(), 1, "images: {:?}", snapshot.images);
        let image = &snapshot.images[0];
        assert_eq!(image.key, kitty_image_key(1));
        assert!(image.above_text, "kitty z=0 paints over text");
        assert_eq!(image.bounds.origin, point(px(0.0), px(0.0)));
        assert_eq!(image.bounds.size, gpui::size(px(32.0), px(32.0)), "4 cols × 2 rows of 8×16");
        // Nothing cached yet → the snapshot hands the pixels over for upload.
        assert_eq!(snapshot.missing_images.len(), 1);
        assert_eq!(snapshot.missing_images[0].0, kitty_image_key(1));
    }

    /// A placement made after the alt screen scrolled (no scrollback, so
    /// every scrolled line is EVICTED) must still land at its viewport row:
    /// rio anchors `dest_row` at `lines_evicted + history + row`.
    #[test]
    fn kitty_placement_survives_evicted_alt_screen_lines() {
        let mut emulator = Emulator::new(20, 4);
        emulator.enable_graphics();
        // Enter the alt screen, scroll ten lines off it, then place at home.
        emulator.advance_bytes(b"\x1b[?1049h");
        emulator.advance_bytes(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9\r\n10\r\n11\r\n12\r\n13\r\n14\r\n");
        emulator.advance_bytes(b"\x1b[1;1H");
        emulator.advance_bytes(KITTY_RED_4X2_CELLS);
        {
            let term = emulator.term();
            let term = term.lock();
            assert!(term.grid.lines_evicted() >= 10, "fixture must evict lines");
            assert_eq!(term.history_size(), 0, "alt screen keeps no history");
        }
        let snapshot = snapshot(&emulator);
        assert_eq!(snapshot.images.len(), 1, "placement dropped: {:?}", snapshot.images);
        assert_eq!(snapshot.images[0].bounds.origin, point(px(0.0), px(0.0)));
    }

    #[test]
    fn image_cache_uploads_bgra_frames_and_frees_on_remove() {
        let mut emulator = Emulator::new(20, 4);
        emulator.enable_graphics();
        emulator.advance_bytes(KITTY_RED_4X2_CELLS);
        emulator.drain_events(&mut |_| {});
        let mut cache = ImageCache::default();
        for update in emulator.take_graphics() {
            cache.apply(update);
        }
        assert!(cache.contains(kitty_image_key(1)));
        let full = cache.get(kitty_image_key(1)).expect("full texture");
        let bytes = full.as_bytes(0).expect("frame 0");
        assert_eq!(bytes.len(), 2 * 2 * 4);
        assert_eq!(&bytes[..4], &[0, 0, 255, 255], "red arrives as BGRA");
        // A prepaint painted it — from here the sprite atlas holds keys for
        // it, so a free must hand them back.
        assert!(cache.take_dropped().is_empty(), "nothing freed yet");
        cache.apply(GraphicsUpdate { images: Vec::new(), removed: vec![kitty_image_key(1)] });
        assert_eq!(cache.len(), 0);
        // The freed texture is handed to the paint side for `drop_image` as
        // its atlas identity only — pixels go with the Arc.
        let dropped = cache.take_dropped();
        assert_eq!(dropped, vec![DroppedTexture { id: full.id, frames: 1 }]);
        assert!(cache.take_dropped().is_empty(), "drained once");
        let tombstone = dropped[0].tombstone();
        assert_eq!(tombstone.id, full.id, "drop_image keys the atlas on the original id");
        assert_eq!(tombstone.frame_count(), full.frame_count());
        assert_eq!(tombstone.as_bytes(0).map(<[u8]>::len), Some(4), "1×1 stand-in frame");
    }

    /// EXP-675: a HIDDEN tab's wake drain keeps feeding the cache while
    /// prepaint (the only drain of the dropped list) never runs. A client
    /// that retransmits a fresh frame every wake must not pile its old
    /// frames up there — only the live texture's bytes stay resident.
    #[test]
    fn hidden_tab_wakes_retain_no_freed_pixels() {
        let mut emulator = Emulator::new(20, 4);
        emulator.enable_graphics();
        let mut cache = ImageCache::default();
        // One VISIBLE frame first, so there is a painted texture whose atlas
        // tiles a later free genuinely has to release.
        emulator.advance_bytes(KITTY_RED_4X2_CELLS);
        emulator.drain_events(&mut |_| {});
        for update in emulator.take_graphics() {
            cache.apply(update);
        }
        assert!(cache.take_dropped().is_empty(), "prepaint: nothing freed yet");

        let wakes = 64;
        for _ in 0..wakes {
            // One wake: the child pushed a new frame under the same id
            // (rio frees the old texture), the view applied the update.
            // Prepaint — the only drain — never runs; the tab is hidden.
            emulator.advance_bytes(KITTY_RED_4X2_CELLS);
            emulator.drain_events(&mut |_| {});
            for update in emulator.take_graphics() {
                cache.apply(update);
            }
        }
        let one_frame = 2 * 2 * 4;
        assert_eq!(cache.len(), 1, "one live texture");
        assert_eq!(cache.retained_bytes(), one_frame, "old frames are not retained");
        // Only the texture that was on screen at that prepaint can hold atlas
        // keys; the other 63 were uploaded AND freed while hidden, so their
        // tombstones would remove nothing. The list stays BOUNDED instead of
        // growing once per wake.
        assert_eq!(
            cache.pending_drops(),
            1,
            "only the painted texture is banked; {wakes} hidden wakes add nothing"
        );
        // What a hidden tab accumulates per freed texture is the atlas key, not a frame.
        assert_eq!(std::mem::size_of::<DroppedTexture>(), 16);
        // Showing the tab drains the ids; each maps to a paintable tombstone.
        let dropped = cache.take_dropped();
        assert!(dropped.iter().all(|d| d.frames == 1));
        assert_eq!(cache.pending_drops(), 0);
    }

    /// EXP-675: a texture freed before ANY prepaint saw it was never painted,
    /// so no atlas key exists to remove — banking a tombstone for it would
    /// only grow the hand-off list.
    #[test]
    fn a_never_painted_texture_banks_no_tombstone() {
        let mut emulator = Emulator::new(20, 4);
        emulator.enable_graphics();
        let mut cache = ImageCache::default();
        emulator.advance_bytes(KITTY_RED_4X2_CELLS);
        emulator.drain_events(&mut |_| {});
        for update in emulator.take_graphics() {
            cache.apply(update);
        }
        assert_eq!(cache.len(), 1);
        cache.apply(GraphicsUpdate { images: Vec::new(), removed: vec![kitty_image_key(1)] });
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.pending_drops(), 0, "no paint inserted a key to remove");
    }

    /// The PTY winsize carries the pixel extent pixel-aware TUIs read via
    /// `TIOCGWINSZ` (libvaxis scales kitty images by it).
    #[test]
    fn pty_winsize_reports_pixel_extent() {
        let size = crate::pty::winsize(120, 40, (7, 15));
        assert_eq!((size.cols, size.rows), (120, 40));
        assert_eq!((size.pixel_width, size.pixel_height), (840, 600));
        let huge = crate::pty::winsize(u16::MAX, u16::MAX, (1000, 1000));
        assert_eq!((huge.pixel_width, huge.pixel_height), (u16::MAX, u16::MAX), "clamped");
    }

    #[test]
    fn full_image_bounds_inverts_the_source_crop() {
        // The visible quad shows the right half of the texture: the full
        // texture quad is twice as wide and starts one quad-width to the left.
        let visible = Bounds::new(point(px(100.0), px(50.0)), gpui::size(px(40.0), px(20.0)));
        let full = full_image_bounds(visible, [0.5, 0.0, 1.0, 1.0]);
        assert_eq!(full.origin, point(px(60.0), px(50.0)));
        assert_eq!(full.size, gpui::size(px(80.0), px(20.0)));
        // An uncropped placement is its own texture quad.
        assert_eq!(full_image_bounds(visible, [0.0, 0.0, 1.0, 1.0]), visible);
    }
}
