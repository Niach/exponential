//! A scripted backend for the guard and server tests.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::backend::*;

pub struct FakeBackend {
    windows: Vec<WindowInfo>,
    log: Arc<Mutex<Vec<String>>>,
    idle: Mutex<Option<Duration>>,
    readiness: Mutex<Readiness>,
}

impl FakeBackend {
    pub fn with_windows(windows: Vec<WindowInfo>) -> Self {
        Self {
            windows,
            log: Arc::default(),
            idle: Mutex::new(Some(Duration::from_secs(60))),
            readiness: Mutex::new(Readiness::Ready),
        }
    }

    pub fn log(&self) -> Arc<Mutex<Vec<String>>> {
        self.log.clone()
    }

    pub fn set_idle(&self, idle: Option<Duration>) {
        *self.idle.lock().unwrap() = idle;
    }

    pub fn set_readiness(&self, readiness: Readiness) {
        *self.readiness.lock().unwrap() = readiness;
    }

    fn record(&self, line: String) -> BackendResult<()> {
        self.log.lock().unwrap().push(line);
        Ok(())
    }
}

impl Backend for FakeBackend {
    fn readiness(&self, _prompt: bool) -> Readiness {
        self.readiness.lock().unwrap().clone()
    }

    /// A 3136x2000 pixel frame over a 1568x1000 point display (2x).
    fn capture(&self, _target: Target) -> BackendResult<Frame> {
        Ok(Frame {
            image: image::RgbaImage::new(3136, 2000),
            area: Rect { x: 0.0, y: 0.0, width: 1568.0, height: 1000.0 },
        })
    }

    fn windows(&self) -> BackendResult<Vec<WindowInfo>> {
        Ok(self.windows.clone())
    }

    fn focus_window(&self, id: u32) -> BackendResult<()> {
        self.record(format!("focus {id}"))
    }

    fn pointer(&self) -> BackendResult<(f64, f64)> {
        Ok((0.0, 0.0))
    }

    fn click(&self, x: f64, y: f64, button: Button, count: u8) -> BackendResult<()> {
        self.record(format!("click {x},{y} {button:?} x{count}"))
    }

    fn scroll(&self, x: f64, y: f64, dx: i32, dy: i32) -> BackendResult<()> {
        self.record(format!("scroll {x},{y} {dx},{dy}"))
    }

    fn type_text(&self, text: &str) -> BackendResult<()> {
        self.record(format!("type {text}"))
    }

    fn key(&self, chord: &Chord) -> BackendResult<()> {
        self.record(format!("key {chord:?}"))
    }

    fn idle(&self) -> Option<Duration> {
        *self.idle.lock().unwrap()
    }

    fn background_click(&self, window: &WindowInfo, x: f64, y: f64, button: Button, count: u8) -> BackendResult<()> {
        self.record(format!("bg click {} {x},{y} {button:?} x{count}", window.id))
    }

    fn background_scroll(&self, window: &WindowInfo, x: f64, y: f64, dx: i32, dy: i32) -> BackendResult<()> {
        self.record(format!("bg scroll {} {x},{y} {dx},{dy}", window.id))
    }

    fn background_type(&self, window: &WindowInfo, text: &str) -> BackendResult<()> {
        self.record(format!("bg type {} {text}", window.id))
    }

    fn background_key(&self, window: &WindowInfo, chord: &Chord) -> BackendResult<()> {
        let modifiers: Vec<&str> = chord
            .modifiers
            .iter()
            .map(|modifier| match modifier {
                Modifier::Meta => "cmd",
                Modifier::Control => "ctrl",
                Modifier::Alt => "alt",
                Modifier::Shift => "shift",
            })
            .collect();
        let key = match chord.key {
            Key::Char(c) => c.to_string(),
            other => format!("{other:?}"),
        };
        let chord = modifiers.into_iter().chain([key.as_str()]).collect::<Vec<_>>().join("+");
        self.record(format!("bg key {} {chord}", window.id))
    }

    fn read_ui(&self, _window: Option<u32>) -> Option<BackendResult<Vec<UiNode>>> {
        Some(Ok(vec![UiNode {
            depth: 0,
            role: "window".into(),
            label: "Fake".into(),
            rect: None,
        }]))
    }
}
