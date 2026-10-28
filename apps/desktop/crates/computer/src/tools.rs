//! The tool surface the agent sees: the shape of Anthropic's computer tool,
//! one MCP tool per action, pixels only. Every coordinate the agent reads or
//! writes is in the FRAME: ONE display's screenshot fitted into 1280x800
//! (never upscaled), so a model sees one size whatever the screen; `Frame`
//! maps it back to the OS's units. A run works on one display at a time
//! (the primary until a `screenshot {display}` picks another); its windows
//! on the others list with coordinates outside the frame.
//!
//! NEW-59: every call passes the work-screen gate first ([`work_screen`]):
//! agents drive only the work displays (the device's pick, else every
//! display without a fullscreen app), and a refused call never touches the
//! screen or the person's input.

use std::io::Cursor;
use std::time::Duration;

use serde_json::{json, Value};

use crate::backend::{Backend, Button, DisplayInfo, Image, WindowInfo};
use crate::keys::{parse_combo, Key, Named};
use crate::work_screen;

/// The frame's ceiling: what the vision models read best.
pub(crate) const MAX_WIDTH: u32 = 1280;
pub(crate) const MAX_HEIGHT: u32 = 800;

/// A `type` call's ceiling (a page of text; longer text goes through the
/// clipboard or a file).
const TYPE_MAX_CHARS: usize = 4000;
/// Between typed characters (xdotool's default).
const TYPE_GAP: Duration = Duration::from_millis(12);
/// Between the press and the release of one click, and between two clicks.
const CLICK_GAP: Duration = Duration::from_millis(40);
/// Between the pointer's arrival and the press: a press in the same
/// instant as the motion is lost on an X11 toolkit that takes it under a
/// click-to-focus grab (gpui), while the release still lands.
const MOVE_SETTLE: Duration = Duration::from_millis(30);
const WAIT_MAX_SECONDS: f64 = 10.0;

/// Screen units ↔ frame pixels, for one display.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Frame {
    /// The display's origin in screen units.
    pub origin: (i32, i32),
    /// The display's size in screen units.
    pub screen: (u32, u32),
    pub image: (u32, u32),
}

impl Frame {
    pub(crate) fn fit(display: &DisplayInfo) -> Frame {
        let (w, h) = (display.width.max(1), display.height.max(1));
        let scale = (f64::from(MAX_WIDTH) / f64::from(w)).min(f64::from(MAX_HEIGHT) / f64::from(h)).min(1.0);
        let image = (
            ((f64::from(w) * scale).round() as u32).max(1),
            ((f64::from(h) * scale).round() as u32).max(1),
        );
        Frame { origin: (display.x, display.y), screen: (w, h), image }
    }

    fn scale(self) -> (f64, f64) {
        (f64::from(self.image.0) / f64::from(self.screen.0), f64::from(self.image.1) / f64::from(self.screen.1))
    }

    /// A frame point to the screen, kept inside the display.
    pub(crate) fn to_screen(self, x: f64, y: f64) -> (i32, i32) {
        let (sx, sy) = self.scale();
        let clamp = |v: f64, max: u32| v.round().clamp(0.0, f64::from(max.saturating_sub(1))) as i32;
        (self.origin.0 + clamp(x / sx, self.screen.0), self.origin.1 + clamp(y / sy, self.screen.1))
    }

    pub(crate) fn to_frame(self, x: i32, y: i32) -> (i32, i32) {
        let (sx, sy) = self.scale();
        (
            (f64::from(x - self.origin.0) * sx).round() as i32,
            (f64::from(y - self.origin.1) * sy).round() as i32,
        )
    }

    fn length_to_frame(self, w: u32, h: u32) -> (u32, u32) {
        let (sx, sy) = self.scale();
        ((f64::from(w) * sx).round() as u32, (f64::from(h) * sy).round() as u32)
    }
}

const COORDS: &str = "in the last screenshot's pixels";

