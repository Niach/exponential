//! The layer between the tools and the OS backend, the same on every OS: the
//! pause while the person is at the keyboard, one action at a time across
//! runs, and the screenshot-space coordinate mapping. There is no app
//! blocklist: the device switch is the one gate, and an agent it lets in may
//! drive any app (EXP-1196, unrestricted by decision).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::backend::{
    Backend, Button, Chord, Delivery, Key, Modifier, Readiness, Rect, Target, UiNode, WindowInfo,
};

/// The long edge a frame is downscaled to before it reaches the model.
pub const SHOT_MAX_EDGE: u32 = 1568;
/// How long the person must have been idle before the agent acts.
pub const QUIET: Duration = Duration::from_millis(1500);
/// How long an action waits for that quiet before it gives up.
pub const QUIET_WAIT: Duration = Duration::from_secs(8);
/// Clock slack between the OS idle counter and our own injection stamp.
const SLACK: Duration = Duration::from_millis(100);
/// The most accessibility elements one `read_ui` returns.
pub const UI_NODES_MAX: usize = 400;

pub const PAUSED_MESSAGE: &str = "The person is using the keyboard or mouse right now, so \
nothing was done. Wait a few seconds and try again; do not work around it.";

// ---------------------------------------------------------------------------
// keys
// ---------------------------------------------------------------------------

/// Parse `cmd+shift+t`, `ctrl+c`, `enter`, `F5`.
pub fn parse_chord(text: &str) -> Result<Chord, String> {
    let mut modifiers = Vec::new();
    let mut key = None;
    let parts: Vec<&str> = if text == "+" {
        vec!["+"]
    } else {
        text.split('+').map(str::trim).collect()
    };
    for part in parts {
        let lower = part.to_lowercase();
        let modifier = match lower.as_str() {
            "cmd" | "command" | "meta" | "super" | "win" | "windows" => Some(Modifier::Meta),
            "ctrl" | "control" => Some(Modifier::Control),
            "alt" | "option" | "opt" => Some(Modifier::Alt),
            "shift" => Some(Modifier::Shift),
            _ => None,
        };
        if let Some(modifier) = modifier {
            if !modifiers.contains(&modifier) {
                modifiers.push(modifier);
            }
            continue;
        }
        if key.is_some() {
            return Err(format!("`{text}` names more than one key; send one chord per call."));
        }
        key = Some(match lower.as_str() {
            "enter" | "return" => Key::Enter,
            "tab" => Key::Tab,
            "esc" | "escape" => Key::Escape,
            "space" => Key::Space,
            "backspace" => Key::Backspace,
            "delete" | "del" => Key::Delete,
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "home" => Key::Home,
            "end" => Key::End,
            "pageup" | "page_up" => Key::PageUp,
            "pagedown" | "page_down" => Key::PageDown,
            other => {
                let function = other
                    .strip_prefix('f')
                    .and_then(|digits| digits.parse::<u8>().ok())
                    .filter(|n| (1..=12).contains(n));
                let mut chars = part.chars();
                match (function, chars.next(), chars.next()) {
                    (Some(n), _, _) => Key::F(n),
                    (None, Some(c), None) => Key::Char(c.to_lowercase().next().unwrap_or(c)),
                    _ => return Err(format!("`{part}` is not a key this tool knows.")),
                }
            }
        });
    }
    key.map(|key| Chord { modifiers, key })
        .ok_or_else(|| format!("`{text}` has no key, only modifiers."))
}

// ---------------------------------------------------------------------------
// coordinates
// ---------------------------------------------------------------------------

/// How the last screenshot a run took maps onto the screen: the tools take
/// pixel positions IN THAT IMAGE.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mapping {
    pub area: Rect,
    pub width: u32,
    pub height: u32,
    /// The window the screenshot showed, when it was one window: the
    /// default target of a background action.
    pub window: Option<u32>,
}

