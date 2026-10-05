//! The macOS + Windows backend: xcap captures, enigo injects (CGEvent /
//! SendInput). What only the OS can answer (the window list with its layers,
//! focus, the idle clock, permissions, the accessibility tree) lives in
//! [`crate::macos`] / [`crate::windows`] as `os`.

use std::time::Duration;

use enigo::{Axis, Coordinate, Direction, Enigo, Keyboard, Mouse, Settings};

use crate::backend::*;
#[cfg(target_os = "macos")]
use crate::macos as os;
#[cfg(target_os = "windows")]
use crate::windows as os;

pub struct DesktopBackend;

impl DesktopBackend {
    pub fn new() -> Self {
        Self
    }
}

/// A fresh injector per action: enigo's handle is not `Send`, and it holds
/// nothing worth keeping between calls.
fn enigo() -> BackendResult<Enigo> {
    let settings = Settings {
        // Permission prompts are `readiness(true)`'s job, with our own text.
        open_prompt_to_get_permissions: false,
        ..Settings::default()
    };
    Enigo::new(&settings).map_err(|err| format!("Could not open the input injector: {err}"))
}

fn input<T>(result: Result<T, enigo::InputError>) -> BackendResult<T> {
    result.map_err(|err| format!("Input failed: {err}"))
}

fn capture_error(err: xcap::XCapError) -> String {
    format!("Screen capture failed: {err}")
}

/// Let the window server deliver one event before the next.
fn settle() {
    std::thread::sleep(Duration::from_millis(30));
}

fn enigo_key(key: Key) -> enigo::Key {
    match key {
        Key::Char(c) => enigo::Key::Unicode(c),
        Key::Enter => enigo::Key::Return,
        Key::Tab => enigo::Key::Tab,
        Key::Escape => enigo::Key::Escape,
        Key::Space => enigo::Key::Space,
        Key::Backspace => enigo::Key::Backspace,
        Key::Delete => enigo::Key::Delete,
        Key::Up => enigo::Key::UpArrow,
        Key::Down => enigo::Key::DownArrow,
        Key::Left => enigo::Key::LeftArrow,
        Key::Right => enigo::Key::RightArrow,
        Key::Home => enigo::Key::Home,
        Key::End => enigo::Key::End,
        Key::PageUp => enigo::Key::PageUp,
        Key::PageDown => enigo::Key::PageDown,
        Key::F(n) => match n {
            1 => enigo::Key::F1,
            2 => enigo::Key::F2,
            3 => enigo::Key::F3,
            4 => enigo::Key::F4,
            5 => enigo::Key::F5,
            6 => enigo::Key::F6,
            7 => enigo::Key::F7,
            8 => enigo::Key::F8,
            9 => enigo::Key::F9,
            10 => enigo::Key::F10,
            11 => enigo::Key::F11,
            _ => enigo::Key::F12,
        },
    }
}

fn enigo_modifier(modifier: Modifier) -> enigo::Key {
    match modifier {
        Modifier::Meta => enigo::Key::Meta,
        Modifier::Control => enigo::Key::Control,
        Modifier::Alt => enigo::Key::Alt,
        Modifier::Shift => enigo::Key::Shift,
    }
}

impl Backend for DesktopBackend {
    fn readiness(&self, prompt: bool) -> Readiness {
        os::readiness(prompt)
    }

    fn capture(&self, target: Target) -> BackendResult<Frame> {
        match target {
            Target::Display(index) => {
                let mut monitors = xcap::Monitor::all().map_err(capture_error)?;
                // Index 0 is the primary display, whatever order the OS lists.
                monitors.sort_by_key(|monitor| !monitor.is_primary().unwrap_or(false));
                let count = monitors.len();
                let monitor = monitors.into_iter().nth(index).ok_or_else(|| {
                    format!("There is no display {index}; this computer has {count}.")
                })?;
                let area = Rect {
                    x: f64::from(monitor.x().map_err(capture_error)?),
                    y: f64::from(monitor.y().map_err(capture_error)?),
                    width: f64::from(monitor.width().map_err(capture_error)?),
                    height: f64::from(monitor.height().map_err(capture_error)?),
                };
                let image = monitor.capture_image().map_err(capture_error)?;
                Ok(Frame { image, area })
            }
            Target::Window(id) => {
                let window = xcap::Window::all()
                    .map_err(capture_error)?
                    .into_iter()
                    .find(|window| window.id().ok() == Some(id))
                    .ok_or_else(|| format!("No window has id {id}; call list_windows."))?;
                let area = Rect {
                    x: f64::from(window.x().map_err(capture_error)?),
                    y: f64::from(window.y().map_err(capture_error)?),
                    width: f64::from(window.width().map_err(capture_error)?),
                    height: f64::from(window.height().map_err(capture_error)?),
                };
                let image = window.capture_image().map_err(capture_error)?;
                Ok(Frame { image, area })
            }
        }
    }

    fn windows(&self) -> BackendResult<Vec<WindowInfo>> {
        os::windows()
    }

    fn focused(&self) -> Option<WindowInfo> {
        os::focused()
    }

    fn focus_window(&self, id: u32) -> BackendResult<()> {
        os::focus_window(id)
    }

