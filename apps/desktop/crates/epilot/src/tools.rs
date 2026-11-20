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

use crate::backend::{Backend, Button, ButtonWait, DisplayInfo, Image, Pasted, Target, WindowInfo};
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
/// A `paste_text`'s ceiling.
const PASTE_MAX_CHARS: usize = 100_000;
/// How long the focused window gets to take a paste, and a copy to land.
const CLIPBOARD_TIMEOUT: Duration = Duration::from_secs(2);
/// After the target took a paste: it may pull a second format.
const PASTE_SETTLE: Duration = Duration::from_millis(150);

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
        { "name": "type", "description": format!("Type text into the focused control key by key (at most {TYPE_MAX_CHARS} characters; a newline presses Return). For more than a few characters, or a secret, use paste_text."), "inputSchema": { "type": "object", "properties": { "text": { "type": "string" } }, "required": ["text"] } },
        { "name": "paste_text", "description": format!("Paste text into the focused control in one step: the clipboard is saved, set to the text, the paste shortcut pressed once, then the old clipboard put back. Prefer it to `type` for anything longer than a few characters (at most {PASTE_MAX_CHARS} characters). `sensitive: true` for a secret (password, token, key): clipboard managers and the OS clipboard history never see it, and the answer never repeats it. `file` instead of `text` reads the text from a local file (one trailing newline dropped), so a secret never enters the conversation."), "inputSchema": { "type": "object", "properties": {
            "text": { "type": "string", "description": "The text (or give `file`)" },
            "file": { "type": "string", "description": "An absolute path whose contents are the text" },
            "sensitive": { "type": "boolean", "description": "A secret: kept out of clipboard managers and history, never echoed (default false)" },
            "shortcut": { "type": "string", "description": format!("The paste combo when the app uses another one, e.g. \"ctrl+shift+v\" in a terminal (default {DEFAULT_PASTE})") } } } },
        { "name": "copy_selection", "description": "Copy the focused window's current selection and answer its text; the person's clipboard is put back afterwards. Select first (drag, double_click, ctrl+a).", "inputSchema": { "type": "object", "properties": {
            "shortcut": { "type": "string", "description": format!("The copy combo when the app uses another one, e.g. \"ctrl+shift+c\" in a terminal (default {DEFAULT_COPY})") } } } },
        { "name": "read_clipboard", "description": "The clipboard's text, only right after your own last click or key copied something (an app's Copy button, a copy shortcut). Never the person's clipboard: anything else is refused.", "inputSchema": none },
        { "name": "key", "description": "Press one key or combo: \"Return\", \"ctrl+s\", \"alt+Tab\", \"cmd+space\", \"F5\". Modifiers: ctrl, shift, alt, super/cmd.", "inputSchema": { "type": "object", "properties": { "combo": { "type": "string" } }, "required": ["combo"] } },
        { "name": "wait", "description": format!("Wait for the screen to settle (at most {WAIT_MAX_SECONDS} seconds), then take a screenshot."), "inputSchema": { "type": "object", "properties": { "seconds": { "type": "number", "minimum": 0, "maximum": WAIT_MAX_SECONDS } } } }
    ])
}

