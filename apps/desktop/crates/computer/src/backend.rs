//! The per-OS seam: everything [`crate::guard`] and the tools need from a
//! desktop, in GLOBAL screen coordinates (the unit the OS takes pointer
//! positions in). Screenshot-space mapping and the input pause
//! live above this trait, OS-independent, so a fake backend tests them.

use std::time::Duration;

/// A rectangle in global screen coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

/// One top-level window, front to back in [`Backend::windows`].
#[derive(Clone, Debug, PartialEq)]
pub struct WindowInfo {
    pub id: u32,
    pub pid: u32,
    /// The owning app's display name (`Safari`, `firefox`).
    pub app: String,
    /// The owning process's executable file name, when the OS tells
    /// (`Safari`, `firefox.exe`), for naming the app.
    pub exe: String,
    pub title: String,
    pub rect: Rect,
    pub focused: bool,
    /// 0 = an ordinary app window. macOS also reports system surfaces above
    /// it (the menu bar, the Dock's screen-sized overlay, auth sheets); the
    /// guard lists and hit-tests ordinary windows only.
    pub layer: i32,
}

/// What a capture covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A display by index (0 = primary).
    Display(usize),
    Window(u32),
}

/// A captured frame: RGBA pixels plus the screen area they show.
pub struct Frame {
    pub image: image::RgbaImage,
    pub area: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    /// Command on macOS, Super/Win elsewhere.
    Meta,
    Control,
    Alt,
    Shift,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    Escape,
    Space,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}

/// A key press with its held modifiers (`cmd+shift+t`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub modifiers: Vec<Modifier>,
    pub key: Key,
}

/// How an input action reaches its app (EXP-1196).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Delivery {
    /// The default: [`Self::Background`] where the backend implements it
    /// and a target window is known, else [`Self::Foreground`].
    #[default]
    Auto,
    /// The global input queue, like the person's own hands: the pointer
    /// moves, keys go to the focused window. Every OS.
    Foreground,
    /// Posted to ONE window's process: the pointer stays put and the app in
    /// front stays in front, so the person keeps working. macOS only.
    Background,
}

impl Delivery {
    pub fn parse(text: Option<&str>) -> Result<Self, String> {
        match text {
            None => Ok(Self::Auto),
            Some("foreground") => Ok(Self::Foreground),
            Some("background") => Ok(Self::Background),
            Some(other) => Err(format!("`{other}` is not a delivery (foreground, background).")),
        }
    }
}

pub const NO_BACKGROUND: &str =
    "Background delivery is not available on this operating system; use foreground.";

/// Why the backend cannot act right now, phrased for the agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Readiness {
    Ready,
    /// The OS withholds a permission; the text says which and where to
    /// grant it.
    MissingPermission(String),
    /// This session type has no backend (a headless box).
    Unsupported(String),
}

pub type BackendResult<T> = Result<T, String>;

pub trait Backend: Send + Sync {
    /// Whether capture and input can work; `prompt` also asks the OS to show
    /// its permission dialogs.
    fn readiness(&self, prompt: bool) -> Readiness;
    /// Ask for EVERY permission now, while the person is at the switch,
    /// so no dialog interrupts a run later. Blocks while a dialog waits.
    fn prepare(&self) -> Readiness {
        self.readiness(true)
    }
    fn capture(&self, target: Target) -> BackendResult<Frame>;
    /// Top-level windows, front to back.
    fn windows(&self) -> BackendResult<Vec<WindowInfo>>;
    fn focus_window(&self, id: u32) -> BackendResult<()>;
    fn pointer(&self) -> BackendResult<(f64, f64)>;
    fn click(&self, x: f64, y: f64, button: Button, count: u8) -> BackendResult<()>;
    /// Scroll at a point by wheel notches (positive `dy` = down, `dx` = right).
    fn scroll(&self, x: f64, y: f64, dx: i32, dy: i32) -> BackendResult<()>;
    fn type_text(&self, text: &str) -> BackendResult<()>;
    fn key(&self, chord: &Chord) -> BackendResult<()>;
    /// The app keyboard input goes to. Default: the focused window; an OS
    /// that can name the focused APP even when it shows no ordinary window
    /// (a system prompt) overrides this.
    fn focused(&self) -> Option<WindowInfo> {
        self.windows().ok()?.into_iter().find(|window| window.focused)
    }
    /// Time since the last input event.
    fn idle(&self) -> Option<Duration>;
    /// Whether [`Self::idle`] also resets on the events WE inject (the guard
    /// then discounts its own injections), or counts hardware input only.
    fn idle_counts_injected(&self) -> bool {
        true
    }
    /// The accessibility tree of a window (the focused one when `None`), as
    /// indented text lines with element centers in global coordinates.
    /// `None` = this OS has no reader.
    fn read_ui(&self, window: Option<u32>) -> Option<BackendResult<Vec<UiNode>>>;

    // Background delivery: into `window` only, pointer and front app
    // untouched. Defaults refuse; macOS overrides.
    /// Whether the `background_*` methods are implemented here (what
    /// [`Delivery::Auto`] keys on).
    fn supports_background(&self) -> bool {
        false
    }
    fn background_click(
        &self,
        _window: &WindowInfo,
        _x: f64,
        _y: f64,
        _button: Button,
        _count: u8,
    ) -> BackendResult<()> {
        Err(NO_BACKGROUND.to_string())
    }
    fn background_scroll(&self, _window: &WindowInfo, _x: f64, _y: f64, _dx: i32, _dy: i32) -> BackendResult<()> {
        Err(NO_BACKGROUND.to_string())
    }
    fn background_type(&self, _window: &WindowInfo, _text: &str) -> BackendResult<()> {
        Err(NO_BACKGROUND.to_string())
    }
    fn background_key(&self, _window: &WindowInfo, _chord: &Chord) -> BackendResult<()> {
        Err(NO_BACKGROUND.to_string())
    }
}

/// One accessibility element.
#[derive(Clone, Debug, PartialEq)]
pub struct UiNode {
    pub depth: usize,
    pub role: String,
    pub label: String,
    pub rect: Option<Rect>,
}

/// The backend for this OS, or why there is none.
pub fn platform() -> Result<Box<dyn Backend>, String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        Ok(Box::new(crate::desktop::DesktopBackend::new()))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if crate::wayland::is_wayland_session() {
            return crate::wayland::WaylandBackend::connect().map(|backend| Box::new(backend) as Box<dyn Backend>);
        }
        crate::linux::X11Backend::connect().map(|backend| Box::new(backend) as Box<dyn Backend>)
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        Err("Computer use is not available on this operating system.".to_string())
    }
}