/// `tools/list`.
pub(crate) fn list() -> Value {
    let point = |what: &str| {
        json!({
            "type": "object",
            "properties": { "x": { "type": "number", "description": format!("{what} x, {COORDS}") }, "y": { "type": "number", "description": format!("{what} y, {COORDS}") },
                "modifiers": { "type": "string", "description": "Keys held during the click, e.g. \"ctrl\" or \"ctrl+shift\"" } },
            "required": ["x", "y"]
        })
    };
    let none = json!({ "type": "object", "properties": {} });
    json!([
        { "name": "screenshot", "description": format!("The current display now, as a PNG fitted to at most {MAX_WIDTH}x{MAX_HEIGHT}. Every coordinate of every other tool is in THIS image's pixels. Take one before you act and one after. `display` switches the run to another work-screen display (the indexes list_windows reports); it stays until the next switch."), "inputSchema": { "type": "object", "properties": { "display": { "type": "integer", "description": "A display index from list_windows (default: the current one, the primary at first)", "minimum": 0 } } } },
        { "name": "list_windows", "description": "Every top-level window on the work screen, top first: id, title, app, display, frame (pixels of the CURRENT display's screenshot; a window on another display lies outside it), active; plus the work-screen displays.", "inputSchema": none },
        { "name": "activate_window", "description": "Bring a window to the front and give it the keyboard.", "inputSchema": { "type": "object", "properties": { "id": { "type": "integer", "description": "A list_windows id" } }, "required": ["id"] } },
        { "name": "cursor_position", "description": "Where the pointer is, in screenshot pixels.", "inputSchema": none },
        { "name": "mouse_move", "description": "Move the pointer (a hover).", "inputSchema": point("Target") },
        { "name": "left_click", "description": "Move the pointer there and click.", "inputSchema": point("Target") },
        { "name": "right_click", "description": "Move the pointer there and right-click.", "inputSchema": point("Target") },
        { "name": "middle_click", "description": "Move the pointer there and middle-click.", "inputSchema": point("Target") },
        { "name": "double_click", "description": "Move the pointer there and double-click.", "inputSchema": point("Target") },
        { "name": "drag", "description": "Press the left button at from, move to to, release.", "inputSchema": { "type": "object", "properties": {
            "from_x": { "type": "number" }, "from_y": { "type": "number" }, "to_x": { "type": "number" }, "to_y": { "type": "number" } }, "required": ["from_x", "from_y", "to_x", "to_y"] } },
        { "name": "scroll", "description": "Move the pointer there and turn the wheel.", "inputSchema": { "type": "object", "properties": {
            "x": { "type": "number" }, "y": { "type": "number" },
            "direction": { "type": "string", "enum": ["up", "down", "left", "right"] },
            "amount": { "type": "integer", "description": "Wheel notches (default 3)", "minimum": 1, "maximum": 50 } }, "required": ["x", "y", "direction"] } },
        { "name": "type", "description": format!("Type text into the focused control (at most {TYPE_MAX_CHARS} characters; a newline presses Return)."), "inputSchema": { "type": "object", "properties": { "text": { "type": "string" } }, "required": ["text"] } },
        { "name": "key", "description": "Press one key or combo: \"Return\", \"ctrl+s\", \"alt+Tab\", \"cmd+space\", \"F5\". Modifiers: ctrl, shift, alt, super/cmd.", "inputSchema": { "type": "object", "properties": { "combo": { "type": "string" } }, "required": ["combo"] } },
        { "name": "wait", "description": format!("Wait for the screen to settle (at most {WAIT_MAX_SECONDS} seconds), then take a screenshot."), "inputSchema": { "type": "object", "properties": { "seconds": { "type": "number", "minimum": 0, "maximum": WAIT_MAX_SECONDS } } } }
    ])
}

/// Every tool name.
#[cfg(test)]
pub(crate) fn names() -> Vec<String> {
    list().as_array().unwrap().iter().map(|tool| tool["name"].as_str().unwrap().to_string()).collect()
}

pub(crate) fn error(text: impl Into<String>) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }], "isError": true })
}

fn ok(text: impl Into<String>, structured: Value) -> Value {
    json!({ "content": [{ "type": "text", "text": text.into() }], "structuredContent": structured })
}

fn number(args: &Value, key: &str) -> Result<f64, String> {
    args.get(key).and_then(Value::as_f64).filter(|v| v.is_finite()).ok_or_else(|| format!("`{key}` must be a number."))
}

fn point(args: &Value, frame: Frame, x: &str, y: &str) -> Result<(i32, i32), String> {
    Ok(frame.to_screen(number(args, x)?, number(args, y)?))
}

/// What one call runs with: the run's abort flag (the person's Esc, read
/// between the steps of a long action), its display, remembered across
/// calls, the device's Work screen pick and the lease on the person's input.
pub(crate) struct CallContext<'a> {
    pub interrupted: &'a dyn Fn() -> bool,
    pub display: &'a mut Option<usize>,
    /// `launch_defaults.computerUseDisplay` (1-based; `None` = Auto).
    pub picked: Option<u32>,
    /// Takes the person's input; runs once the call passed the gate, so a
    /// refused call never locks them out. `Err` = the answer.
    pub engage: &'a dyn Fn() -> Result<(), String>,
}