/// #EDE-126: `tools/list` of one run. A space run's adds `launch_app` and
/// says "this space's clipboard" where the person's list speaks of the
/// person's; a person run's is [`list`], unchanged.
pub(crate) fn list_for(space: bool) -> Value {
    let mut tools = list();
    if !space {
        return tools;
    }
    let all = tools.as_array_mut().expect("the tools array");
    for tool in all.iter_mut() {
        let description = match tool["name"].as_str() {
            Some("paste_text") => format!("Paste text into the focused control in one step: this space's clipboard is set to the text and the paste shortcut pressed once. Prefer it to `type` for anything longer than a few characters (at most {PASTE_MAX_CHARS} characters). `sensitive: true` for a secret (password, token, key): the answer never repeats it. `file` instead of `text` reads the text from a local file (one trailing newline dropped), so a secret never enters the conversation."),
            Some("copy_selection") => "Copy the focused window's current selection and answer its text. Select first (drag, double_click, ctrl+a).".to_string(),
            Some("read_clipboard") => "The text on this space's clipboard: what an app's Copy button or a copy shortcut put there.".to_string(),
            _ => continue,
        };
        tool["description"] = json!(description);
    }
    all.push(json!({ "name": "launch_app", "description": "Start a program in this space: `argv` is the command and its arguments (no shell). Its window opens here; take a screenshot to see it.", "inputSchema": { "type": "object", "properties": {
        "argv": { "type": "array", "items": { "type": "string" }, "minItems": 1, "description": "The command, then its arguments" },
        "cwd": { "type": "string", "description": "An absolute directory to start it in (default: the home directory)" } }, "required": ["argv"] } }));
    tools
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
    /// The clipboard's change count just before the run's last click or
    /// key: `read_clipboard` reads only a change made since (the run's own
    /// copy). `None` = nothing armed.
    pub copy_mark: &'a mut Option<u64>,
    /// `launch_defaults.computerUseDisplayId` (a display id; `None` = Auto).
    pub picked: Option<String>,
    /// Takes the person's input; runs once a DRIVING call
    /// (`work_screen::drives`) passed the gate, so a refused call never
    /// locks them out and a look or a wait never does. `Err` = the answer.
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
    fn read(backend: &dyn Backend, name: &str, picked: Option<&str>) -> Result<Screen, String> {
        let displays = backend.displays()?;
        if displays.is_empty() {
            return Err("No display.".to_string());
        }
        // #EDE-126: a space is the run's own: all of it is its work screen,
        // whatever is fullscreen there, and no pick of the person's applies.
        if in_space(backend) {
            let windows = if name == "list_windows" { backend.list_windows() } else { Ok(Vec::new()) };
            let state = work_screen::state_of(&displays, &[], None);
            let work = (0..displays.len()).collect();
            return Ok(Screen { displays, windows, state, work });
        }
        // #EXP-1514: the pick Agent space only: nothing is done here, so
        // nothing is read either; the gate refuses it first.
        if picked == Some(work_screen::SPACE_PICK) {
            let mut state = work_screen::state_of(&displays, &[], None);
            state.space_only = true;
            return Ok(Screen { displays, windows: Ok(Vec::new()), state, work: Vec::new() });
        }
        // #FEED-161: the stored id names the display, whatever its index now.
        let picked = work_screen::resolve(&displays, picked);
        // A connected pick is the work screen whatever is fullscreen: only
        // the input tools (the focus rules) and list_windows read windows.
        let pinned = picked.is_some_and(|number| number >= 1);
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

/// #EDE-126: the backend drives an agent space, not the person's seat:
/// no work-screen gate, no banner, no second focus read, and the clipboard
/// there is the run's own.
fn in_space(backend: &dyn Backend) -> bool {
    backend.target() != Target::Person
}

/// One call: the work-screen gate, the lease, the tool.
pub(crate) fn call(backend: &dyn Backend, name: &str, args: &Value, ctx: &mut CallContext<'_>) -> Value {
    let space = in_space(backend);
    let screen = match Screen::read(backend, name, ctx.picked.as_deref()) {
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
    if !space {
        let verdict = work_screen::gate(&screen.state, work_screen::Call { tool: name, id, display: shown, window_display });
        if let Err(refused) = verdict {
            return refusal(refused, &screen.work);
        }
    }
    if shown.is_some() {
        *ctx.display = shown;
    }
    let current = work_screen::current(&screen.work, screen.primary(), *ctx.display);
    if current.is_some() {
        *ctx.display = current;
        if !space {
            work_screen::set_banner(backend.target(), current);
        }
    }
    // #EXP-1898: looking and waiting leave the person's input alone.
    if work_screen::drives(name) {
        if let Err(text) = (ctx.engage)() {
            return error(text);
        }
    }
    // #EXP-1895: the gate read the focus before the hold was taken, and it
    // may have moved since: the keys go where the focus is NOW.
    if work_screen::is_key(name) && !space {
        let now = match Screen::read(backend, name, ctx.picked.as_deref()) {
            Ok(now) => now,
            Err(text) => return error(text),
        };
        if let Err(refused) = work_screen::gate(&now.state, work_screen::Call { tool: name, ..Default::default() }) {
            return refusal(refused, &now.work);
        }
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
        let screen = Screen::read(backend, "screenshot", ctx.picked.as_deref())?;
        // The screenshot it answers with passes the gate too (#FEED-161: in
        // Auto without a window list no display is known to be free).
        let space = in_space(backend);
        let gated = if space { Ok(()) } else { work_screen::gate(&screen.state, work_screen::Call { tool: "screenshot", ..Default::default() }) };
        if let Err(refused) = gated {
            return Ok(ok(refused.text, json!({ "work": [] })));
        }
        let Some(current) = work_screen::current(&screen.work, screen.primary(), *ctx.display) else {
            return Ok(ok("No work screen.".to_string(), json!({ "work": [] })));
        };
        *ctx.display = Some(current);
        if !space {
            work_screen::set_banner(backend.target(), Some(current));
        }
        return screenshot(backend, &screen, current, Frame::fit(&screen.displays[current]));
    }
    // Every other tool passed the gate, so a work display exists.
    let current = current.ok_or("No work screen.")?;
    // #EXP-1979: the pointer never moves while the person still holds a
    // button they held when the hold engaged: it would drag what they hold.
    if work_screen::is_pointer(name) {
        match crate::backend::wait_for_person_buttons(|| backend.person_holds_button(), interrupted, crate::backend::PERSON_BUTTON_WAIT) {
            ButtonWait::Free => {}
            ButtonWait::Interrupted => return Err(ABORTED.to_string()),
            ButtonWait::Held => return Err(PERSON_HOLDS_A_BUTTON.to_string()),
        }
    }
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
            *ctx.copy_mark = clipboard_mark(backend);
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
            let mut pressed = Held::new(backend);
            for modifier in &held {
                pressed.key(Key::Named(*modifier))?;
            }
            for click in 0..clicks {
                if click > 0 {
                    std::thread::sleep(CLICK_GAP);
                }
                pressed.button(button)?;
                std::thread::sleep(CLICK_GAP);
                pressed.up()?;
            }
            pressed.release()?;
            Ok(ok("Clicked.", json!({})))
        }
        "drag" => {
            let from = point(args, frame, "from_x", "from_y")?;
            let to = point(args, frame, "to_x", "to_y")?;
            backend.mouse_move(from.0, from.1)?;
            std::thread::sleep(MOVE_SETTLE);
            let mut pressed = Held::new(backend);
            pressed.button(Button::Left)?;
            std::thread::sleep(CLICK_GAP);
            // A few intermediate points so drag-and-drop targets see motion.
            for step in 1..=8 {
                let t = f64::from(step) / 8.0;
                let x = f64::from(from.0) + (f64::from(to.0) - f64::from(from.0)) * t;
                let y = f64::from(from.1) + (f64::from(to.1) - f64::from(from.1)) * t;
                backend.mouse_move(x.round() as i32, y.round() as i32)?;
                std::thread::sleep(Duration::from_millis(15));
            }
            pressed.release()?;
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
            *ctx.copy_mark = clipboard_mark(backend);
            press_combo(backend, &combo)?;
            Ok(ok("Pressed.", json!({})))
        }
        "paste_text" => {
            let text = paste_source(args)?;
            let sensitive = args.get("sensitive").and_then(Value::as_bool).unwrap_or(false);
            let combo = shortcut(args, DEFAULT_PASTE)?;
            *ctx.copy_mark = None;
            paste(backend, &text, sensitive, &combo, interrupted)
        }
        "copy_selection" => {
            let combo = shortcut(args, DEFAULT_COPY)?;
            *ctx.copy_mark = None;
            copy(backend, &combo, interrupted)
        }
        // #EDE-126: a space run's tool only; elsewhere it is no tool.
        "launch_app" if in_space(backend) => {
            let argv: Option<Vec<String>> = args.get("argv").and_then(Value::as_array).filter(|argv| !argv.is_empty()).and_then(|argv| argv.iter().map(|arg| arg.as_str().map(str::to_string)).collect());
            let argv = argv.ok_or("`argv` must be the command and its arguments, as strings.")?;
            let pid = backend.launch(&argv, args.get("cwd").and_then(Value::as_str))?;
            Ok(ok(format!("Started {} (pid {pid}). Take a screenshot to see its window.", argv[0]), json!({ "pid": pid })))
        }
        // #EDE-126: a space's clipboard holds only what the run put there.
        "read_clipboard" if in_space(backend) => {
            let text = backend.clipboard()?.text()?.ok_or("The clipboard holds no text.")?;
            let count = text.chars().count();
            Ok(ok(text, json!({ "characters": count })))
        }
        "read_clipboard" => {
            let Some(mark) = ctx.copy_mark.take() else {
                return Err(READ_REFUSED.to_string());
            };
            let clipboard = backend.clipboard()?;
            if clipboard.change_count()? == mark {
                // Not yet: an app may copy a moment after the click.
                *ctx.copy_mark = Some(mark);
                return Err("The clipboard has not changed since your last click or key, so there is nothing of yours to read yet.".to_string());
            }
            let text = clipboard.text()?.ok_or("What was copied is not text.")?;
            let count = text.chars().count();
            Ok(ok(text, json!({ "characters": count })))
        }
        other => Err(format!("Unknown tool {other}.")),
    }
}

/// The platform's paste and copy combos.
const DEFAULT_PASTE: &str = if cfg!(target_os = "macos") { "cmd+v" } else { "ctrl+v" };
const DEFAULT_COPY: &str = if cfg!(target_os = "macos") { "cmd+c" } else { "ctrl+c" };

pub(crate) const READ_REFUSED: &str = "read_clipboard reads only what your own last click or key copied (an app's Copy button, a copy shortcut), never the person's clipboard. Copy first, or use copy_selection.";

/// One thing a tool holds down.
#[derive(Clone, Copy)]
enum Press {
    Key(Key),
    Button(Button),
}

/// #ERR-29: the error of a release that failed twice.
const STAYS_PRESSED: &str = "[computer] a key or button stays pressed after its release retries";
/// #ERR-29: the error of a paste or copy that left the agent's text (a
/// secret, after a sensitive paste) on the person's clipboard.
const CLIPBOARD_NOT_BACK: &str = "[computer] the person's clipboard was not put back";

/// Put the person's clipboard back; a failure is an error (#ERR-29), never
/// with the clipboard's text.
fn restore_clipboard(clipboard: &dyn crate::backend::Clipboard, saved: crate::backend::SavedClipboard) -> Result<(), String> {
    let restored = clipboard.restore(saved);
    if let Err(err) = &restored {
        log::warn!("[computer] clipboard restore: {err}");
        log::error!("{CLIPBOARD_NOT_BACK}");
    }
    restored
}

/// What a tool holds down on the person's seat (#EXP-1895): released in
/// reverse on EVERY exit path, a failed step's early return included, so
/// no button or modifier stays pressed. A release that fails is tried once
/// more.
struct Held<'a> {
    backend: &'a dyn Backend,
    pressed: Vec<Press>,
}