impl Mapping {
    pub fn to_screen(&self, x: f64, y: f64) -> Result<(f64, f64), String> {
        if x < 0.0 || y < 0.0 || x >= f64::from(self.width) || y >= f64::from(self.height) {
            return Err(format!(
                "({x}, {y}) is outside the last screenshot ({}x{}).",
                self.width, self.height
            ));
        }
        Ok((
            self.area.x + x * self.area.width / f64::from(self.width),
            self.area.y + y * self.area.height / f64::from(self.height),
        ))
    }

    /// A screen point as a pixel of the screenshot, when it shows there.
    pub fn to_shot(&self, x: f64, y: f64) -> Option<(i64, i64)> {
        self.area.contains(x, y).then(|| {
            (
                ((x - self.area.x) * f64::from(self.width) / self.area.width).round() as i64,
                ((y - self.area.y) * f64::from(self.height) / self.area.height).round() as i64,
            )
        })
    }
}

/// The size a `width`x`height` frame is delivered at: its long edge capped at
/// [`SHOT_MAX_EDGE`], never enlarged.
pub fn shot_size(width: u32, height: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= SHOT_MAX_EDGE || long == 0 {
        return (width, height);
    }
    let scale = f64::from(SHOT_MAX_EDGE) / f64::from(long);
    (
        ((f64::from(width) * scale).round() as u32).max(1),
        ((f64::from(height) * scale).round() as u32).max(1),
    )
}

// ---------------------------------------------------------------------------
// the person
// ---------------------------------------------------------------------------

/// Whether the last input came from the person: recent and, where the OS
/// idle counter also sees our own events (`counts_injected`), later than
/// anything we injected ourselves.
pub fn person_active(
    idle: Option<Duration>,
    since_injection: Option<Duration>,
    counts_injected: bool,
) -> bool {
    match idle {
        None => false,
        Some(idle) if !counts_injected => idle < QUIET,
        Some(idle) => idle < QUIET && since_injection.is_none_or(|since| idle + SLACK < since),
    }
}

// ---------------------------------------------------------------------------
// guard
// ---------------------------------------------------------------------------

/// What a tool hands back: text, plus a PNG for `screenshot`.
#[derive(Debug, Default)]
pub struct ToolOutput {
    pub text: String,
    pub png: Option<Vec<u8>>,
}

impl ToolOutput {
    fn text(text: impl Into<String>) -> Self {
        Self { text: text.into(), png: None }
    }
}

pub struct Guard {
    backend: Box<dyn Backend>,
    /// One pointer and one keyboard: every action of every run goes through
    /// this, in turn.
    turn: Mutex<()>,
    last_injection: Mutex<Option<Instant>>,
    quiet_wait: Duration,
}

impl Guard {
    pub fn new(backend: Box<dyn Backend>) -> Self {
        Self {
            backend,
            turn: Mutex::new(()),
            last_injection: Mutex::new(None),
            quiet_wait: QUIET_WAIT,
        }
    }

    #[cfg(test)]
    pub fn with_quiet_wait(mut self, wait: Duration) -> Self {
        self.quiet_wait = wait;
        self
    }

    pub fn readiness(&self, prompt: bool) -> Readiness {
        self.backend.readiness(prompt)
    }

    /// See [`Backend::prepare`]; under the turn lock, so no action races it.
    pub fn prepare(&self) -> Readiness {
        let _turn = self.turn.lock().unwrap();
        self.backend.prepare()
    }

    fn ready(&self) -> Result<(), String> {
        match self.backend.readiness(true) {
            Readiness::Ready => Ok(()),
            Readiness::MissingPermission(text) | Readiness::Unsupported(text) => Err(text),
        }
    }

    fn since_injection(&self) -> Option<Duration> {
        self.last_injection.lock().unwrap().map(|at| at.elapsed())
    }