/// The screen one call reads (NEW-59): the displays, the window list (read
/// only when the gate needs it, else empty) and the work displays.
struct Screen {
    displays: Vec<DisplayInfo>,
    windows: Result<Vec<WindowInfo>, String>,
    state: work_screen::State,
    work: Vec<usize>,
}

impl Screen {
    fn read(backend: &dyn Backend, name: &str, picked: Option<u32>) -> Result<Screen, String> {
        let displays = backend.displays()?;
        if displays.is_empty() {
            return Err("No display.".to_string());
        }
        // A connected pick is the work screen whatever is fullscreen: only
        // the input tools (the focus rules) and list_windows read windows.
        let pinned = picked.is_some_and(|number| number >= 1 && number as usize <= displays.len());
        let windows = if !pinned || work_screen::is_input(name) || name == "list_windows" {
            backend.list_windows()
        } else {
            Ok(Vec::new())
        };
        if let Err(reason) = &windows {
            log::debug!("[computer] no window list for the work screen ({reason})");
        }
        let mut state = work_screen::state_of(&displays, windows.as_deref().unwrap_or_default(), picked);
        // Fail closed (#1295 review): no list = no focus or fullscreen read.
        state.windows_read = windows.is_ok();
        let work = work_screen::work_displays(&state);
        Ok(Screen { displays, windows, state, work })
    }

    fn primary(&self) -> usize {
        self.displays.iter().position(|display| display.primary).unwrap_or(0)
    }

    fn displays_json(&self, current: usize) -> Value {
        Value::Array(
            self.work
                .iter()
                .filter_map(|&index| self.displays.get(index).map(|display| (index, display)))
                .map(|(index, display)| json!({ "index": index, "name": display.name, "width": display.width, "height": display.height, "primary": display.primary, "current": index == current }))
                .collect(),
        )
    }
}

/// The `wrong_display` answer (`work-screen.json`).
fn refusal(refusal: work_screen::Refusal, work: &[usize]) -> Value {
    json!({
        "content": [{ "type": "text", "text": refusal.text }],
        "isError": true,
        "structuredContent": { "refusal": "wrong_display", "kind": refusal.kind, "work": work }
    })
}

/// One call: the work-screen gate, the lease, the tool.
pub(crate) fn call(backend: &dyn Backend, name: &str, args: &Value, ctx: &mut CallContext<'_>) -> Value {
    let screen = match Screen::read(backend, name, ctx.picked) {
        Ok(screen) => screen,
        Err(text) => return error(text),
    };
    let mut shown = None;
    if name == "screenshot" {
        if let Some(index) = args.get("display").and_then(Value::as_u64) {
            let index = usize::try_from(index).unwrap_or(usize::MAX);
            if index >= screen.displays.len() {
                return error(format!("There is no display {index}; list_windows names {}.", screen.displays.len()));
            }
            shown = Some(index);
        }
    }
    let window_display = match (name, args.get("id").and_then(Value::as_u64), &screen.windows) {
        ("activate_window", Some(id), Ok(windows)) => windows
            .iter()
            .find(|window| window.id == id)
            .and_then(|window| work_screen::window_display(&screen.displays, window)),
        _ => None,
    };
    let id = args.get("id").and_then(Value::as_u64);
    let verdict = work_screen::gate(&screen.state, work_screen::Call { tool: name, id, display: shown, window_display });
    if let Err(refused) = verdict {
        return refusal(refused, &screen.work);
    }
    if shown.is_some() {
        *ctx.display = shown;
    }
    let current = work_screen::current(&screen.work, screen.primary(), *ctx.display);
    if current.is_some() {
        *ctx.display = current;
        work_screen::set_banner(current);
    }
    if let Err(text) = (ctx.engage)() {
        return error(text);
    }
    match dispatch(backend, name, args, ctx, &screen, current) {
        Ok(result) => result,
        Err(text) => error(text),
    }
}