impl<'a> Held<'a> {
    fn new(backend: &'a dyn Backend) -> Self {
        Held { backend, pressed: Vec::new() }
    }

    fn key(&mut self, key: Key) -> Result<(), String> {
        self.backend.key(key, true)?;
        self.pressed.push(Press::Key(key));
        Ok(())
    }

    fn button(&mut self, button: Button) -> Result<(), String> {
        self.backend.mouse_button(button, true)?;
        self.pressed.push(Press::Button(button));
        Ok(())
    }

    /// Release the last press.
    fn up(&mut self) -> Result<(), String> {
        let Some(press) = self.pressed.pop() else {
            return Ok(());
        };
        let release = || match press {
            Press::Key(key) => self.backend.key(key, false),
            Press::Button(button) => self.backend.mouse_button(button, false),
        };
        let released = release().or_else(|_| release());
        if let Err(err) = &released {
            // #ERR-29: still down after the retries: the person's key or
            // button stays pressed. An error (a GlitchTip event), its text
            // fixed so they group; the backend's words in the warning.
            log::warn!("[computer] a release failed after its retry ({err})");
            if !self.backend.lets_go_itself() {
                log::error!("{STAYS_PRESSED}");
            }
        }
        released
    }

    /// Release everything, last press first; the first failure is the answer.
    fn release_all(&mut self) -> Result<(), String> {
        let mut result = Ok(());
        while !self.pressed.is_empty() {
            result = result.and(self.up());
        }
        result
    }

    fn release(mut self) -> Result<(), String> {
        self.release_all()
    }
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        if let Err(err) = self.release_all() {
            log::warn!("[computer] a release failed after a failed step ({err})");
        }
    }
}

fn press_combo(backend: &dyn Backend, combo: &crate::keys::Combo) -> Result<(), String> {
    let mut held = Held::new(backend);
    for modifier in &combo.modifiers {
        held.key(Key::Named(*modifier))?;
    }
    let pressed = press(backend, combo.key);
    held.release()?;
    pressed
}

fn shortcut(args: &Value, default: &str) -> Result<crate::keys::Combo, String> {
    parse_combo(args.get("shortcut").and_then(Value::as_str).map(str::trim).filter(|combo| !combo.is_empty()).unwrap_or(default))
}

/// The clipboard's change count now, where the desktop has one (a space
/// run reads its clipboard without a mark).
fn clipboard_mark(backend: &dyn Backend) -> Option<u64> {
    if in_space(backend) {
        return None;
    }
    backend.clipboard().and_then(|clipboard| clipboard.change_count()).ok()
}

/// `paste_text`'s `file` (#EXP-1895): its size is checked before a byte is
/// read, and the read itself stops just past the limit (a path whose size
/// reads 0, a file that grows).
fn read_paste_file(path: &str) -> Result<String, String> {
    use std::io::Read;
    let unreadable = |err: std::io::Error| format!("Could not read `file` ({err}).");
    let too_large = || format!("`file` is over the limit of {PASTE_MAX_CHARS} characters.");
    // A character is at most 4 bytes; one trailing line end is dropped.
    let limit = (PASTE_MAX_CHARS * 4 + 2) as u64;
    let file = std::fs::File::open(path).map_err(unreadable)?;
    if file.metadata().map_err(unreadable)?.len() > limit {
        return Err(too_large());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).map_err(unreadable)?;
    if bytes.len() as u64 > limit {
        return Err(too_large());
    }
    String::from_utf8(bytes).map_err(|err| format!("Could not read `file` ({err})."))
}

/// `paste_text`'s text: `text`, or `file`'s contents less one trailing
/// newline. Never logged or echoed.
fn paste_source(args: &Value) -> Result<String, String> {
    let text = match (args.get("text").and_then(Value::as_str), args.get("file").and_then(Value::as_str)) {
        (Some(text), None) => text.to_string(),
        (None, Some(path)) => {
            let mut text = read_paste_file(path)?;
            if text.ends_with('\n') {
                text.pop();
                if text.ends_with('\r') {
                    text.pop();
                }
            }
            text
        }
        _ => return Err("Give either `text` or `file`.".to_string()),
    };
    let count = text.chars().count();
    if text.is_empty() {
        return Err("There is no text to paste.".to_string());
    }
    if count > PASTE_MAX_CHARS {
        return Err(format!("The text is {count} characters; the limit is {PASTE_MAX_CHARS}."));
    }
    Ok(text)
}

/// The person's clipboard, saved whole, or a refusal BEFORE anything touched
/// it (#EXP-1900): a save that left a format out (a large image, one that
/// cannot be read) would come back as a smaller clipboard, so the tool does
/// not start. The one policy for every backend; they only report
/// [`crate::backend::SavedClipboard::partial`]. `nothing` = what did not
/// happen, `instead` = what the agent does now.
fn save_whole(clipboard: &dyn crate::backend::Clipboard, nothing: &str, instead: &str) -> Result<crate::backend::SavedClipboard, String> {
    let saved = clipboard.save().map_err(|err| format!("Could not keep the clipboard ({err}); {nothing}."))?;
    if saved.partial {
        return Err(format!("The person's clipboard holds something that cannot be saved whole (a large image, or a type that cannot be read), so it was left untouched and {nothing}. {instead}"));
    }
    Ok(saved)
}

/// `paste_text` and `copy_selection` in an agent space (#EDE-126): the
/// clipboard there is the run's own, so nothing is saved or put back.
fn space_paste(backend: &dyn Backend, text: &str, sensitive: bool, combo: &crate::keys::Combo, interrupted: &dyn Fn() -> bool) -> Result<Value, String> {
    let clipboard = backend.clipboard()?;
    clipboard.set_text(text, sensitive)?;
    if interrupted() {
        return Err(ABORTED.to_string());
    }
    press_combo(backend, combo)?;
    match clipboard.wait_pasted(CLIPBOARD_TIMEOUT) {
        Pasted::Missed => return Err("The focused window did not take the paste (is a text field focused?).".to_string()),
        Pasted::Taken => std::thread::sleep(PASTE_SETTLE),
        Pasted::Settled => {}
    }
    let what = if sensitive { "the sensitive text".to_string() } else { format!("{} characters", text.chars().count()) };
    Ok(ok(format!("Pasted {what}."), json!({})))
}