    /// Block until the person has been idle for [`QUIET`], or refuse.
    fn wait_for_quiet(&self) -> Result<(), String> {
        let deadline = Instant::now() + self.quiet_wait;
        let counts_injected = self.backend.idle_counts_injected();
        while person_active(self.backend.idle(), self.since_injection(), counts_injected) {
            if Instant::now() >= deadline {
                return Err(PAUSED_MESSAGE.to_string());
            }
            std::thread::sleep(Duration::from_millis(150));
        }
        Ok(())
    }

    /// Run one input action: readiness, the person's quiet, then `act`, all
    /// under the turn lock; stamps the injection so our own events never
    /// read as the person's.
    fn act(&self, act: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        let _turn = self.turn.lock().unwrap();
        self.ready()?;
        self.wait_for_quiet()?;
        let result = act();
        *self.last_injection.lock().unwrap() = Some(Instant::now());
        result
    }

    /// Run one BACKGROUND action: readiness and the turn lock, but no wait
    /// for the person's quiet. The events go to one process, not the queue
    /// the person types into, so the two do not collide.
    fn act_background(&self, act: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        let _turn = self.turn.lock().unwrap();
        self.ready()?;
        act()
    }

    /// The window a background action goes to: the one named, else the one
    /// the last screenshot showed.
    fn background_window(
        &self,
        window: Option<u32>,
        mapping: Option<Mapping>,
    ) -> Result<WindowInfo, String> {
        let id = window.or(mapping.and_then(|mapping| mapping.window)).ok_or(NO_TARGET)?;
        self.backend
            .windows()?
            .into_iter()
            .find(|info| info.id == id)
            .ok_or_else(|| format!("No window has id {id}; call list_windows."))
    }

    /// The window a click at this screen point lands in (the frontmost
    /// ordinary one), only to name its app in the answer.
    fn window_at(&self, x: f64, y: f64) -> Option<WindowInfo> {
        self.backend
            .windows()
            .ok()?
            .into_iter()
            .find(|window| window.layer == 0 && window.rect.contains(x, y))
    }