fn dispatch(
    backend: &dyn Backend,
    name: &str,
    args: &Value,
    ctx: &mut CallContext<'_>,
    screen: &Screen,
    current: Option<usize>,
) -> Result<Value, String> {
    let interrupted = ctx.interrupted;
    if name == "wait" {
        let seconds = args.get("seconds").and_then(Value::as_f64).unwrap_or(1.0).clamp(0.0, WAIT_MAX_SECONDS);
        let until = std::time::Instant::now() + Duration::from_secs_f64(seconds);
        while std::time::Instant::now() < until {
            if interrupted() {
                return Err(ABORTED.to_string());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        // The screen after the wait: a fullscreen app may have closed.
        let screen = Screen::read(backend, "screenshot", ctx.picked)?;
        let Some(current) = work_screen::current(&screen.work, screen.primary(), *ctx.display) else {
            let text = work_screen::gate(&screen.state, work_screen::Call { tool: "screenshot", ..Default::default() })
                .err()
                .map(|refused| refused.text)
                .unwrap_or_default();
            return Ok(ok(text, json!({ "work": [] })));
        };
        *ctx.display = Some(current);
        work_screen::set_banner(Some(current));
        return screenshot(backend, &screen, current, Frame::fit(&screen.displays[current]));
    }
    // Every other tool passed the gate, so a work display exists.
    let current = current.ok_or("No work screen.")?;
    let displays = &screen.displays;
    let frame = Frame::fit(&displays[current]);
    match name {
        "screenshot" => screenshot(backend, screen, current, frame),
        "list_windows" => {
            let on_work = |w: &&WindowInfo| work_screen::window_display(displays, w).is_some_and(|index| screen.work.contains(&index));
            let windows: Vec<Value> = screen.windows.clone()?.iter().filter(on_work).map(|w| window_json(w, frame, displays)).collect();
            let mut lines: Vec<String> = windows
                .iter()
                .map(|w| format!("{} {}{} ({}, display {})", w["id"], w["title"], if w["active"] == true { " [active]" } else { "" }, w["app"], w["display"]))
                .collect();
            if lines.is_empty() {
                lines.push("No windows.".to_string());
            }
            if displays.len() > 1 {
                let work: Vec<String> = screen.work.iter().map(usize::to_string).collect();
                lines.push(format!("Displays: {} (current: {current}).", work.join(", ")));
            }
            Ok(ok(lines.join("\n"), json!({ "windows": windows, "displays": screen.displays_json(current) })))
        }
        "activate_window" => {
            let id = args.get("id").and_then(Value::as_u64).ok_or("`id` must be a list_windows id.")?;
            backend.activate_window(id)?;
            Ok(ok(format!("Window {id} activated."), json!({ "id": id })))
        }
        "cursor_position" => {
            let (x, y) = backend.cursor_position()?;
            let (fx, fy) = frame.to_frame(x, y);
            Ok(ok(format!("The pointer is at {fx},{fy}."), json!({ "x": fx, "y": fy })))
        }
        "mouse_move" => {
            let (x, y) = point(args, frame, "x", "y")?;
            backend.mouse_move(x, y)?;
            Ok(ok("Moved.", json!({})))
        }
        "left_click" | "right_click" | "middle_click" | "double_click" => {
            let (x, y) = point(args, frame, "x", "y")?;
            let (button, clicks) = match name {
                "right_click" => (Button::Right, 1),
                "middle_click" => (Button::Middle, 1),
                "double_click" => (Button::Left, 2),
                _ => (Button::Left, 1),
            };
            let held = match args.get("modifiers").and_then(Value::as_str).map(str::trim).filter(|m| !m.is_empty()) {
                Some(modifiers) => {
                    let combo = parse_combo(&format!("{modifiers}+Return"))?;
                    combo.modifiers
                }
                None => Vec::new(),
            };
            backend.mouse_move(x, y)?;
            std::thread::sleep(MOVE_SETTLE);
            for modifier in &held {
                backend.key(Key::Named(*modifier), true)?;
            }
            for click in 0..clicks {
                if click > 0 {
                    std::thread::sleep(CLICK_GAP);
                }
                backend.mouse_button(button, true)?;
                std::thread::sleep(CLICK_GAP);
                backend.mouse_button(button, false)?;
            }
            for modifier in held.iter().rev() {
                backend.key(Key::Named(*modifier), false)?;
            }
            Ok(ok("Clicked.", json!({})))
        }
        "drag" => {
            let from = point(args, frame, "from_x", "from_y")?;
            let to = point(args, frame, "to_x", "to_y")?;
            backend.mouse_move(from.0, from.1)?;
            std::thread::sleep(MOVE_SETTLE);
            backend.mouse_button(Button::Left, true)?;
            std::thread::sleep(CLICK_GAP);
            // A few intermediate points so drag-and-drop targets see motion.
            for step in 1..=8 {
                let t = f64::from(step) / 8.0;
                let x = f64::from(from.0) + (f64::from(to.0) - f64::from(from.0)) * t;
                let y = f64::from(from.1) + (f64::from(to.1) - f64::from(from.1)) * t;
                backend.mouse_move(x.round() as i32, y.round() as i32)?;
                std::thread::sleep(Duration::from_millis(15));
            }
            backend.mouse_button(Button::Left, false)?;
            Ok(ok("Dragged.", json!({})))
        }
        "scroll" => {
            let (x, y) = point(args, frame, "x", "y")?;
            let amount = args.get("amount").and_then(Value::as_i64).unwrap_or(3).clamp(1, 50) as i32;
            let (dx, dy) = match args.get("direction").and_then(Value::as_str) {
                Some("up") => (0, -amount),
                Some("down") => (0, amount),
                Some("left") => (-amount, 0),
                Some("right") => (amount, 0),
                _ => return Err("`direction` must be up, down, left or right.".to_string()),
            };
            backend.mouse_move(x, y)?;
            std::thread::sleep(MOVE_SETTLE);
            backend.scroll(dx, dy)?;
            Ok(ok("Scrolled.", json!({})))
        }
        "type" => {
            let text = args.get("text").and_then(Value::as_str).ok_or("`text` must be a string.")?;
            let count = text.chars().count();
            if count > TYPE_MAX_CHARS {
                return Err(format!("`text` is {count} characters; the limit is {TYPE_MAX_CHARS}."));
            }
            for (index, ch) in text.chars().enumerate() {
                if interrupted() {
                    return Err(ABORTED.to_string());
                }
                if index > 0 {
                    std::thread::sleep(TYPE_GAP);
                }
                match ch {
                    '\n' => press(backend, Key::Named(Named::Return))?,
                    '\t' => press(backend, Key::Named(Named::Tab))?,
                    '\r' => {}
                    ch => backend.type_char(ch)?,
                }
            }
            Ok(ok(format!("Typed {count} characters."), json!({ "characters": count })))
        }
        "key" => {
            let combo = parse_combo(args.get("combo").and_then(Value::as_str).unwrap_or_default())?;
            for modifier in &combo.modifiers {
                backend.key(Key::Named(*modifier), true)?;
            }
            let pressed = press(backend, combo.key);
            for modifier in combo.modifiers.iter().rev() {
                backend.key(Key::Named(*modifier), false)?;
            }
            pressed?;
            Ok(ok("Pressed.", json!({})))
        }
        other => Err(format!("Unknown tool {other}.")),
    }
}

/// What a call answers once the person pressed Esc.
pub(crate) const ABORTED: &str = "The person pressed Esc: computer use is aborted. Stop driving the screen and say so in your reply; drive again only when they ask.";

fn press(backend: &dyn Backend, key: Key) -> Result<(), String> {
    backend.key(key, true)?;
    std::thread::sleep(TYPE_GAP);
    backend.key(key, false)
}

fn window_json(w: &WindowInfo, frame: Frame, displays: &[DisplayInfo]) -> Value {
    let (x, y) = frame.to_frame(w.x, w.y);
    let (width, height) = frame.length_to_frame(w.width, w.height);
    let (cx, cy) = (w.x + w.width as i32 / 2, w.y + w.height as i32 / 2);
    let display = displays.iter().position(|display| display.contains(cx, cy)).unwrap_or(0);
    json!({ "id": w.id, "title": w.title, "app": w.app, "pid": w.pid, "display": display, "x": x, "y": y, "width": width, "height": height, "active": w.active })
}

fn screenshot(backend: &dyn Backend, screen: &Screen, current: usize, frame: Frame) -> Result<Value, String> {
    let shot = backend.screenshot(&screen.displays[current])?;
    let png = encode_png(&shot, frame.image)?;
    let which = if screen.displays.len() > 1 { format!(" of display {current} ({} work displays)", screen.work.len()) } else { String::new() };
    Ok(json!({
        "content": [
            { "type": "text", "text": format!("Screenshot {}x{}{} (its pixels are {}x{}); coordinates are in this image's pixels.", frame.image.0, frame.image.1, which, frame.screen.0, frame.screen.1) },
            { "type": "image", "data": base64(&png), "mimeType": "image/png" }
        ],
        "structuredContent": { "width": frame.image.0, "height": frame.image.1, "display": current, "displays": screen.displays_json(current) }
    }))
}

/// Fit `shot` to `size` and encode it.
pub(crate) fn encode_png(shot: &Image, size: (u32, u32)) -> Result<Vec<u8>, String> {
    let source = image::RgbaImage::from_raw(shot.width, shot.height, shot.rgba.clone())
        .ok_or("The screenshot's buffer does not match its size.")?;
    let fitted = if (shot.width, shot.height) == size {
        source
    } else {
        image::imageops::resize(&source, size.0, size.1, image::imageops::FilterType::Triangle)
    };
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(fitted)
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|err| format!("Could not encode the screenshot: {err}"))?;
    Ok(png.into_inner())
}

/// Standard base64 (RFC 4648, padded); small enough to own.
pub(crate) fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, b)| acc | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A backend that records what it was asked: a 2000x1000 primary display
    /// and a 1000x1000 second one to its right.
    #[derive(Default)]
    pub(crate) struct FakeBackend {
        pub log: Mutex<Vec<String>>,
        pub fail_lock: bool,
        /// A fullscreen game focused on display B.
        pub gaming: bool,
        /// The window list fails (#1295 review: input fails closed).
        pub no_windows: bool,
    }

    impl Backend for FakeBackend {
        fn displays(&self) -> Result<Vec<DisplayInfo>, String> {
            Ok(vec![
                DisplayInfo { x: 0, y: 0, width: 2000, height: 1000, primary: true, name: "A".into() },
                DisplayInfo { x: 2000, y: 0, width: 1000, height: 1000, primary: false, name: "B".into() },
            ])
        }
        fn screenshot(&self, display: &DisplayInfo) -> Result<Image, String> {
            self.log.lock().unwrap().push(format!("shot {}", display.name));
            Ok(Image { width: display.width, height: display.height, rgba: vec![0; (display.width * display.height * 4) as usize] })
        }
        fn cursor_position(&self) -> Result<(i32, i32), String> {
            Ok((1000, 500))
        }
        fn list_windows(&self) -> Result<Vec<WindowInfo>, String> {
            if self.no_windows {
                return Err("no window list".into());
            }
            let brave = WindowInfo { id: 7, title: "Brave".into(), app: "brave".into(), pid: Some(42), x: 200, y: 100, width: 1000, height: 500, active: !self.gaming, fullscreen: false };
            if !self.gaming {
                return Ok(vec![brave]);
            }
            // The person's game, fullscreen and focused on display B.
            let game = WindowInfo { id: 9, title: "Game".into(), app: "game".into(), pid: Some(43), x: 2000, y: 0, width: 1000, height: 1000, active: true, fullscreen: true };
            Ok(vec![game, brave])
        }
        fn activate_window(&self, id: u64) -> Result<(), String> {
            self.log.lock().unwrap().push(format!("activate {id}"));
            Ok(())
        }
        fn mouse_move(&self, x: i32, y: i32) -> Result<(), String> {
            self.log.lock().unwrap().push(format!("move {x},{y}"));
            Ok(())
        }
        fn mouse_button(&self, button: Button, down: bool) -> Result<(), String> {
            self.log.lock().unwrap().push(format!("{button:?} {}", if down { "down" } else { "up" }));
            Ok(())
        }
        fn scroll(&self, dx: i32, dy: i32) -> Result<(), String> {
            self.log.lock().unwrap().push(format!("scroll {dx},{dy}"));
            Ok(())
        }
        fn key(&self, key: Key, down: bool) -> Result<(), String> {
            self.log.lock().unwrap().push(format!("key {key:?} {}", if down { "down" } else { "up" }));
            Ok(())
        }
        fn type_char(&self, ch: char) -> Result<(), String> {
            self.log.lock().unwrap().push(format!("type {ch}"));
            Ok(())
        }
        fn lock(&self, _on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Box<dyn crate::backend::LockGuard>, String> {
            if self.fail_lock {
                return Err("Another program holds the keyboard.".to_string());
            }
            struct Guard;
            impl crate::backend::LockGuard for Guard {}
            self.log.lock().unwrap().push("lock".to_string());
            Ok(Box::new(Guard))
        }
    }

    fn never() -> bool {
        false
    }

    fn engaged() -> Result<(), String> {
        Ok(())
    }

    /// A call with a fresh display memory.
    fn one(backend: &dyn Backend, name: &str, args: Value) -> Value {
        let mut display = None;
        call(backend, name, &args, &mut CallContext { interrupted: &never, display: &mut display, picked: None, engage: &engaged })
    }

    #[test]
    fn the_frame_fits_1280x800_and_maps_both_ways() {
        let display = |x, y, width, height| DisplayInfo { x, y, width, height, primary: true, name: String::new() };
        let frame = Frame::fit(&display(0, 0, 2560, 1440));
        assert_eq!(frame.image, (1280, 720));
        assert_eq!(frame.to_screen(640.0, 360.0), (1280, 720));
        assert_eq!(frame.to_frame(2559, 1439), (1280, 720));
        assert_eq!(frame.to_screen(-5.0, 9999.0), (0, 1439));
        // A second display to the right: its frame starts at its origin.
        let right = Frame::fit(&display(2560, 0, 2560, 1440));
        assert_eq!(right.to_screen(0.0, 0.0), (2560, 0));
        assert_eq!(right.to_screen(-100.0, 0.0), (2560, 0));
        assert_eq!(right.to_frame(0, 0), (-1280, 0));
        // Never upscaled, tall screens bound by height.
        assert_eq!(Frame::fit(&display(0, 0, 1000, 600)).image, (1000, 600));
        assert_eq!(Frame::fit(&display(0, 0, 1080, 1920)).image, (450, 800));
    }

    #[test]
    fn a_click_moves_in_screen_units_with_the_modifiers_held() {
        let backend = FakeBackend::default();
        let reply = one(&backend, "left_click", json!({ "x": 640, "y": 400, "modifiers": "ctrl" }));
        assert_eq!(reply["isError"], Value::Null, "{reply}");
        assert_eq!(
            backend.log.lock().unwrap().as_slice(),
            ["move 1000,625", "key Named(Control) down", "Left down", "Left up", "key Named(Control) up"]
        );
        backend.log.lock().unwrap().clear();
        one(&backend, "double_click", json!({ "x": 0, "y": 0 }));
        assert_eq!(backend.log.lock().unwrap().as_slice(), ["move 0,0", "Left down", "Left up", "Left down", "Left up"]);
    }

    #[test]
    fn the_screenshot_is_a_fitted_png_and_windows_report_in_frame_pixels() {
        let backend = FakeBackend::default();
        let shot = one(&backend, "screenshot", json!({}));
        assert_eq!(shot["structuredContent"]["width"], 1280);
        assert_eq!(shot["structuredContent"]["height"], 640);
        assert_eq!(shot["structuredContent"]["display"], 0);
        let data = shot["content"][1]["data"].as_str().unwrap();
        assert!(data.starts_with("iVBORw0KGgo"), "a PNG");
        let windows = one(&backend, "list_windows", json!({}));
        assert_eq!(windows["structuredContent"]["windows"][0], json!({ "id": 7, "title": "Brave", "app": "brave", "pid": 42, "display": 0, "x": 128, "y": 64, "width": 640, "height": 320, "active": true }));
        assert_eq!(windows["structuredContent"]["displays"][1]["current"], false);
        let cursor = one(&backend, "cursor_position", json!({}));
        assert_eq!(cursor["structuredContent"], json!({ "x": 640, "y": 320 }));
    }

    #[test]
    fn a_run_switches_display_with_the_screenshot_and_keeps_it() {
        let backend = FakeBackend::default();
        let mut display = None;
        let mut ctx = CallContext { interrupted: &never, display: &mut display, picked: None, engage: &engaged };
        let shot = call(&backend, "screenshot", &json!({ "display": 1 }), &mut ctx);
        assert_eq!(shot["structuredContent"]["width"], 800);
        assert_eq!(shot["structuredContent"]["display"], 1);
        call(&backend, "left_click", &json!({ "x": 0, "y": 0 }), &mut ctx);
        assert_eq!(display, Some(1));
        let mut ctx = CallContext { interrupted: &never, display: &mut display, picked: None, engage: &engaged };
        let missing = call(&backend, "screenshot", &json!({ "display": 9 }), &mut ctx);
        assert_eq!(missing["isError"], true);
        assert_eq!(backend.log.lock().unwrap().as_slice(), ["shot B", "move 2000,0", "Left down", "Left up"]);
    }

    #[test]
    fn type_presses_return_for_newlines_and_stops_when_interrupted() {
        let backend = FakeBackend::default();
        one(&backend, "type", json!({ "text": "a\nb" }));
        assert_eq!(
            backend.log.lock().unwrap().as_slice(),
            ["type a", "key Named(Return) down", "key Named(Return) up", "type b"]
        );
        backend.log.lock().unwrap().clear();
        let mut display = None;
        let reply = call(&backend, "type", &json!({ "text": "abc" }), &mut CallContext { interrupted: &|| true, display: &mut display, picked: None, engage: &engaged });
        assert_eq!(reply["isError"], true);
        assert!(backend.log.lock().unwrap().is_empty());
        let too_long = one(&backend, "type", json!({ "text": "x".repeat(TYPE_MAX_CHARS + 1) }));
        assert_eq!(too_long["isError"], true);
    }

    #[test]
    fn key_combos_scroll_and_unknown_tools() {
        let backend = FakeBackend::default();
        one(&backend, "key", json!({ "combo": "ctrl+s" }));
        assert_eq!(
            backend.log.lock().unwrap().as_slice(),
            ["key Named(Control) down", "key Char('s') down", "key Char('s') up", "key Named(Control) up"]
        );
        backend.log.lock().unwrap().clear();
        one(&backend, "scroll", json!({ "x": 10, "y": 10, "direction": "up", "amount": 2 }));
        assert_eq!(backend.log.lock().unwrap().as_slice(), ["move 16,16", "scroll 0,-2"]);
        assert_eq!(one(&backend, "scroll", json!({ "x": 10, "y": 10, "direction": "sideways" }))["isError"], true);
        assert_eq!(one(&backend, "nope", json!({}))["isError"], true);
        assert_eq!(one(&backend, "left_click", json!({ "x": "ten" }))["isError"], true);
    }

    #[test]
    fn a_fullscreen_game_refuses_input_without_locking_and_hides_its_display() {
        let backend = FakeBackend { gaming: true, ..Default::default() };
        let engaged = std::cell::Cell::new(0);
        let engage = || {
            engaged.set(engaged.get() + 1);
            Ok(())
        };
        let mut display = None;
        let mut run = |name: &str, args: Value, picked: Option<u32>| {
            call(&backend, name, &args, &mut CallContext { interrupted: &never, display: &mut display, picked, engage: &engage })
        };
        let click = run("left_click", json!({ "x": 1, "y": 1 }), None);
        assert_eq!(click["isError"], true);
        assert_eq!(click["structuredContent"], json!({ "refusal": "wrong_display", "kind": "gaming", "work": [0] }));
        assert!(click["content"][0]["text"].as_str().unwrap().starts_with("The person is in a fullscreen app on display 1."));
        let shot = run("screenshot", json!({ "display": 1 }), None);
        assert_eq!(shot["structuredContent"]["kind"], "notWorkScreen");
        let activate = run("activate_window", json!({ "id": 9 }), None);
        assert_eq!(activate["structuredContent"]["kind"], "windowElsewhere");
        // An id list_windows never named (a tool window, a stale HWND).
        let unknown = run("activate_window", json!({ "id": 4242 }), None);
        assert_eq!(unknown["structuredContent"]["kind"], "unknownWindow");
        // A picked display that is gone: nothing runs, never Auto.
        let gone = run("left_click", json!({ "x": 1, "y": 1 }), Some(3));
        assert_eq!(gone["structuredContent"]["kind"], "pickedGone");
        assert_eq!(engaged.get(), 0, "a refusal never locks the person out");
        assert!(backend.log.lock().unwrap().is_empty());
        // list_windows: only display A and its windows.
        let windows = run("list_windows", json!({}), None);
        assert_eq!(windows["structuredContent"]["windows"].as_array().unwrap().len(), 1);
        assert_eq!(windows["structuredContent"]["windows"][0]["id"], 7);
        assert_eq!(windows["structuredContent"]["displays"].as_array().unwrap().len(), 1);
        assert_eq!(windows["structuredContent"]["displays"][0]["index"], 0);
        // The device picked display 2: its fullscreen app is the agent's own.
        let click = run("left_click", json!({ "x": 0, "y": 0 }), Some(2));
        assert_eq!(click["isError"], Value::Null, "{click}");
        assert_eq!(engaged.get(), 2);
        assert_eq!(display, Some(1));
        assert_eq!(backend.log.lock().unwrap().first().map(String::as_str), Some("move 2000,0"));
    }

    #[test]
    fn an_unreadable_window_list_refuses_input_but_still_shows_the_screen() {
        let backend = FakeBackend { no_windows: true, ..Default::default() };
        let never = || false;
        let engaged = std::cell::Cell::new(0);
        let engage = || {
            engaged.set(engaged.get() + 1);
            Ok(())
        };
        let mut display = None;
        let mut run = |name: &str, args: Value| {
            call(&backend, name, &args, &mut CallContext { interrupted: &never, display: &mut display, picked: None, engage: &engage })
        };
        for (name, args) in [("type", json!({ "text": "x" })), ("left_click", json!({ "x": 1, "y": 1 })), ("list_windows", json!({}))] {
            let refused = run(name, args);
            assert_eq!(refused["structuredContent"]["kind"], "noWindowList", "{name}");
        }
        assert_eq!(engaged.get(), 0);
        assert_eq!(run("screenshot", json!({}))["isError"], Value::Null);
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn every_tool_has_a_schema() {
        for tool in list().as_array().unwrap() {
            assert_eq!(tool["inputSchema"]["type"], "object", "{}", tool["name"]);
            assert!(tool["description"].as_str().unwrap().len() > 10);
        }
        assert!(names().contains(&"screenshot".to_string()));
    }
}
