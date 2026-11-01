//! The compositor's end of `ede-protocol` (#NEW-104): the socket, the
//! grants, and every op. Calls are served on the event loop, one frame at a
//! time; a client that stops reading for 2 s is dropped.

use std::{
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use ede_protocol::{
    events, take_frames, write_frame, Call, Capability, CaptureInfo, CaptureTarget, Envelope, ErrorCode, Failure, Grant, Hello, InputEvent, Message,
    OutputInfo, Request, SessionId, Token, WindowId, WindowInfo, EVENT_SOURCE, PROTOCOL_VERSION,
};
use smithay::{
    backend::input::{ButtonState, KeyState},
    desktop::Window,
    reexports::calloop::{generic::Generic, Interest, Mode, PostAction},
};

use crate::state::Ede;

/// The root grant's session: the shell.
pub const ROOT_SESSION: &str = "ede-shell";

#[derive(Debug, Default)]
pub struct Ipc {
    pub grants: Vec<Grant>,
    clients: Vec<Conn>,
    next_client: u64,
    next_event: u64,
    /// The socket and token file, removed on exit.
    pub paths: Vec<PathBuf>,
}

#[derive(Debug)]
struct Conn {
    id: u64,
    writer: UnixStream,
    inbuf: Vec<u8>,
    token: Option<Token>,
    subscribed: bool,
    dead: bool,
}

impl Drop for Ipc {
    fn drop(&mut self) {
        for p in &self.paths {
            let _ = std::fs::remove_file(p);
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// 32 random bytes as eCore's 64-hex token.
pub fn new_token() -> Token {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).expect("/dev/urandom");
    Token::new(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()).expect("64 hex")
}

fn fail(code: ErrorCode, message: &str) -> Failure {
    Failure::new(code, message)
}

impl Ipc {
    /// The grant with every capability, for the shell.
    pub fn root_grant(&mut self) -> Grant {
        if let Some(g) = self.grants.iter().find(|g| g.session_id.as_str() == ROOT_SESSION) {
            return g.clone();
        }
        let grant = Grant {
            token: new_token(),
            session_id: SessionId::new(ROOT_SESSION).unwrap(),
            run_id: None,
            capabilities: Capability::ALL.to_vec(),
            expires_at: None,
        };
        self.grants.push(grant.clone());
        grant
    }

    fn grant(&self, token: &Token) -> Option<&Grant> {
        self.grants.iter().find(|g| &g.token == token)
    }
}

impl Ede {
    /// Opens `$XDG_RUNTIME_DIR/ede/<wayland-N>.sock` (directory 0700) and
    /// writes the root token next to it (0600).
    pub fn ipc_listen(&mut self, runtime_dir: &Path, wayland_display: &str) -> std::io::Result<PathBuf> {
        let dir = runtime_dir.join("ede");
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        let socket = ede_protocol::socket_path(runtime_dir, wayland_display);
        let _ = std::fs::remove_file(&socket);
        let listener = UnixListener::bind(&socket)?;
        let token_file = ede_protocol::token_path(runtime_dir, wayland_display);
        let _ = std::fs::remove_file(&token_file);
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&token_file)?;
        f.write_all(self.ipc.root_grant().token.as_str().as_bytes())?;
        self.ipc.paths = vec![socket.clone(), token_file];

        listener.set_nonblocking(true)?;
        self.loop_handle
            .insert_source(Generic::new(listener, Interest::READ, Mode::Level), |_, listener, state| {
                while let Ok((stream, _)) = listener.accept() {
                    state.ipc_insert(stream);
                }
                Ok(PostAction::Continue)
            })
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(socket)
    }

    /// Serves one connection (the socket's, or a test's socketpair).
    pub fn ipc_insert(&mut self, stream: UnixStream) {
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
        let Ok(reader) = stream.try_clone() else { return };
        let id = self.ipc.next_client;
        self.ipc.next_client += 1;
        self.ipc.clients.push(Conn { id, writer: stream, inbuf: Vec::new(), token: None, subscribed: false, dead: false });
        let inserted = self.loop_handle.insert_source(Generic::new(reader, Interest::READ, Mode::Level), move |_, reader, state| {
            let mut buf = [0u8; 65536];
            // Level-triggered and readable: one read never blocks.
            let mut r: &UnixStream = reader;
            let n = match r.read(&mut buf) {
                Ok(0) | Err(_) => 0,
                Ok(n) => n,
            };
            if n == 0 || !state.ipc_received(id, &buf[..n]) {
                state.ipc.clients.retain(|c| c.id != id);
                return Ok(PostAction::Remove);
            }
            Ok(PostAction::Continue)
        });
        if inserted.is_err() {
            self.ipc.clients.retain(|c| c.id != id);
        }
    }

    /// `false` = drop the connection.
    fn ipc_received(&mut self, id: u64, bytes: &[u8]) -> bool {
        let frames = {
            let Some(conn) = self.ipc.clients.iter_mut().find(|c| c.id == id) else { return false };
            conn.inbuf.extend_from_slice(bytes);
            match take_frames(&mut conn.inbuf) {
                Ok(f) => f,
                Err(()) => return false,
            }
        };
        for frame in frames {
            let (call_id, outcome) = match serde_json::from_slice::<Call>(&frame) {
                Ok(call) => (call.id, self.ipc_call(id, call.request)),
                Err(e) => (0, Err(fail(ErrorCode::Invalid, &e.to_string()))),
            };
            let (message, extra) = match outcome {
                Ok((result, extra)) => (Message::Reply { id: call_id, result }, extra),
                Err(error) => (Message::Error { id: call_id, error }, None),
            };
            if !self.ipc_send(id, &message, extra.as_deref()) {
                return false;
            }
        }
        self.ipc.clients.iter().any(|c| c.id == id && !c.dead)
    }

    fn ipc_send(&mut self, id: u64, message: &Message, extra: Option<&[u8]>) -> bool {
        let Some(conn) = self.ipc.clients.iter_mut().find(|c| c.id == id) else { return false };
        let json = serde_json::to_vec(message).expect("messages serialize");
        let ok = write_frame(&mut conn.writer, &json).and_then(|_| extra.map_or(Ok(()), |b| write_frame(&mut conn.writer, b))).is_ok();
        if !ok {
            conn.dead = true;
            let _ = conn.writer.shutdown(std::net::Shutdown::Both);
        }
        ok
    }

    /// Sends an event to every subscribed connection whose grant is still
    /// valid.
    pub fn emit(&mut self, kind: &str, data: serde_json::Value) {
        if !self.ipc.clients.iter().any(|c| c.subscribed) {
            return;
        }
        self.ipc.next_event += 1;
        let envelope = Envelope::new(&format!("ev-{}", self.ipc.next_event), kind, now_ms(), EVENT_SOURCE, None, data);
        let message = Message::Event { event: envelope };
        let now = now_ms();
        let ids: Vec<u64> = self
            .ipc
            .clients
            .iter()
            .filter(|c| c.subscribed && !c.dead)
            .filter(|c| c.token.as_ref().and_then(|t| self.ipc.grant(t)).is_some_and(|g| g.expires_at.is_none_or(|e| now < e)))
            .map(|c| c.id)
            .collect();
        for id in ids {
            self.ipc_send(id, &message, None);
        }
    }

    fn ipc_call(&mut self, id: u64, request: Request) -> Result<(serde_json::Value, Option<Vec<u8>>), Failure> {
        let now = now_ms();
        let token = self.ipc.clients.iter().find(|c| c.id == id).and_then(|c| c.token.clone());
        if let Request::Hello { version, token } = &request {
            if *version != PROTOCOL_VERSION {
                return Err(fail(ErrorCode::Invalid, &format!("this compositor speaks ede-protocol v{PROTOCOL_VERSION}")));
            }
            let grant = self.ipc.grant(token).filter(|g| g.expires_at.is_none_or(|e| now < e)).cloned();
            let Some(grant) = grant else { return Err(fail(ErrorCode::Unauthorized, "unknown or expired token")) };
            if let Some(c) = self.ipc.clients.iter_mut().find(|c| c.id == id) {
                c.token = Some(grant.token.clone());
            }
            let hello = Hello { version: PROTOCOL_VERSION, capabilities: grant.capabilities, session_id: grant.session_id };
            return Ok((serde_json::to_value(hello).unwrap(), None));
        }
        let Some(grant) = token.and_then(|t| self.ipc.grant(&t).cloned()) else {
            return Err(fail(ErrorCode::Unauthorized, "say hello with a valid token first"));
        };
        if grant.expires_at.is_some_and(|e| now >= e) {
            return Err(fail(ErrorCode::Unauthorized, "the grant expired"));
        }
        if let Some(cap) = request.capability() {
            if !grant.admits(cap, now) {
                return Err(fail(ErrorCode::Forbidden, &format!("needs the {} capability", cap.as_str())));
            }
        }
        let json = |v: serde_json::Value| Ok((v, None));
        match request {
            Request::Hello { .. } => unreachable!(),
            Request::ListWindows => json(serde_json::to_value(self.window_infos()).unwrap()),
            Request::ListOutputs => json(serde_json::to_value(self.output_infos()).unwrap()),
            Request::Subscribe => {
                if let Some(c) = self.ipc.clients.iter_mut().find(|c| c.id == id) {
                    c.subscribed = true;
                }
                json(serde_json::Value::Null)
            }
            Request::FocusWindow { window } => {
                let w = self.window_by_id(&window)?;
                if self.is_locked() {
                    return Err(fail(ErrorCode::Forbidden, "the session is locked"));
                }
                self.focus(Some(w));
                json(serde_json::Value::Null)
            }
            Request::CloseWindow { window } => {
                let w = self.window_by_id(&window)?;
                self.close_window(&w);
                json(serde_json::Value::Null)
            }
            Request::MoveWindow { window, x, y } => {
                let w = self.window_by_id(&window)?;
                if crate::state::window_state(&w).tile.is_some() {
                    self.untile(&w, Some((x, y).into()));
                } else {
                    self.move_window(&w, (x, y).into(), false);
                }
                json(serde_json::Value::Null)
            }
            Request::Capture { target } => {
                let frame = match target {
                    CaptureTarget::Output { name } => {
                        let output = self.space.outputs().find(|o| o.name() == name).cloned().ok_or_else(|| fail(ErrorCode::NotFound, "no such output"))?;
                        crate::backend::capture_output(self, &output)
                    }
                    CaptureTarget::Window { window } => {
                        let w = self.window_by_id(&window)?;
                        if !self.may_capture(&w) {
                            return Err(fail(ErrorCode::Forbidden, "a protected window is never captured"));
                        }
                        crate::backend::capture_window(&w)
                    }
                }
                .map_err(|e| fail(ErrorCode::Internal, &e))?;
                let info = CaptureInfo { width: frame.width, height: frame.height, stride: frame.width * 4, format: "argb8888".into() };
                let bytes = frame.pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
                Ok((serde_json::to_value(info).unwrap(), Some(bytes)))
            }
            Request::Input { event } => {
                let refused = match event {
                    InputEvent::Key { key, pressed } => {
                        self.drive_key((key + 8).into(), if pressed { KeyState::Pressed } else { KeyState::Released }, now as u32)
                    }
                    InputEvent::PointerMotion { x, y } => self.drive_pointer_motion((x, y).into(), now as u32),
                    InputEvent::PointerButton { button, pressed } => {
                        self.drive_pointer_button(button, if pressed { ButtonState::Pressed } else { ButtonState::Released }, now as u32)
                    }
                };
                refused.map_err(|r| fail(ErrorCode::Forbidden, if r == crate::protected::Refused::Locked { "the session is locked" } else { "a protected surface is never driven" }))?;
                json(serde_json::Value::Null)
            }
            Request::ReloadConfig => {
                self.reload_config().map_err(|e| fail(ErrorCode::Invalid, &e))?;
                json(serde_json::Value::Null)
            }
            Request::Lock => {
                if self.config.lock.command.is_empty() {
                    return Err(fail(ErrorCode::Unavailable, "no [lock] command is configured"));
                }
                self.request_lock();
                json(serde_json::Value::Null)
            }
            Request::MintGrant { session_id, run_id, capabilities, expires_at } => {
                if let Some(extra) = capabilities.iter().find(|c| !grant.capabilities.contains(c)) {
                    return Err(fail(ErrorCode::Forbidden, &format!("cannot grant {}: not held", extra.as_str())));
                }
                if let Some(limit) = grant.expires_at {
                    if expires_at.is_none_or(|e| e > limit) {
                        return Err(fail(ErrorCode::Forbidden, "cannot outlive the minting grant"));
                    }
                }
                let minted = Grant { token: new_token(), session_id, run_id, capabilities, expires_at };
                self.ipc.grants.push(minted.clone());
                json(serde_json::to_value(minted).unwrap())
            }
            Request::RevokeGrant { token } => {
                let g = self.ipc.grant(&token).ok_or_else(|| fail(ErrorCode::NotFound, "no such grant"))?;
                if g.session_id.as_str() == ROOT_SESSION {
                    return Err(fail(ErrorCode::Forbidden, "the root grant lives as long as the compositor"));
                }
                self.ipc.grants.retain(|g| g.token != token);
                json(serde_json::Value::Null)
            }
        }
    }

    fn window_by_id(&self, id: &WindowId) -> Result<Window, Failure> {
        self.space.elements().find(|w| &window_id(w) == id).cloned().ok_or_else(|| fail(ErrorCode::NotFound, "no such window"))
    }

    pub fn window_infos(&self) -> Vec<WindowInfo> {
        let focused = self.focused_window();
        self.space
            .elements()
            .filter(|w| w.x11_surface().is_none_or(|x| !x.is_override_redirect()))
            .filter_map(|w| {
                let g = self.space.element_geometry(w)?;
                let protected = self.is_protected_window(w);
                let (title, app_id) = window_title(w);
                Some(WindowInfo {
                    id: window_id(w),
                    title: title.filter(|_| !protected),
                    app_id,
                    x: g.loc.x,
                    y: g.loc.y,
                    width: g.size.w,
                    height: g.size.h,
                    focused: focused.as_ref() == Some(w),
                    fullscreen: crate::state::window_state(w).fullscreen.is_some(),
                    protected,
                    x11: w.x11_surface().is_some(),
                })
            })
            .collect()
    }

    pub fn output_infos(&self) -> Vec<OutputInfo> {
        self.space
            .outputs()
            .filter_map(|o| {
                let g = self.space.output_geometry(o)?;
                Some(OutputInfo { name: o.name(), x: g.loc.x, y: g.loc.y, width: g.size.w, height: g.size.h, scale: o.current_scale().fractional_scale() })
            })
            .collect()
    }

    pub fn window_event(&mut self, kind: &str, window: &Window) {
        if self.ipc.clients.iter().any(|c| c.subscribed) {
            let (_, app_id) = window_title(window);
            self.emit(kind, serde_json::json!({ "window": window_id(window), "appId": app_id }));
        }
    }

    /// Re-reads the config file and applies what can change live (all but
    /// `xwayland`).
    pub fn reload_config(&mut self) -> Result<(), String> {
        let Some(path) = self.config_path.clone() else { return Err("started without a config file (safe mode)".into()) };
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let mut config = crate::Config::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        config.xwayland = self.config.xwayland;
        let kb = config.keyboard.clone();
        self.config = config;
        let keyboard = self.seat.get_keyboard().unwrap();
        let xkb = smithay::input::keyboard::XkbConfig { layout: &kb.layout, variant: &kb.variant, options: Some(kb.options.clone()).filter(|o| !o.is_empty()), ..Default::default() };
        if let Err(e) = keyboard.set_xkb_config(self, xkb) {
            tracing::warn!("keyboard layout {:?} rejected on reload: {e:?}", kb.layout);
        }
        keyboard.change_repeat_info(kb.repeat_rate, kb.repeat_delay);
        self.emit(events::CONFIG_RELOADED, serde_json::Value::Null);
        Ok(())
    }
}

/// A window's stable id for the protocol (`w<n>`, never reused).
pub fn window_id(window: &Window) -> WindowId {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    window.user_data().get_or_insert(|| WindowId::new(format!("w{}", NEXT.fetch_add(1, Ordering::Relaxed))).unwrap()).clone()
}

fn window_title(window: &Window) -> (Option<String>, Option<String>) {
    if let Some(t) = window.toplevel() {
        return smithay::wayland::compositor::with_states(t.wl_surface(), |s| {
            s.data_map
                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                .map(|d| {
                    let d = d.lock().unwrap();
                    (d.title.clone(), d.app_id.clone())
                })
                .unwrap_or_default()
        });
    }
    #[cfg(feature = "xwayland")]
    if let Some(x) = window.x11_surface() {
        return (Some(x.title()), Some(x.class()).filter(|c| !c.is_empty()));
    }
    (None, None)
}