    pub fn screenshot(
        &self,
        target: Target,
        save_to: Option<&std::path::Path>,
    ) -> Result<(ToolOutput, Mapping), String> {
        self.ready()?;
        let frame = self.backend.capture(target)?;
        let (width, height) = shot_size(frame.image.width(), frame.image.height());
        let image = if (width, height) == (frame.image.width(), frame.image.height()) {
            frame.image
        } else {
            image::imageops::resize(&frame.image, width, height, image::imageops::FilterType::Triangle)
        };
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|err| format!("Could not encode the screenshot: {err}"))?;
        let mut text = format!(
            "Screenshot {width}x{height}. click and scroll take pixel positions in this image."
        );
        if let Some(path) = save_to {
            match std::fs::write(path, &png) {
                Ok(()) => text.push_str(&format!(" Saved to {}.", path.display())),
                Err(err) => text.push_str(&format!(" Could not save to {}: {err}.", path.display())),
            }
        }
        let window = match target {
            Target::Window(id) => Some(id),
            Target::Display(_) => None,
        };
        let mapping = Mapping { area: frame.area, width, height, window };
        Ok((ToolOutput { text, png: Some(png) }, mapping))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn click(
        &self,
        mapping: Option<Mapping>,
        x: f64,
        y: f64,
        button: Button,
        count: u8,
        delivery: Delivery,
        window: Option<u32>,
    ) -> Result<ToolOutput, String> {
        let (sx, sy) = mapping.ok_or(NO_SHOT)?.to_screen(x, y)?;
        let count = count.clamp(1, 3);
        if delivery == Delivery::Background {
            let target = self.background_window(window, mapping)?;
            self.act_background(|| self.backend.background_click(&target, sx, sy, button, count))?;
            return Ok(ToolOutput::text(format!(
                "Posted a click at ({x}, {y}) to {} in the background.",
                name(&target)
            )));
        }
        let target = self.window_at(sx, sy);
        self.act(|| self.backend.click(sx, sy, button, count))?;
        Ok(ToolOutput::text(format!("Clicked ({x}, {y}){}.", in_app(target.as_ref()))))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn scroll(
        &self,
        mapping: Option<Mapping>,
        x: f64,
        y: f64,
        dx: i32,
        dy: i32,
        delivery: Delivery,
        window: Option<u32>,
    ) -> Result<ToolOutput, String> {
        let (sx, sy) = mapping.ok_or(NO_SHOT)?.to_screen(x, y)?;
        let (dx, dy) = (dx.clamp(-50, 50), dy.clamp(-50, 50));
        if delivery == Delivery::Background {
            let target = self.background_window(window, mapping)?;
            self.act_background(|| self.backend.background_scroll(&target, sx, sy, dx, dy))?;
            return Ok(ToolOutput::text(format!(
                "Posted a scroll at ({x}, {y}) to {} in the background.",
                name(&target)
            )));
        }
        let target = self.window_at(sx, sy);
        self.act(|| self.backend.scroll(sx, sy, dx, dy))?;
        Ok(ToolOutput::text(format!("Scrolled at ({x}, {y}){}.", in_app(target.as_ref()))))
    }

    pub fn type_text(
        &self,
        text: &str,
        delivery: Delivery,
        window: Option<u32>,
        mapping: Option<Mapping>,
    ) -> Result<ToolOutput, String> {
        if delivery == Delivery::Background {
            let target = self.background_window(window, mapping)?;
            self.act_background(|| self.backend.background_type(&target, text))?;
            return Ok(ToolOutput::text(format!(
                "Posted {} characters to {} in the background.",
                text.chars().count(),
                name(&target)
            )));
        }
        let target = self.backend.focused();
        self.act(|| self.backend.type_text(text))?;
        Ok(ToolOutput::text(format!(
            "Typed {} characters{}.",
            text.chars().count(),
            in_app(target.as_ref())
        )))
    }

    pub fn key(
        &self,
        keys: &str,
        delivery: Delivery,
        window: Option<u32>,
        mapping: Option<Mapping>,
    ) -> Result<ToolOutput, String> {
        let chord = parse_chord(keys)?;
        if delivery == Delivery::Background {
            let target = self.background_window(window, mapping)?;
            self.act_background(|| self.backend.background_key(&target, &chord))?;
            return Ok(ToolOutput::text(format!(
                "Posted {keys} to {} in the background.",
                name(&target)
            )));
        }
        let target = self.backend.focused();
        self.act(|| self.backend.key(&chord))?;
        Ok(ToolOutput::text(format!("Pressed {keys}{}.", in_app(target.as_ref()))))
    }

    pub fn list_windows(&self) -> Result<ToolOutput, String> {
        self.ready()?;
        let windows: Vec<WindowInfo> =
            self.backend.windows()?.into_iter().filter(|window| window.layer == 0).collect();
        if windows.is_empty() {
            return Ok(ToolOutput::text("No windows are open."));
        }
        let lines: Vec<String> = windows
            .iter()
            .map(|window| {
                let mut line = format!(
                    "{}  {} | {} | {}x{}",
                    window.id,
                    window.app,
                    window.title,
                    window.rect.width.round(),
                    window.rect.height.round()
                );
                if window.focused {
                    line.push_str(" | focused");
                }
                line
            })
            .collect();
        Ok(ToolOutput::text(format!(
            "id  app | title | size (front to back)\n{}",
            lines.join("\n")
        )))
    }

    pub fn focus_window(&self, id: u32) -> Result<ToolOutput, String> {
        let window = self
            .backend
            .windows()?
            .into_iter()
            .find(|window| window.id == id)
            .ok_or_else(|| format!("No window has id {id}; call list_windows."))?;
        self.act(|| self.backend.focus_window(id))?;
        Ok(ToolOutput::text(format!("Focused {} ({}).", window.app, window.title)))
    }

    pub fn read_ui(&self, mapping: Option<Mapping>, window: Option<u32>) -> Result<ToolOutput, String> {
        self.ready()?;
        let nodes = self
            .backend
            .read_ui(window)
            .ok_or("This operating system has no accessibility reader yet; use screenshot.")??;
        Ok(ToolOutput::text(render_ui(&nodes, mapping)))
    }
}

const NO_SHOT: &str = "Take a screenshot first: positions are pixels in the last screenshot.";
const NO_TARGET: &str = "Background delivery needs a target window: pass `window` (an id from \
list_windows) or take a screenshot of that window first.";

fn name(window: &WindowInfo) -> String {
    if window.title.is_empty() {
        window.app.clone()
    } else {
        format!("{} ({})", window.app, window.title)
    }
}

fn in_app(window: Option<&WindowInfo>) -> String {
    window
        .filter(|window| !window.app.is_empty())
        .map(|window| format!(" in {}", window.app))
        .unwrap_or_default()
}

/// The accessibility tree as indented lines; an element the last screenshot
/// shows carries its center in that image's pixels.
pub fn render_ui(nodes: &[UiNode], mapping: Option<Mapping>) -> String {
    if nodes.is_empty() {
        return "The window exposes no accessibility elements; use screenshot.".to_string();
    }
    let mut lines: Vec<String> = nodes
        .iter()
        .take(UI_NODES_MAX)
        .map(|node| {
            let mut line = format!("{}{}", "  ".repeat(node.depth.min(12)), node.role);
            if !node.label.is_empty() {
                let label: String = node.label.chars().take(120).collect();
                line.push_str(&format!(" \"{}\"", label.replace('\n', " ")));
            }
            let center = node.rect.zip(mapping).and_then(|(rect, mapping)| {
                mapping.to_shot(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
            });
            if let Some((x, y)) = center {
                line.push_str(&format!(" @ ({x}, {y})"));
            }
            line
        })
        .collect();
    if nodes.len() > UI_NODES_MAX {
        lines.push(format!("… {} more elements", nodes.len() - UI_NODES_MAX));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeBackend;

    fn window(app: &str, exe: &str, pid: u32) -> WindowInfo {
        WindowInfo {
            id: 1,
            pid,
            app: app.into(),
            exe: exe.into(),
            title: String::new(),
            rect: Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
            focused: true,
            layer: 0,
        }
    }

    #[test]
    fn chords_parse_modifiers_named_keys_and_characters() {
        assert_eq!(
            parse_chord("cmd+shift+T").unwrap(),
            Chord { modifiers: vec![Modifier::Meta, Modifier::Shift], key: Key::Char('t') }
        );
        assert_eq!(parse_chord("Enter").unwrap(), Chord { modifiers: vec![], key: Key::Enter });
        assert_eq!(parse_chord("ctrl+F5").unwrap().key, Key::F(5));
        assert_eq!(parse_chord("f").unwrap().key, Key::Char('f'));
        assert_eq!(parse_chord("+").unwrap().key, Key::Char('+'));
        assert!(parse_chord("cmd+shift").is_err());
        assert!(parse_chord("a+b").is_err());
        assert!(parse_chord("hello").is_err());
    }

    #[test]
    fn a_frame_is_capped_on_its_long_edge_and_never_enlarged() {
        assert_eq!(shot_size(800, 600), (800, 600));
        assert_eq!(shot_size(3136, 2000), (1568, 1000));
        assert_eq!(shot_size(1000, 3136), (500, 1568));
    }

    #[test]
    fn screenshot_pixels_map_onto_the_captured_area_and_back() {
        let mapping = Mapping {
            area: Rect { x: 100.0, y: 50.0, width: 2000.0, height: 1000.0 },
            width: 1000,
            height: 500,
            window: None,
        };
        assert_eq!(mapping.to_screen(0.0, 0.0).unwrap(), (100.0, 50.0));
        assert_eq!(mapping.to_screen(500.0, 250.0).unwrap(), (1100.0, 550.0));
        assert!(mapping.to_screen(1000.0, 10.0).is_err());
        assert!(mapping.to_screen(-1.0, 10.0).is_err());
        assert_eq!(mapping.to_shot(1100.0, 550.0), Some((500, 250)));
        assert_eq!(mapping.to_shot(10.0, 10.0), None);
    }

    #[test]
    fn only_input_after_our_own_counts_as_the_person() {
        let ms = Duration::from_millis;
        // Nobody touched anything for a while.
        assert!(!person_active(Some(ms(5000)), None, true));
        // Fresh input and we never injected: the person.
        assert!(person_active(Some(ms(200)), None, true));
        // Fresh input, but it is our own click from 200 ms ago.
        assert!(!person_active(Some(ms(200)), Some(ms(200)), true));
        // We clicked a second ago and something newer arrived: the person.
        assert!(person_active(Some(ms(100)), Some(ms(1000)), true));
        // A hardware-only counter never sees our click: fresh = the person.
        assert!(person_active(Some(ms(200)), Some(ms(200)), false));
        assert!(!person_active(Some(ms(5000)), Some(ms(200)), false));
        // No idle counter on this OS: never pause.
        assert!(!person_active(None, Some(ms(10)), true));
    }

    #[test]
    fn a_click_needs_a_screenshot_and_lands_in_screen_coordinates() {
        let backend = FakeBackend::with_windows(vec![window("TextEdit", "TextEdit", 7)]);
        let log = backend.log();
        let guard = Guard::new(Box::new(backend));
        assert!(guard.click(None, 1.0, 1.0, Button::Left, 1, Delivery::Foreground, None).unwrap_err().contains("screenshot"));
        let (output, mapping) = guard.screenshot(Target::Display(0), None).unwrap();
        assert!(output.png.is_some());
        // The fake display is 3136x2000 at 2x: a 1568x1000 image over 1568x1000 points.
        assert_eq!((mapping.width, mapping.height), (1568, 1000));
        let done = guard.click(Some(mapping), 50.0, 50.0, Button::Left, 2, Delivery::Foreground, None).unwrap();
        assert_eq!(done.text, "Clicked (50, 50) in TextEdit.");
        assert_eq!(log.lock().unwrap().as_slice(), ["click 50,50 Left x2"]);
    }

    #[test]
    fn every_app_takes_input_even_a_terminal() {
        // Unrestricted by decision (EXP-1196): no app blocklist, the device
        // switch is the one gate.
        let backend = FakeBackend::with_windows(vec![window("Terminal", "Terminal", 7)]);
        let log = backend.log();
        let guard = Guard::new(Box::new(backend));
        let (_, mapping) = guard.screenshot(Target::Display(0), None).unwrap();
        assert_eq!(
            guard.click(Some(mapping), 50.0, 50.0, Button::Left, 1, Delivery::Foreground, None).unwrap().text,
            "Clicked (50, 50) in Terminal."
        );
        guard.scroll(Some(mapping), 50.0, 50.0, 0, 3, Delivery::Foreground, None).unwrap();
        assert_eq!(guard.type_text("ls", Delivery::Foreground, None, None).unwrap().text, "Typed 2 characters in Terminal.");
        guard.key("enter", Delivery::Foreground, None, None).unwrap();
        guard.focus_window(1).unwrap();
        assert_eq!(log.lock().unwrap().len(), 5);
        assert!(!guard.list_windows().unwrap().text.contains("off limits"));
    }

    #[test]
    fn typing_with_no_known_focused_app_still_types() {
        let backend = FakeBackend::with_windows(vec![]);
        let log = backend.log();
        Guard::new(Box::new(backend)).type_text("ls", Delivery::Foreground, None, None).unwrap();
        assert_eq!(*log.lock().unwrap(), vec!["type ls".to_string()]);
    }

    #[test]
    fn a_click_names_the_frontmost_ordinary_window_under_it() {
        // The Dock's screen-sized overlay sits above everything; the answer
        // names the app window under the pointer, not the overlay.
        let mut dock = window("Dock", "Dock", 3);
        dock.layer = 20;
        let backend = FakeBackend::with_windows(vec![dock, window("TextEdit", "TextEdit", 7)]);
        let guard = Guard::new(Box::new(backend));
        let (_, mapping) = guard.screenshot(Target::Display(0), None).unwrap();
        assert_eq!(
            guard.click(Some(mapping), 5.0, 5.0, Button::Left, 1, Delivery::Foreground, None).unwrap().text,
            "Clicked (5, 5) in TextEdit."
        );
        assert!(!guard.list_windows().unwrap().text.contains("Dock"));
    }

    #[test]
    fn background_actions_go_to_the_shot_window_without_waiting_for_quiet() {
        let backend = FakeBackend::with_windows(vec![window("TextEdit", "TextEdit", 7)]);
        // The person is typing right now: background input does not collide.
        backend.set_idle(Some(Duration::from_millis(10)));
        let log = backend.log();
        let guard = Guard::new(Box::new(backend)).with_quiet_wait(Duration::from_millis(50));
        let bg = Delivery::Background;
        // No target: neither a window id nor a window screenshot.
        assert!(guard.type_text("hi", bg, None, None).unwrap_err().contains("target window"));
        let (_, shot) = guard.screenshot(Target::Window(1), None).unwrap();
        assert_eq!(shot.window, Some(1));
        assert_eq!(
            guard.click(Some(shot), 50.0, 50.0, Button::Left, 1, bg, None).unwrap().text,
            "Posted a click at (50, 50) to TextEdit in the background."
        );
        guard.type_text("hi", bg, None, Some(shot)).unwrap();
        guard.key("cmd+s", bg, Some(1), None).unwrap();
        assert!(guard.type_text("x", bg, Some(99), None).unwrap_err().contains("No window has id 99"));
        assert_eq!(
            log.lock().unwrap().as_slice(),
            ["bg click 1 50,50 Left x1", "bg type 1 hi", "bg key 1 cmd+s"]
        );
        // The foreground path still waits for the person.
        assert_eq!(guard.type_text("hi", Delivery::Foreground, None, None).unwrap_err(), PAUSED_MESSAGE);
    }

    #[test]
    fn the_agent_waits_while_the_person_types_and_gives_up_politely() {
        let backend = FakeBackend::with_windows(vec![window("TextEdit", "TextEdit", 7)]);
        backend.set_idle(Some(Duration::from_millis(10)));
        let log = backend.log();
        let guard = Guard::new(Box::new(backend)).with_quiet_wait(Duration::from_millis(50));
        assert_eq!(guard.type_text("hi", Delivery::Foreground, None, None).unwrap_err(), PAUSED_MESSAGE);
        assert!(log.lock().unwrap().is_empty());
    }

    #[test]
    fn a_missing_permission_is_what_the_tool_says() {
        let backend = FakeBackend::with_windows(vec![]);
        backend.set_readiness(Readiness::MissingPermission("Grant Screen Recording.".into()));
        let guard = Guard::new(Box::new(backend));
        assert_eq!(guard.screenshot(Target::Display(0), None).err().unwrap(), "Grant Screen Recording.");
        assert_eq!(guard.type_text("hi", Delivery::Foreground, None, None).unwrap_err(), "Grant Screen Recording.");
    }

    #[test]
    fn the_ui_tree_renders_indented_with_screenshot_positions() {
        let mapping = Mapping {
            area: Rect { x: 0.0, y: 0.0, width: 200.0, height: 100.0 },
            width: 100,
            height: 50,
            window: None,
        };
        let nodes = vec![
            UiNode { depth: 0, role: "window".into(), label: "Untitled".into(), rect: None },
            UiNode {
                depth: 1,
                role: "button".into(),
                label: "Save".into(),
                rect: Some(Rect { x: 20.0, y: 20.0, width: 40.0, height: 20.0 }),
            },
        ];
        assert_eq!(render_ui(&nodes, Some(mapping)), "window \"Untitled\"\n  button \"Save\" @ (20, 15)");
        assert_eq!(render_ui(&nodes, None), "window \"Untitled\"\n  button \"Save\"");
    }
}