    fn pointer(&self) -> BackendResult<(f64, f64)> {
        let (x, y) = input(enigo()?.location())?;
        Ok((f64::from(x), f64::from(y)))
    }

    fn click(&self, x: f64, y: f64, button: Button, count: u8) -> BackendResult<()> {
        let mut enigo = enigo()?;
        input(enigo.move_mouse(x.round() as i32, y.round() as i32, Coordinate::Abs))?;
        settle();
        let button = match button {
            Button::Left => enigo::Button::Left,
            Button::Right => enigo::Button::Right,
            Button::Middle => enigo::Button::Middle,
        };
        for _ in 0..count {
            input(enigo.button(button, Direction::Click))?;
            settle();
        }
        Ok(())
    }

    fn scroll(&self, x: f64, y: f64, dx: i32, dy: i32) -> BackendResult<()> {
        let mut enigo = enigo()?;
        input(enigo.move_mouse(x.round() as i32, y.round() as i32, Coordinate::Abs))?;
        settle();
        if dy != 0 {
            input(enigo.scroll(dy, Axis::Vertical))?;
        }
        if dx != 0 {
            input(enigo.scroll(dx, Axis::Horizontal))?;
        }
        Ok(())
    }

    fn type_text(&self, text: &str) -> BackendResult<()> {
        input(enigo()?.text(text))
    }

    fn key(&self, chord: &Chord) -> BackendResult<()> {
        let mut enigo = enigo()?;
        #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
        let mut modifiers = chord.modifiers.clone();
        // macOS: a character key goes out by its keycode in the layout the
        // host read on the main thread. enigo would resolve it at press time
        // through Text Input Source calls that abort an app off that thread.
        #[cfg(target_os = "macos")]
        let character = match chord.key {
            Key::Char(c) => {
                let (keycode, shift) = os::key_spot(c).ok_or_else(|| {
                    format!("This keyboard layout has no key for `{c}`; use the type tool for it.")
                })?;
                if shift && !modifiers.contains(&Modifier::Shift) {
                    modifiers.push(Modifier::Shift);
                }
                Some(keycode)
            }
            _ => None,
        };
        for modifier in &modifiers {
            input(enigo.key(enigo_modifier(*modifier), Direction::Press))?;
        }
        #[cfg(target_os = "macos")]
        let pressed = match character {
            Some(keycode) => enigo.raw(keycode, Direction::Click),
            None => enigo.key(enigo_key(chord.key), Direction::Click),
        };
        #[cfg(not(target_os = "macos"))]
        let pressed = enigo.key(enigo_key(chord.key), Direction::Click);
        // Release what was pressed even when the key itself failed.
        for modifier in modifiers.iter().rev() {
            let _ = enigo.key(enigo_modifier(*modifier), Direction::Release);
        }
        input(pressed)
    }

    fn idle(&self) -> Option<Duration> {
        os::idle()
    }

    fn idle_counts_injected(&self) -> bool {
        os::IDLE_COUNTS_INJECTED
    }

    fn read_ui(&self, window: Option<u32>) -> Option<BackendResult<Vec<UiNode>>> {
        Some(os::read_ui(window))
    }

    #[cfg(target_os = "macos")]
    fn supports_background(&self) -> bool {
        true
    }

    #[cfg(target_os = "macos")]
    fn background_click(
        &self,
        window: &WindowInfo,
        x: f64,
        y: f64,
        button: Button,
        count: u8,
    ) -> BackendResult<()> {
        let previous = os::focused_pid()
            .and_then(|pid| os::key_window_of(pid).map(|window| (pid, window)));
        crate::macos_background::click(window.pid, window.id, x, y, button, count, previous, (window.rect.x, window.rect.y))
    }

    #[cfg(target_os = "macos")]
    fn background_scroll(&self, window: &WindowInfo, x: f64, y: f64, dx: i32, dy: i32) -> BackendResult<()> {
        crate::macos_background::scroll(window.pid, window.id, x, y, dx, dy, (window.rect.x, window.rect.y))
    }

    #[cfg(target_os = "macos")]
    fn background_type(&self, window: &WindowInfo, text: &str) -> BackendResult<()> {
        crate::macos_background::type_text(window.pid, text)
    }

    #[cfg(target_os = "macos")]
    fn background_key(&self, window: &WindowInfo, chord: &Chord) -> BackendResult<()> {
        let mut chord = chord.clone();
        let keycode = match chord.key {
            Key::Char(c) => {
                let (keycode, shift) = os::key_spot(c).ok_or_else(|| {
                    format!("This keyboard layout has no key for `{c}`; use the type tool for it.")
                })?;
                if shift && !chord.modifiers.contains(&Modifier::Shift) {
                    chord.modifiers.push(Modifier::Shift);
                }
                keycode
            }
            key => crate::macos_background::named_keycode(key).expect("a named key"),
        };
        if chord.modifiers.contains(&Modifier::Meta) {
            let previous = os::focused_pid()
                .and_then(|pid| os::key_window_of(pid).map(|window| (pid, window)));
            return crate::macos_background::shortcut(window.pid, window.id, &chord, keycode, previous);
        }
        crate::macos_background::key(window.pid, &chord, keycode)
    }
}