fn space_copy(backend: &dyn Backend, combo: &crate::keys::Combo, interrupted: &dyn Fn() -> bool) -> Result<Value, String> {
    let clipboard = backend.clipboard()?;
    let before = clipboard.change_count()?;
    press_combo(backend, combo)?;
    let until = std::time::Instant::now() + CLIPBOARD_TIMEOUT;
    while clipboard.change_count()? == before {
        if interrupted() {
            return Err(ABORTED.to_string());
        }
        if std::time::Instant::now() >= until {
            return Err("Nothing was copied: the selection is empty, not text, or the window ignored the shortcut.".to_string());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let text = clipboard.text()?.ok_or("What was copied is not text.")?;
    let count = text.chars().count();
    Ok(ok(text, json!({ "characters": count })))
}

/// Save, set, press once, put back. The person's clipboard is back
/// whatever happens after it was saved.
fn paste(backend: &dyn Backend, text: &str, sensitive: bool, combo: &crate::keys::Combo, interrupted: &dyn Fn() -> bool) -> Result<Value, String> {
    if in_space(backend) {
        return space_paste(backend, text, sensitive, combo, interrupted);
    }
    let clipboard = backend.clipboard()?;
    let saved = save_whole(&*clipboard, "nothing was pasted", "Use `type` for this text.")?;
    let pasted = (|| {
        clipboard.set_text(text, sensitive)?;
        if interrupted() {
            return Err(ABORTED.to_string());
        }
        press_combo(backend, combo)?;
        let pasted = clipboard.wait_pasted(CLIPBOARD_TIMEOUT);
        if pasted == Pasted::Taken {
            std::thread::sleep(PASTE_SETTLE);
        }
        Ok(pasted)
    })();
    let restored = restore_clipboard(&*clipboard, saved);
    let pasted = pasted?;
    let note = match &restored {
        Ok(()) => "the clipboard is as it was".to_string(),
        Err(err) => format!("the old clipboard could not be put back ({err})"),
    };
    if pasted == Pasted::Missed {
        return Err(format!("The focused window did not take the paste (is a text field focused?); {note}."));
    }
    let what = if sensitive { "the sensitive text".to_string() } else { format!("{} characters", text.chars().count()) };
    Ok(ok(format!("Pasted {what}; {note}."), json!({ "restored": restored.is_ok() })))
}

/// Press copy, wait for the clipboard to change, read it, put it back.
fn copy(backend: &dyn Backend, combo: &crate::keys::Combo, interrupted: &dyn Fn() -> bool) -> Result<Value, String> {
    if in_space(backend) {
        return space_copy(backend, combo, interrupted);
    }
    let clipboard = backend.clipboard()?;
    let saved = save_whole(&*clipboard, "nothing was copied", "Read the text from a screenshot instead.")?;
    let copied = (|| {
        let before = clipboard.change_count()?;
        press_combo(backend, combo)?;
        let until = std::time::Instant::now() + CLIPBOARD_TIMEOUT;
        while clipboard.change_count()? == before {
            if interrupted() {
                return Err(ABORTED.to_string());
            }
            if std::time::Instant::now() >= until {
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        clipboard.text()
    })();
    let restored = restore_clipboard(&*clipboard, saved);
    let Some(text) = copied? else {
        return Err("Nothing was copied: the selection is empty, not text, or the window ignored the shortcut.".to_string());
    };
    let count = text.chars().count();
    Ok(ok(text, json!({ "characters": count, "restored": restored.is_ok() })))
}

/// What a call answers once the person pressed Esc.
/// #EXP-1979: a pointer tool whose wait for the person's button ran out.
pub(crate) const PERSON_HOLDS_A_BUTTON: &str = "The person is holding a mouse button, so nothing was moved or clicked. Try again once they let go.";

pub(crate) const ABORTED: &str = "The person pressed Esc: computer use is aborted. Stop driving the screen and say so in your reply; drive again only when they ask.";

fn press(backend: &dyn Backend, key: Key) -> Result<(), String> {
    let mut held = Held::new(backend);
    held.key(key)?;
    std::thread::sleep(TYPE_GAP);
    held.release()
}

fn window_json(w: &WindowInfo, frame: Frame, displays: &[DisplayInfo]) -> Value {
    let (x, y) = frame.to_frame(w.x, w.y);
    let (width, height) = frame.length_to_frame(w.width, w.height);
    let display = work_screen::window_display(displays, w).unwrap_or(0);
    json!({ "id": w.id, "title": w.title, "app": w.app, "pid": w.pid, "display": display, "x": x, "y": y, "width": width, "height": height, "active": w.active })
}

fn screenshot(backend: &dyn Backend, screen: &Screen, current: usize, frame: Frame) -> Result<Value, String> {
    let shot = backend.screenshot(&screen.displays[current])?;
    let png = encode_png(&shot, frame.image)?;
    let which = if screen.displays.len() > 1 { format!(" of display {current} ({} work displays)", screen.work.len()) } else { String::new() };
    // #EDE-162: an app whose GPU path failed in the space, said once.
    let notice = backend.take_notice().map(|n| format!(" {n}")).unwrap_or_default();
    Ok(json!({
        "content": [
            { "type": "text", "text": format!("Screenshot {}x{}{} (its pixels are {}x{}); coordinates are in this image's pixels.{notice}", frame.image.0, frame.image.1, which, frame.screen.0, frame.screen.1) },
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
    use crate::backend::Clipboard;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
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
        pub clip: Mutex<FakeClip>,
        /// The input step (a move, a button, a key, a scroll, a typed
        /// character; 0-based) that fails, once (#EXP-1895).
        pub fail_step: Option<usize>,
        pub steps: AtomicUsize,
        /// The person's fullscreen game took the focus mid-call.
        pub focus_moved: AtomicBool,
        /// It drives this agent space, not the person's seat (#EDE-126).
        pub space: Option<u32>,
        /// `launch` fails with this (the compositor's refusal).
        pub no_launch: Option<String>,
        /// The person holds a mouse button they held at engage (#EXP-1979).
        pub holding: AtomicBool,
        /// #ERR-29: every release fails (a key that stays down).
        pub fail_ups: bool,
        /// #ERR-29: the compositor releases what it holds (eDE).
        pub lets_go: bool,
        /// #EDE-162: what the space's compositor says with the next screenshot.
        pub notice: Mutex<Option<String>>,
    }

    impl FakeBackend {
        /// One input step: logged, or failed when it is `fail_step`.
        fn step(&self, line: String) -> Result<(), String> {
            if self.fail_ups && line.ends_with(" up") {
                return Err(format!("`{line}` stays pressed."));
            }
            if self.fail_step == Some(self.steps.fetch_add(1, Ordering::SeqCst)) {
                return Err(format!("the compositor refused `{line}`"));
            }
            self.log.lock().unwrap().push(line);
            Ok(())
        }
    }

    /// The fake clipboard: its text, change count, what the focused app
    /// copies on ctrl+c and whether it takes a paste.
    #[derive(Default)]
    pub(crate) struct FakeClip {
        pub text: Option<String>,
        pub count: u64,
        pub copies: Option<String>,
        pub refuses_paste: bool,
        /// It also holds something `save` cannot keep (a large image).
        pub partial: bool,
        /// #ERR-29: putting the saved clipboard back fails.
        pub fails_restore: bool,
    }

    struct FakeClipboard<'a>(&'a FakeBackend);

    impl FakeClipboard<'_> {
        fn log(&self, line: String) {
            self.0.log.lock().unwrap().push(line);
        }
    }

    impl Clipboard for FakeClipboard<'_> {
        fn change_count(&self) -> Result<u64, String> {
            Ok(self.0.clip.lock().unwrap().count)
        }
        fn save(&self) -> Result<crate::backend::SavedClipboard, String> {
            self.log("clip save".into());
            let clip = self.0.clip.lock().unwrap();
            Ok(crate::backend::SavedClipboard { formats: clip.text.clone().map(|text| vec![("text".to_string(), text.into_bytes())]).unwrap_or_default(), partial: clip.partial })
        }
        fn set_text(&self, text: &str, sensitive: bool) -> Result<(), String> {
            self.log(format!("clip set {}", if sensitive { "<secret>" } else { text }));
            let mut clip = self.0.clip.lock().unwrap();
            clip.text = Some(text.to_string());
            clip.count += 1;
            Ok(())
        }
        fn wait_pasted(&self, _timeout: Duration) -> Pasted {
            if self.0.clip.lock().unwrap().refuses_paste {
                Pasted::Missed
            } else {
                Pasted::Taken
            }
        }
        fn restore(&self, saved: crate::backend::SavedClipboard) -> Result<(), String> {
            self.log(format!("clip restore {}", saved.formats.len()));
            let mut clip = self.0.clip.lock().unwrap();
            if clip.fails_restore {
                return Err("the clipboard's new owner went away".to_string());
            }
            clip.text = saved.formats.into_iter().next().map(|(_, bytes)| String::from_utf8(bytes).unwrap());
            clip.count += 1;
            Ok(())
        }
        fn text(&self) -> Result<Option<String>, String> {
            Ok(self.0.clip.lock().unwrap().text.clone())
        }
    }

    impl Backend for FakeBackend {
        fn person_holds_button(&self) -> bool {
            self.holding.load(Ordering::SeqCst)
        }
        fn target(&self) -> crate::backend::Target {
            self.space.map_or(crate::backend::Target::Person, crate::backend::Target::Space)
        }
        fn lets_go_itself(&self) -> bool {
            self.lets_go
        }
        fn take_notice(&self) -> Option<String> {
            self.notice.lock().unwrap().take()
        }
        fn displays(&self) -> Result<Vec<DisplayInfo>, String> {
            Ok(vec![
                DisplayInfo { id: "uuid-A".into(), x: 0, y: 0, width: 2000, height: 1000, primary: true, name: "A".into(), connector: None },
                DisplayInfo { id: "uuid-B".into(), x: 2000, y: 0, width: 1000, height: 1000, primary: false, name: "B".into(), connector: None },
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
            let gaming = self.gaming || self.focus_moved.load(Ordering::SeqCst);
            let brave = WindowInfo { id: 7, title: "Brave".into(), app: "brave".into(), pid: Some(42), x: 200, y: 100, width: 1000, height: 500, active: !gaming, fullscreen: false };
            if !gaming {
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
            self.step(format!("move {x},{y}"))
        }
        fn mouse_button(&self, button: Button, down: bool) -> Result<(), String> {
            self.step(format!("{button:?} {}", if down { "down" } else { "up" }))
        }
        fn scroll(&self, dx: i32, dy: i32) -> Result<(), String> {
            self.step(format!("scroll {dx},{dy}"))
        }
        fn key(&self, key: Key, down: bool) -> Result<(), String> {
            self.step(format!("key {key:?} {}", if down { "down" } else { "up" }))?;
            let mut clip = self.clip.lock().unwrap();
            if key == Key::Char('c') && down {
                if let Some(copied) = clip.copies.clone() {
                    clip.text = Some(copied);
                    clip.count += 1;
                }
            }
            Ok(())
        }
        fn clipboard(&self) -> Result<Box<dyn Clipboard + '_>, String> {
            Ok(Box::new(FakeClipboard(self)))
        }
        fn launch(&self, argv: &[String], cwd: Option<&str>) -> Result<u32, String> {
            if let Some(refused) = &self.no_launch {
                return Err(refused.clone());
            }
            self.log.lock().unwrap().push(format!("launch {argv:?} {cwd:?}"));
            Ok(77)
        }
        fn char_stroke(&self, ch: char, down: bool) -> Result<(), String> {
            self.step(format!("stroke {ch} {}", if down { "down" } else { "up" }))
        }
        // One line per character, without the hold: the tools' tests read it.
        fn type_char(&self, ch: char) -> Result<(), String> {
            self.step(format!("type {ch}"))
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
        call(backend, name, &args, &mut CallContext { interrupted: &never, display: &mut display, copy_mark: &mut None, picked: None, engage: &engaged })
    }

    /// #EDE-126: a space is the run's own: no work-screen gate (nothing of
    /// the person's is on it), every display of it is its work screen, no
    /// banner, and input never fails closed on the person's window list.
    #[test]
    fn a_space_run_passes_no_gate_and_sets_no_banner() {
        use crate::backend::Target;
        let key = json!({ "combo": "a" });
        // On the person's seat this is refused: their fullscreen game has the focus.
        let person = FakeBackend { gaming: true, ..Default::default() };
        assert_eq!(one(&person, "key", key.clone())["isError"], true);
        let space = FakeBackend { gaming: true, space: Some(41), ..Default::default() };
        let reply = one(&space, "key", key);
        assert_eq!(reply["isError"], Value::Null, "{reply}");
        let listed = one(&space, "list_windows", json!({}));
        assert_eq!(listed["structuredContent"]["displays"].as_array().unwrap().len(), 2, "{listed}");
        assert_eq!(listed["structuredContent"]["windows"].as_array().unwrap().len(), 2, "{listed}");
        let waited = one(&space, "wait", json!({ "seconds": 0 }));
        assert_eq!(waited["content"][1]["type"], "image", "{waited}");
        assert_eq!(work_screen::banner_index(Target::Space(41)), None);
        let blind = FakeBackend { no_windows: true, space: Some(41), ..Default::default() };
        let reply = one(&blind, "left_click", json!({ "x": 10, "y": 10 }));
        assert_eq!(reply["isError"], Value::Null, "{reply}");
    }

    /// #EDE-126: `launch_app` is a space run's tool only: listed there,
    /// with no word about the person's clipboard in that list; the
    /// person's list is today's. It hands argv and cwd to the backend (the
    /// compositor decides the environment) and answers the pid.
    #[test]
    fn launch_app_is_a_space_runs_tool() {
        assert_eq!(list_for(false), list());
        assert!(!names().contains(&"launch_app".to_string()));
        let space = list_for(true);
        let tools = space.as_array().unwrap();
        assert_eq!(tools.len(), names().len() + 1);
        assert_eq!(tools.last().unwrap()["name"], "launch_app");
        assert_eq!(tools.last().unwrap()["inputSchema"]["required"], json!(["argv"]));
        assert!(!space.to_string().contains("person"), "a space run reads nothing about the person's clipboard");
        let backend = FakeBackend { space: Some(44), ..Default::default() };
        let reply = one(&backend, "launch_app", json!({ "argv": ["foot", "-e", "top"], "cwd": "/tmp" }));
        assert_eq!(reply["structuredContent"]["pid"], 77, "{reply}");
        assert_eq!(backend.log.lock().unwrap().as_slice(), [r#"launch ["foot", "-e", "top"] Some("/tmp")"#]);
        for bad in [json!({}), json!({ "argv": [] }), json!({ "argv": ["foot", 3] }), json!({ "argv": "foot" })] {
            assert_eq!(one(&backend, "launch_app", bad.clone())["isError"], true, "{bad}");
        }
        assert_eq!(backend.log.lock().unwrap().len(), 1);
        let refusing = FakeBackend { space: Some(44), no_launch: Some("eDE launch: not_found: no such program".into()), ..Default::default() };
        assert_eq!(one(&refusing, "launch_app", json!({ "argv": ["nope"] }))["content"][0]["text"], "eDE launch: not_found: no such program");
        // On the person's seat it is no tool at all.
        let person = FakeBackend::default();
        assert_eq!(one(&person, "launch_app", json!({ "argv": ["foot"] }))["isError"], true);
        assert!(person.log.lock().unwrap().is_empty());
    }

    /// #EDE-126: a space's clipboard is the run's own: a paste and a copy
    /// save and restore nothing, and a read needs no copy of its own first.
    #[test]
    fn a_space_runs_paste_copy_and_read_keep_no_clipboard() {
        let clip = || Mutex::new(FakeClip { text: Some("old".into()), copies: Some("picked".into()), ..Default::default() });
        let space = FakeBackend { space: Some(42), clip: clip(), ..Default::default() };
        let pasted = one(&space, "paste_text", json!({ "text": "hi" }));
        assert_eq!(pasted["content"][0]["text"], "Pasted 2 characters.", "{pasted}");
        assert_eq!(space.clip.lock().unwrap().text.as_deref(), Some("hi"), "nothing was put back");
        let copied = one(&space, "copy_selection", json!({}));
        assert_eq!(copied["content"][0]["text"], "picked", "{copied}");
        let log = space.log.lock().unwrap().clone();
        assert!(!log.iter().any(|line| line.starts_with("clip save") || line.starts_with("clip restore")), "{log:?}");
        assert_eq!(one(&space, "read_clipboard", json!({}))["content"][0]["text"], "picked");
        // The person's clipboard is never read without the run's own copy.
        let person = FakeBackend { clip: clip(), ..Default::default() };
        assert_eq!(one(&person, "read_clipboard", json!({}))["content"][0]["text"], READ_REFUSED);
    }

    #[test]
    fn the_frame_fits_1280x800_and_maps_both_ways() {
        let display = |x, y, width, height| DisplayInfo { id: String::new(), x, y, width, height, primary: true, name: String::new(), connector: None };
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

    /// #EDE-162: an app whose GPU path failed in the space is named with
    /// the next screenshot, once, so the agent starts it again.
    #[test]
    fn a_space_notice_comes_with_the_next_screenshot_once() {
        let backend = FakeBackend { space: Some(41), ..Default::default() };
        *backend.notice.lock().unwrap() = Some("vkcube ended at start on the GPU (exit 134); this space now draws in software: start it again.".into());
        let shot = one(&backend, "screenshot", json!({}));
        let text = shot["content"][0]["text"].as_str().unwrap();
        assert!(text.ends_with("vkcube ended at start on the GPU (exit 134); this space now draws in software: start it again."), "{text}");
        let again = one(&backend, "screenshot", json!({}));
        assert!(!again["content"][0]["text"].as_str().unwrap().contains("vkcube"), "said once");
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
        let mut ctx = CallContext { interrupted: &never, display: &mut display, copy_mark: &mut None, picked: None, engage: &engaged };
        let shot = call(&backend, "screenshot", &json!({ "display": 1 }), &mut ctx);
        assert_eq!(shot["structuredContent"]["width"], 800);
        assert_eq!(shot["structuredContent"]["display"], 1);
        call(&backend, "left_click", &json!({ "x": 0, "y": 0 }), &mut ctx);
        assert_eq!(display, Some(1));
        let mut ctx = CallContext { interrupted: &never, display: &mut display, copy_mark: &mut None, picked: None, engage: &engaged };
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
        let reply = call(&backend, "type", &json!({ "text": "abc" }), &mut CallContext { interrupted: &|| true, display: &mut display, copy_mark: &mut None, picked: None, engage: &engaged });
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

    /// What the log left pressed: every `down` without a later `up`.
    fn still_pressed(backend: &FakeBackend) -> Vec<String> {
        let mut pressed: Vec<String> = Vec::new();
        for line in backend.log.lock().unwrap().iter() {
            if let Some(what) = line.strip_suffix(" down") {
                pressed.push(what.to_string());
            } else if let Some(what) = line.strip_suffix(" up") {
                if let Some(index) = pressed.iter().rposition(|held| held == what) {
                    pressed.remove(index);
                }
            }
        }
        pressed
    }

    /// #EXP-1895: `name` with the backend failing at each of its steps in
    /// turn: whatever step fails, every press has its release.
    fn fails_at_every_step(name: &str, args: Value) {
        for fail in 0.. {
            let backend = FakeBackend { fail_step: Some(fail), ..Default::default() };
            backend.clip.lock().unwrap().copies = Some("copied".to_string());
            one(&backend, name, args.clone());
            assert_eq!(still_pressed(&backend), Vec::<String>::new(), "{name} failing at step {fail}: {:?}", backend.log.lock().unwrap());
            if backend.steps.load(Ordering::SeqCst) <= fail {
                assert!(fail > 0, "{name} never reached the backend");
                return;
            }
        }
    }

    /// #ERR-29: a release that fails after its retry is an error, the
    /// level GlitchTip sends (the person's key or button stays down).
    #[test]
    fn a_release_that_keeps_failing_is_an_error() {
        crate::log_capture::start();
        let backend = FakeBackend { fail_ups: true, ..Default::default() };
        let reply = one(&backend, "left_click", json!({ "x": 10, "y": 10 }));
        assert_eq!(reply["isError"], true, "{reply}");
        assert!(crate::log_capture::error_logged(STAYS_PRESSED), "{:?}", crate::log_capture::records());
    }

    /// #ERR-29: on eDE the compositor releases what a run drove (and refuses
    /// a release after the person's Esc): a refused release is no stuck key.
    #[test]
    fn a_refused_release_is_no_error_where_the_compositor_lets_go() {
        crate::log_capture::start();
        let backend = FakeBackend { fail_ups: true, lets_go: true, ..Default::default() };
        let reply = one(&backend, "left_click", json!({ "x": 10, "y": 10 }));
        assert_eq!(reply["isError"], true, "{reply}");
        assert!(!crate::log_capture::error_logged(STAYS_PRESSED), "{:?}", crate::log_capture::records());
    }

    /// #ERR-29: a clipboard that was not put back is an error, without its text.
    #[test]
    fn a_clipboard_not_put_back_is_an_error() {
        crate::log_capture::start();
        let backend = FakeBackend::default();
        person_clipboard(&backend, "the person's");
        backend.clip.lock().unwrap().fails_restore = true;
        let reply = one(&backend, "paste_text", json!({ "text": "hunter2", "sensitive": true }));
        assert!(reply.to_string().contains("could not be put back"), "{reply}");
        assert!(crate::log_capture::error_logged(CLIPBOARD_NOT_BACK), "{:?}", crate::log_capture::records());
        assert!(!crate::log_capture::records().iter().any(|(_, text)| text.contains("hunter2")), "never the text");
    }

    #[test]
    fn a_failed_step_in_a_drag_releases_the_button() {
        fails_at_every_step("drag", json!({ "from_x": 10, "from_y": 10, "to_x": 90, "to_y": 50 }));
    }

    #[test]
    fn a_failed_step_in_a_modifier_click_releases_the_button_and_the_modifiers() {
        fails_at_every_step("left_click", json!({ "x": 10, "y": 10, "modifiers": "ctrl+shift" }));
        fails_at_every_step("double_click", json!({ "x": 10, "y": 10, "modifiers": "shift" }));
        fails_at_every_step("right_click", json!({ "x": 10, "y": 10 }));
    }

    #[test]
    fn a_failed_step_in_a_key_combo_releases_the_modifiers() {
        fails_at_every_step("key", json!({ "combo": "ctrl+shift+s" }));
        fails_at_every_step("paste_text", json!({ "text": "hello" }));
        fails_at_every_step("copy_selection", json!({}));
    }

    /// #EXP-1895: the gate read the focus before the hold was taken; the
    /// person's game took it meanwhile, so the keys would land in it.
    #[test]
    fn a_key_tool_checks_the_focus_again_once_the_hold_is_taken() {
        for (name, args) in [
            ("key", json!({ "combo": "ctrl+s" })),
            ("type", json!({ "text": "a" })),
            ("paste_text", json!({ "text": "a" })),
            ("copy_selection", json!({})),
        ] {
            let backend = FakeBackend::default();
            let engage = || {
                backend.focus_moved.store(true, Ordering::SeqCst);
                Ok(())
            };
            let mut display = None;
            let reply = call(&backend, name, &args, &mut CallContext { interrupted: &never, display: &mut display, copy_mark: &mut None, picked: None, engage: &engage });
            assert_eq!(reply["structuredContent"]["refusal"], "wrong_display", "{name}: {reply}");
            assert_eq!(reply["structuredContent"]["kind"], "gaming");
            assert!(backend.log.lock().unwrap().is_empty(), "{name}: {:?}", backend.log.lock().unwrap());
        }
    }

    /// #EXP-1895: a `file` over the limit is refused by its size, unread.
    #[test]
    fn paste_text_stats_a_file_before_reading_it() {
        let dir = std::env::temp_dir().join(format!("epilot-paste-size-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let huge = dir.join("huge");
        // Sparse: 64 MiB that take no disk.
        std::fs::File::create(&huge).unwrap().set_len(64 << 20).unwrap();
        let refused = paste_source(&json!({ "file": huge.to_str().unwrap() }));
        let small = dir.join("small");
        std::fs::write(&small, "secret\r\n").unwrap();
        let read = paste_source(&json!({ "file": small.to_str().unwrap() }));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(refused, Err(format!("`file` is over the limit of {PASTE_MAX_CHARS} characters.")));
        assert_eq!(read.as_deref(), Ok("secret"));
        // A path whose size reads 0 and never ends: the read is bounded too.
        #[cfg(unix)]
        assert_eq!(paste_source(&json!({ "file": "/dev/zero" })), Err(format!("`file` is over the limit of {PASTE_MAX_CHARS} characters.")));
    }

    #[test]
    /// #EXP-1514 (`spaceGateCases`, row 11): with the pick Agent space only
    /// every seat call but `wait` is refused `spaceOnly`, first, whatever is
    /// connected; nothing is read, driven or locked.
    #[test]
    fn the_pick_agent_space_only_refuses_every_seat_call_but_wait() {
        let backend = FakeBackend::default();
        let engaged = std::cell::Cell::new(0);
        let engage = || {
            engaged.set(engaged.get() + 1);
            Ok(())
        };
        let mut display = None;
        let mut run = |name: &str, args: Value| {
            call(&backend, name, &args, &mut CallContext { interrupted: &never, display: &mut display, copy_mark: &mut None, picked: Some(work_screen::SPACE_PICK.to_string()), engage: &engage })
        };
        let text = "This device keeps agents in agent spaces. Nothing is done on its screen.";
        for (name, args) in [("screenshot", json!({})), ("left_click", json!({ "x": 1, "y": 1 })), ("list_windows", json!({})), ("type", json!({ "text": "a" })), ("activate_window", json!({ "id": 7 }))] {
            let refused = run(name, args);
            assert_eq!(refused["structuredContent"], json!({ "refusal": "wrong_display", "kind": "spaceOnly", "work": [] }), "{name}");
            assert_eq!(refused["content"][0]["text"], text);
        }
        assert_eq!(run("wait", json!({ "seconds": 0 }))["isError"], Value::Null);
        assert_eq!(engaged.get(), 0, "never the person's input");
        assert!(backend.log.lock().unwrap().is_empty(), "nothing done on the screen");
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
        let mut run = |name: &str, args: Value, picked: Option<&str>| {
            let picked = picked.map(str::to_string);
            call(&backend, name, &args, &mut CallContext { interrupted: &never, display: &mut display, copy_mark: &mut None, picked, engage: &engage })
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
        let gone = run("left_click", json!({ "x": 1, "y": 1 }), Some("uuid-unplugged"));
        assert_eq!(gone["structuredContent"]["kind"], "pickedGone");
        assert_eq!(run("screenshot", json!({}), Some("uuid-unplugged"))["structuredContent"]["kind"], "pickedGone");
        assert_eq!(engaged.get(), 0, "a refusal never locks the person out");
        assert!(backend.log.lock().unwrap().is_empty());
        // list_windows: only display A and its windows.
        let windows = run("list_windows", json!({}), None);
        assert_eq!(windows["structuredContent"]["windows"].as_array().unwrap().len(), 1);
        assert_eq!(windows["structuredContent"]["windows"][0]["id"], 7);
        assert_eq!(windows["structuredContent"]["displays"].as_array().unwrap().len(), 1);
        assert_eq!(windows["structuredContent"]["displays"][0]["index"], 0);
        // #EXP-1898: looking and waiting never take the person's input, so
        // their game keeps its keys and its pointer.
        for (name, args) in [("screenshot", json!({})), ("cursor_position", json!({})), ("wait", json!({ "seconds": 0 }))] {
            let looked = run(name, args, None);
            assert_eq!(looked["isError"], Value::Null, "{name}: {looked}");
        }
        assert_eq!(engaged.get(), 0, "a look or a wait locked the person out");
        // The device picked display B (by its id): its fullscreen app is the agent's own.
        let click = run("left_click", json!({ "x": 0, "y": 0 }), Some("uuid-B"));
        assert_eq!(click["isError"], Value::Null, "{click}");
        assert_eq!(engaged.get(), 1, "an input tool takes the hold");
        assert_eq!(display, Some(1));
        assert_eq!(backend.log.lock().unwrap().iter().find(|line| line.starts_with("move ")).map(String::as_str), Some("move 2000,0"));
    }

    #[test]
    fn an_unreadable_window_list_refuses_everything_in_auto_but_a_pick_still_shows_the_screen() {
        let backend = FakeBackend { no_windows: true, ..Default::default() };
        let never = || false;
        let engaged = std::cell::Cell::new(0);
        let engage = || {
            engaged.set(engaged.get() + 1);
            Ok(())
        };
        let mut display = None;
        let mut run = |name: &str, args: Value, picked: Option<&str>| {
            let picked = picked.map(str::to_string);
            call(&backend, name, &args, &mut CallContext { interrupted: &never, display: &mut display, copy_mark: &mut None, picked, engage: &engage })
        };
        for picked in [None, Some("uuid-A")] {
            for (name, args) in [("type", json!({ "text": "x" })), ("left_click", json!({ "x": 1, "y": 1 })), ("list_windows", json!({}))] {
                let refused = run(name, args, picked);
                assert_eq!(refused["structuredContent"]["kind"], "noWindowList", "{name} {picked:?}");
            }
        }
        // #FEED-161: Auto without a window list knows no free display.
        assert_eq!(run("screenshot", json!({}), None)["structuredContent"]["kind"], "noWindowList");
        assert_eq!(engaged.get(), 0);
        let waited = run("wait", json!({ "seconds": 0 }), None);
        assert_eq!(waited["isError"], Value::Null);
        assert_eq!(waited["structuredContent"]["work"], json!([]), "no screenshot after the wait: {waited}");
        assert_eq!(engaged.get(), 0, "a wait without a work screen locked the person out");
        let shot = run("screenshot", json!({}), Some("uuid-B"));
        assert_eq!(shot["isError"], Value::Null);
        assert_eq!(shot["structuredContent"]["display"], 1);
        assert_eq!(engaged.get(), 0);
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }


    fn person_clipboard(backend: &FakeBackend, text: &str) {
        let mut clip = backend.clip.lock().unwrap();
        clip.text = Some(text.to_string());
        clip.count = 10;
    }

    /// #EXP-1979: while the person holds a mouse button they held when the
    /// hold engaged, a pointer tool waits for its release (then acts); keys
    /// and screenshots never wait; Esc meanwhile aborts.
    #[test]
    fn the_pointer_waits_while_the_person_holds_a_button() {
        assert!(["mouse_move", "left_click", "right_click", "middle_click", "double_click", "drag", "scroll"].into_iter().all(work_screen::is_pointer));
        assert!(!["type", "key", "paste_text", "copy_selection", "activate_window", "screenshot", "wait"].into_iter().any(work_screen::is_pointer));
        let backend = FakeBackend::default();
        backend.holding.store(true, Ordering::SeqCst);
        let shot = one(&backend, "screenshot", json!({}));
        assert_eq!(shot["isError"], Value::Null, "{shot}");
        let key = one(&backend, "key", json!({ "combo": "Return" }));
        assert_eq!(key["isError"], Value::Null, "{key}");
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(150));
                assert!(backend.log.lock().unwrap().iter().all(|line| !line.starts_with("move")), "nothing moved while held");
                backend.holding.store(false, Ordering::SeqCst);
            });
            let clicked = one(&backend, "left_click", json!({ "x": 10, "y": 10 }));
            assert_eq!(clicked["isError"], Value::Null, "{clicked}");
        });
        assert!(backend.log.lock().unwrap().iter().any(|line| line.starts_with("move")));
        backend.holding.store(true, Ordering::SeqCst);
        let mut display = None;
        let aborted = call(&backend, "mouse_move", &json!({ "x": 1, "y": 1 }), &mut CallContext { interrupted: &|| true, display: &mut display, copy_mark: &mut None, picked: None, engage: &engaged });
        assert_eq!(aborted["isError"], true);
        assert_eq!(aborted["content"][0]["text"], ABORTED);
    }

    #[test]
    fn paste_text_saves_sets_presses_once_and_restores() {
        let backend = FakeBackend::default();
        person_clipboard(&backend, "the person's");
        let reply = one(&backend, "paste_text", json!({ "text": "hello" }));
        assert_eq!(reply["isError"], Value::Null, "{reply}");
        // The paste key is the OS's: cmd+v on a Mac.
        let paste = if cfg!(target_os = "macos") { "Super" } else { "Control" };
        assert_eq!(
            backend.log.lock().unwrap().as_slice(),
            [
                "clip save".to_string(),
                "clip set hello".to_string(),
                format!("key Named({paste}) down"),
                "key Char('v') down".to_string(),
                "key Char('v') up".to_string(),
                format!("key Named({paste}) up"),
                "clip restore 1".to_string(),
            ]
        );
        assert_eq!(backend.clip.lock().unwrap().text.as_deref(), Some("the person's"));
        assert!(reply["content"][0]["text"].as_str().unwrap().starts_with("Pasted 5 characters"));
    }

    #[test]
    fn a_sensitive_paste_never_echoes_and_a_missed_paste_still_restores() {
        let backend = FakeBackend::default();
        person_clipboard(&backend, "before");
        let dir = std::env::temp_dir().join(format!("epilot-paste-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("secret");
        std::fs::write(&file, "hunter2\n").unwrap();
        let reply = one(&backend, "paste_text", json!({ "file": file, "sensitive": true, "shortcut": "ctrl+shift+v" }));
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(reply["isError"], Value::Null, "{reply}");
        assert!(!reply.to_string().contains("hunter2"), "{reply}");
        let log = backend.log.lock().unwrap().join("|");
        assert!(log.contains("clip set <secret>") && log.contains("key Named(Shift) down") && log.ends_with("clip restore 1"), "{log}");
        assert_eq!(backend.clip.lock().unwrap().text.as_deref(), Some("before"));

        backend.clip.lock().unwrap().refuses_paste = true;
        let missed = one(&backend, "paste_text", json!({ "text": "x" }));
        assert_eq!(missed["isError"], true);
        assert_eq!(backend.clip.lock().unwrap().text.as_deref(), Some("before"));
        assert_eq!(one(&backend, "paste_text", json!({}))["isError"], true);
        assert_eq!(one(&backend, "paste_text", json!({ "text": "a", "file": "/x" }))["isError"], true);
        assert_eq!(one(&backend, "paste_text", json!({ "text": "x".repeat(PASTE_MAX_CHARS + 1) }))["isError"], true);
    }

    /// #EXP-1900: a clipboard that cannot be saved whole (a large image
    /// beside the text) is never touched: no set, no key, no restore, and
    /// the refusal says what to do instead.
    #[test]
    fn a_clipboard_that_cannot_be_saved_whole_is_left_untouched() {
        let backend = FakeBackend::default();
        *backend.clip.lock().unwrap() = FakeClip { text: Some("mine".into()), count: 3, copies: Some("picked".into()), partial: true, ..FakeClip::default() };
        for (tool, args, instead) in [("paste_text", json!({ "text": "hello" }), "nothing was pasted. Use `type` for this text."), ("copy_selection", json!({}), "nothing was copied. Read the text from a screenshot instead.")] {
            let reply = one(&backend, tool, args);
            assert_eq!(reply["isError"], true, "{tool}: {reply}");
            let text = reply["content"][0]["text"].as_str().unwrap();
            assert!(text.starts_with("The person's clipboard holds something that cannot be saved whole") && text.ends_with(instead), "{tool}: {text}");
            assert!(!text.contains("as it was"), "{tool}: {text}");
            assert_eq!(backend.log.lock().unwrap().as_slice(), ["clip save"], "{tool} touched the clipboard or pressed a key");
            backend.log.lock().unwrap().clear();
            let clip = backend.clip.lock().unwrap();
            assert_eq!((clip.text.as_deref(), clip.count), (Some("mine"), 3), "{tool}");
        }
        // Saved whole, the same clipboard is pasted over and put back.
        backend.clip.lock().unwrap().partial = false;
        let reply = one(&backend, "paste_text", json!({ "text": "hello" }));
        assert_eq!(reply["isError"], Value::Null, "{reply}");
        assert_eq!(reply["content"][0]["text"], "Pasted 5 characters; the clipboard is as it was.");
        let log = backend.log.lock().unwrap().join(" | ");
        assert!(log.starts_with("clip save | clip set hello") && log.contains("key Char('v') down") && log.ends_with("clip restore 1"), "{log}");
        assert_eq!(backend.clip.lock().unwrap().text.as_deref(), Some("mine"));
    }

    #[test]
    fn copy_selection_answers_the_text_and_puts_the_clipboard_back() {
        let backend = FakeBackend::default();
        person_clipboard(&backend, "mine");
        backend.clip.lock().unwrap().copies = Some("selected words".into());
        let reply = one(&backend, "copy_selection", json!({}));
        assert_eq!(reply["content"][0]["text"], "selected words", "{reply}");
        assert_eq!(backend.clip.lock().unwrap().text.as_deref(), Some("mine"));
        // Nothing copied: an error, the clipboard untouched.
        backend.clip.lock().unwrap().copies = None;
        let mut display = None;
        let none = call(&backend, "copy_selection", &json!({}), &mut CallContext { interrupted: &never, display: &mut display, copy_mark: &mut None, picked: None, engage: &engaged });
        assert_eq!(none["isError"], true);
        assert_eq!(backend.clip.lock().unwrap().text.as_deref(), Some("mine"));
    }

    #[test]
    fn read_clipboard_reads_only_the_runs_own_copy() {
        let backend = FakeBackend::default();
        person_clipboard(&backend, "the person's secret");
        let mut display = None;
        let mut mark = None;
        let mut run = |name: &str, args: Value| {
            call(&backend, name, &args, &mut CallContext { interrupted: &never, display: &mut display, copy_mark: &mut mark, picked: None, engage: &engaged })
        };
        // No action of the run's yet: the person's clipboard stays theirs.
        let ambient = run("read_clipboard", json!({}));
        assert_eq!(ambient["isError"], true);
        assert!(!ambient.to_string().contains("secret"));
        // A click that copies nothing: still refused.
        run("left_click", json!({ "x": 1, "y": 1 }));
        let unchanged = run("read_clipboard", json!({}));
        assert_eq!(unchanged["isError"], true);
        assert!(!unchanged.to_string().contains("secret"));
        // The run's own copy shortcut: readable once.
        backend.clip.lock().unwrap().copies = Some("a token".into());
        run("key", json!({ "combo": "ctrl+c" }));
        assert_eq!(run("read_clipboard", json!({}))["content"][0]["text"], "a token");
        assert_eq!(run("read_clipboard", json!({}))["isError"], true);
        // A paste's own restore never arms it.
        run("paste_text", json!({ "text": "x" }));
        assert_eq!(run("read_clipboard", json!({}))["isError"], true);
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
