//! Linux on eDE (#NEW-112): the compositor's own control plane,
//! `ede-protocol`, instead of XTest (which Wayland does not have).
//!
//! Input is `input` on the person's seat (`input:person`): pointer motion,
//! evdev buttons, wheel notches, and keys as keysyms the compositor looks up
//! on its own layout (Shift/AltGr included). The screen is `capture` of an
//! output (protected windows black), windows are `list_windows` and
//! `focus_window`. The compositor refuses input on protected surfaces and
//! while the session is locked; those refusals are the tool's error.
//!
//! The lock is `hold_input` on a connection of its own: the person's keys,
//! pointer and wheel reach no window while it lives; their Esc ends it in
//! the compositor and comes back as `input.escape`, which is [`LockGuard`]'s
//! `on_esc`. Dropping the guard closes the connection, which releases the
//! hold (a crashed host never leaves the person locked out).
//!
//! eDE = `XDG_CURRENT_DESKTOP` names `eDE` and its socket
//! (`$XDG_RUNTIME_DIR/ede/$WAYLAND_DISPLAY.sock`) is there. The grant is the
//! shell's token file, or `EDE_TOKEN` (a narrower minted grant).

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use ede_protocol::{CaptureTarget, Client, InputEvent, Request, Token, WindowId};

use crate::backend::{finish_ids, Backend, Button, DisplayInfo, Image, LockGuard, WindowInfo};
use crate::keys::Key;

/// Where an eDE session's control plane lives, read from the environment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Session {
    pub socket: PathBuf,
    pub token_file: PathBuf,
    /// `EDE_TOKEN`, when set (wins over the file).
    pub token: Option<String>,
}

/// `Some` = this is an eDE session (`XDG_CURRENT_DESKTOP` names `eDE`): its
/// paths, or why it cannot be driven.
pub(crate) fn session() -> Option<Result<Session, String>> {
    session_of(|name| std::env::var(name).ok())
}

fn session_of(env: impl Fn(&str) -> Option<String>) -> Option<Result<Session, String>> {
    if !env("XDG_CURRENT_DESKTOP")?.split(':').any(|desktop| desktop == "eDE") {
        return None;
    }
    let (Some(runtime), Some(display)) = (env("XDG_RUNTIME_DIR"), env("WAYLAND_DISPLAY")) else {
        return Some(Err("This eDE session has no XDG_RUNTIME_DIR or WAYLAND_DISPLAY.".to_string()));
    };
    let runtime = Path::new(&runtime);
    Some(Ok(Session {
        socket: ede_protocol::socket_path(runtime, &display),
        token_file: ede_protocol::token_path(runtime, &display),
        token: env("EDE_TOKEN").filter(|token| !token.trim().is_empty()),
    }))
}

/// Why this eDE session cannot be driven (the doctor line), without
/// connecting.
pub(crate) fn unsupported(session: &Result<Session, String>) -> Option<String> {
    match session {
        Err(reason) => Some(reason.clone()),
        Ok(session) if !session.socket.exists() => {
            Some(format!("eDE's control socket is missing ({}); log in to the eDE session again.", session.socket.display()))
        }
        Ok(session) if session.token.is_none() && !session.token_file.exists() => {
            Some(format!("eDE's token file is missing ({}).", session.token_file.display()))
        }
        Ok(_) => None,
    }
}

impl Session {
    fn connect(&self) -> Result<Client, String> {
        let token = match &self.token {
            Some(token) => token.clone(),
            None => std::fs::read_to_string(&self.token_file).map_err(|err| format!("eDE token {}: {err}", self.token_file.display()))?,
        };
        let token = Token::new(token.trim()).map_err(|err| format!("eDE token: {err}"))?;
        Client::connect(&self.socket, token).map_err(|err| format!("eDE ({}): {err}", self.socket.display()))
    }
}

pub(crate) fn open(session: &Session) -> Result<Box<dyn Backend>, String> {
    Ok(Box::new(Ede { client: Mutex::new(session.connect()?), session: session.clone() }))
}

pub(crate) struct Ede {
    client: Mutex<Client>,
    session: Session,
}

fn e(what: &str) -> impl Fn(ede_protocol::Error) -> String + '_ {
    move |err| format!("{what}: {err}")
}

/// evdev `BTN_LEFT` / `BTN_RIGHT` / `BTN_MIDDLE`.
fn evdev_button(button: Button) -> u32 {
    match button {
        Button::Left => 0x110,
        Button::Right => 0x111,
        Button::Middle => 0x112,
    }
}

/// Our window ids are the compositor's `w<n>`.
fn window_number(id: &WindowId) -> Option<u64> {
    id.as_str().strip_prefix('w')?.parse().ok()
}

/// `argb8888` (little-endian `0xAARRGGBB` words, `stride` bytes a row) → RGBA8.
fn rgba_of(capture: &ede_protocol::Capture) -> Result<Image, String> {
    let info = &capture.info;
    if info.format != "argb8888" {
        return Err(format!("Unsupported eDE capture format {:?}.", info.format));
    }
    let (width, height, stride) = (info.width.max(0) as usize, info.height.max(0) as usize, info.stride.max(0) as usize);
    if stride < width * 4 || capture.pixels.len() < stride * height {
        return Err("The eDE capture is short.".to_string());
    }
    let mut rgba = Vec::with_capacity(width * height * 4);
    for row in capture.pixels.chunks_exact(stride).take(height) {
        for pixel in row[..width * 4].chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
    Ok(Image { width: width as u32, height: height as u32, rgba })
}

impl Ede {
    fn input(&self, event: InputEvent) -> Result<(), String> {
        self.client.lock().unwrap().input(event).map_err(e("eDE input"))
    }

    fn keysym(&self, keysym: u32, pressed: bool) -> Result<(), String> {
        self.input(InputEvent::Keysym { keysym, pressed })
    }
}

impl Backend for Ede {
    fn displays(&self) -> Result<Vec<DisplayInfo>, String> {
        let outputs = self.client.lock().unwrap().outputs().map_err(e("eDE outputs"))?;
        if outputs.is_empty() {
            return Err("eDE has no output.".to_string());
        }
        let connectors: Vec<Option<String>> = outputs.iter().map(|output| Some(output.name.clone())).collect();
        let mut displays: Vec<DisplayInfo> = outputs
            .into_iter()
            .enumerate()
            .map(|(index, output)| DisplayInfo {
                id: format!("output:{}", output.name),
                x: output.x,
                y: output.y,
                width: output.width.max(1) as u32,
                height: output.height.max(1) as u32,
                // eDE lays outputs out left to right from the first.
                primary: index == 0,
                name: output.name,
            })
            .collect();
        finish_ids(&mut displays, &connectors);
        Ok(displays)
    }

    fn screenshot(&self, display: &DisplayInfo) -> Result<Image, String> {
        let capture = self.client.lock().unwrap().capture(CaptureTarget::Output { name: display.name.clone() }).map_err(e("eDE capture"))?;
        rgba_of(&capture)
    }

    fn cursor_position(&self) -> Result<(i32, i32), String> {
        let at = self.client.lock().unwrap().pointer().map_err(e("eDE pointer"))?;
        Ok((at.x.round() as i32, at.y.round() as i32))
    }

    fn list_windows(&self) -> Result<Vec<WindowInfo>, String> {
        let windows = self.client.lock().unwrap().windows().map_err(e("eDE windows"))?;
        // The compositor lists bottom to top.
        Ok(windows
            .into_iter()
            .rev()
            .filter_map(|window| {
                Some(WindowInfo {
                    id: window_number(&window.id)?,
                    title: window.title.unwrap_or_default(),
                    app: window.app_id.unwrap_or_default(),
                    pid: None,
                    x: window.x,
                    y: window.y,
                    width: window.width.max(0) as u32,
                    height: window.height.max(0) as u32,
                    active: window.focused,
                    fullscreen: window.fullscreen,
                })
            })
            .collect())
    }

    fn activate_window(&self, id: u64) -> Result<(), String> {
        let window = WindowId::new(format!("w{id}")).map_err(|err| err.to_string())?;
        self.client.lock().unwrap().call(Request::FocusWindow { window }).map(|_| ()).map_err(e("eDE focus"))
    }

    fn mouse_move(&self, x: i32, y: i32) -> Result<(), String> {
        self.input(InputEvent::PointerMotion { x: f64::from(x), y: f64::from(y) })
    }

    fn mouse_button(&self, button: Button, down: bool) -> Result<(), String> {
        self.input(InputEvent::PointerButton { button: evdev_button(button), pressed: down })
    }

    fn scroll(&self, dx: i32, dy: i32) -> Result<(), String> {
        self.input(InputEvent::PointerAxis { dx: f64::from(dx), dy: f64::from(dy) })
    }

    fn key(&self, key: Key, down: bool) -> Result<(), String> {
        self.keysym(super::linux::keysym_of(key), down)
    }

    fn type_char(&self, ch: char) -> Result<(), String> {
        let keysym = super::linux::char_keysym(ch);
        self.keysym(keysym, true)?;
        std::thread::sleep(Duration::from_millis(6));
        self.keysym(keysym, false)
    }

    fn lock(&self, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Box<dyn LockGuard>, String> {
        Ok(Box::new(Hold::take(self.session.connect()?, on_esc)?))
    }
}

/// The hold, on a connection of its own; a thread waits for the person's Esc.
struct Hold {
    stream: UnixStream,
}

impl Hold {
    fn take(mut client: Client, on_esc: Box<dyn Fn() + Send + Sync>) -> Result<Hold, String> {
        client.hold_input().map_err(e("eDE hold"))?;
        let stream = client.try_clone_stream().map_err(|err| format!("eDE hold: {err}"))?;
        std::thread::Builder::new()
            .name("computer-use-lock".into())
            .spawn(move || loop {
                match client.next_event() {
                    Ok(event) if event.kind == ede_protocol::events::INPUT_ESCAPE => {
                        on_esc();
                        return;
                    }
                    Ok(_) => {}
                    // Closed: the guard dropped (or the compositor went away).
                    Err(_) => return,
                }
            })
            .map_err(|err| format!("lock thread: {err}"))?;
        Ok(Hold { stream })
    }
}

impl LockGuard for Hold {}

impl Drop for Hold {
    /// Closing the connection ends the hold; never joins (this may run on
    /// the lock's own thread, inside `on_esc`).
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ede_protocol::{read_frame, write_frame, Call, Envelope, Message};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| vars.iter().find(|(key, _)| *key == name).map(|(_, value)| value.to_string())
    }

    #[test]
    fn ede_is_its_desktop_name_and_its_socket() {
        assert_eq!(session_of(env(&[("XDG_CURRENT_DESKTOP", "X-Cinnamon"), ("WAYLAND_DISPLAY", "wayland-0")])), None);
        assert_eq!(session_of(env(&[])), None);
        assert!(matches!(session_of(env(&[("XDG_CURRENT_DESKTOP", "eDE")])), Some(Err(_))));
        let session = session_of(env(&[("XDG_CURRENT_DESKTOP", "GNOME:eDE"), ("XDG_RUNTIME_DIR", "/run/user/1000"), ("WAYLAND_DISPLAY", "wayland-1")]))
            .unwrap()
            .unwrap();
        assert_eq!(session.socket, Path::new("/run/user/1000/ede/wayland-1.sock"));
        assert_eq!(session.token_file, Path::new("/run/user/1000/ede/wayland-1.token"));
        let reason = unsupported(&Ok(session)).unwrap();
        assert!(reason.contains("control socket is missing"), "{reason}");
    }

    #[test]
    fn captures_become_rgba_and_buttons_evdev() {
        let info = ede_protocol::CaptureInfo { width: 2, height: 1, stride: 12, format: "argb8888".into() };
        let pixels = [0xff10_2030u32, 0xff00_00ff, 0].iter().flat_map(|p| p.to_le_bytes()).collect();
        let image = rgba_of(&ede_protocol::Capture { info, pixels }).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.rgba, [0x10, 0x20, 0x30, 255, 0, 0, 0xff, 255]);
        assert_eq!(evdev_button(Button::Left), 272);
        assert_eq!(window_number(&WindowId::new("w42").unwrap()), Some(42));
    }

    /// A fake compositor on a socketpair: the hold is taken, the person's
    /// Esc reaches `on_esc`, and dropping the guard closes the connection.
    #[test]
    fn the_hold_hears_esc_and_lets_go_on_drop() {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let server = std::thread::spawn(move || {
            let mut theirs = theirs;
            let mut ops = Vec::new();
            let reply = |stream: &mut UnixStream, id: u64, result: serde_json::Value| {
                write_frame(stream, &serde_json::to_vec(&Message::Reply { id, result }).unwrap()).unwrap();
            };
            for _ in 0..2 {
                let call: Call = serde_json::from_slice(&read_frame(&mut theirs).unwrap()).unwrap();
                let result = match &call.request {
                    Request::Hello { .. } => serde_json::json!({"version": 1, "capabilities": ["input:person"], "session_id": "epilot"}),
                    _ => serde_json::Value::Null,
                };
                ops.push(serde_json::to_value(&call).unwrap()["op"].as_str().unwrap().to_string());
                reply(&mut theirs, call.id, result);
            }
            let escape = Envelope::new("ev-1", ede_protocol::events::INPUT_ESCAPE, 0, ede_protocol::EVENT_SOURCE, None, serde_json::Value::Null);
            write_frame(&mut theirs, &serde_json::to_vec(&Message::Event { event: escape }).unwrap()).unwrap();
            // The guard's drop closes the stream: EOF.
            let closed = read_frame(&mut theirs).is_err();
            (ops, closed)
        });
        let client = Client::over(ours, Token::new("a".repeat(64)).unwrap()).unwrap();
        let escaped = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let tx = Mutex::new(tx);
        let flag = escaped.clone();
        let guard = Hold::take(
            client,
            Box::new(move || {
                flag.store(true, Ordering::SeqCst);
                let _ = tx.lock().unwrap().send(());
            }),
        )
        .unwrap();
        rx.recv_timeout(Duration::from_secs(5)).expect("on_esc");
        assert!(escaped.load(Ordering::SeqCst));
        drop(guard);
        let (ops, closed) = server.join().unwrap();
        assert_eq!(ops, ["hello", "hold_input"]);
        assert!(closed);
    }

    /// #NEW-112 against a real headless `ede-compositor` (pixman, no seat,
    /// no GPU): `EDE_COMPOSITOR=<path to the binary> cargo test -p epilot
    /// headless_ede`. Skipped without it (the desktop CI builds no eDE).
    #[test]
    fn headless_ede_drives_and_captures() {
        let Some(binary) = std::env::var_os("EDE_COMPOSITOR") else {
            eprintln!("EDE_COMPOSITOR unset: skipping the headless eDE run");
            return;
        };
        use std::os::unix::fs::PermissionsExt;
        let runtime = std::env::temp_dir().join(format!("epilot-ede-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
        let config = runtime.join("compositor.toml");
        std::fs::write(&config, "background = \"#102030\"\n").unwrap();
        let child = std::process::Command::new(binary)
            .args(["--headless", "640x480", "--no-xwayland", "--config"])
            .arg(&config)
            .env("XDG_RUNTIME_DIR", &runtime)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env("RUST_LOG", "warn")
            .spawn()
            .unwrap();
        struct Kill(std::process::Child, PathBuf);
        impl Drop for Kill {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
                let _ = std::fs::remove_dir_all(&self.1);
            }
        }
        let _kill = Kill(child, runtime.clone());
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let socket = loop {
            let found = std::fs::read_dir(runtime.join("ede")).ok().and_then(|dir| {
                dir.flatten().map(|entry| entry.path()).find(|path| path.extension().is_some_and(|ext| ext == "sock"))
            });
            if let Some(socket) = found {
                break socket;
            }
            assert!(std::time::Instant::now() < deadline, "ede-compositor did not open its socket");
            std::thread::sleep(Duration::from_millis(20));
        };
        let display = socket.file_stem().unwrap().to_string_lossy().into_owned();
        let session = session_of(env(&[("XDG_CURRENT_DESKTOP", "eDE"), ("XDG_RUNTIME_DIR", runtime.to_str().unwrap()), ("WAYLAND_DISPLAY", &display)]))
            .unwrap()
            .unwrap();
        while !session.token_file.exists() {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(unsupported(&Ok(session.clone())), None);
        let ede = open(&session).unwrap();

        let displays = ede.displays().unwrap();
        assert_eq!(displays.len(), 1);
        let screen = &displays[0];
        assert_eq!((screen.id.as_str(), screen.width, screen.height, screen.primary), ("output:HEADLESS-1", 640, 480, true));
        let shot = ede.screenshot(screen).unwrap();
        assert_eq!((shot.width, shot.height), (640, 480));
        assert_eq!(&shot.rgba[..4], [0x10, 0x20, 0x30, 255]);

        ede.mouse_move(100, 50).unwrap();
        assert_eq!(ede.cursor_position().unwrap(), (100, 50));
        ede.mouse_button(Button::Left, true).unwrap();
        ede.mouse_button(Button::Left, false).unwrap();
        ede.scroll(0, 2).unwrap();
        ede.key(Key::Named(crate::keys::Named::Return), true).unwrap();
        ede.key(Key::Named(crate::keys::Named::Return), false).unwrap();
        ede.type_char('A').unwrap();
        assert!(ede.type_char('\u{2603}').is_err(), "no key types a snowman on the default layout");
        assert_eq!(ede.list_windows().unwrap(), vec![]);

        // The hold is taken and given back, again and again.
        for _ in 0..2 {
            let guard = ede.lock(Box::new(|| {})).unwrap();
            ede.mouse_move(10, 10).unwrap();
            drop(guard);
        }
        assert_eq!(ede.cursor_position().unwrap(), (10, 10));
    }
}
