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
    OutputInfo, PointerInfo, Request, SessionId, Token, WindowId, WindowInfo, EVENT_SOURCE, PROTOCOL_VERSION,
};
use smithay::{
    backend::input::{ButtonState, KeyState},
    desktop::Window,
    reexports::calloop::{generic::Generic, Interest, Mode, PostAction},
};

use crate::state::Ede;

/// The root grant's session: the shell.
pub const ROOT_SESSION: &str = "exponential-shell";

#[derive(Debug, Default)]
pub struct Ipc {
    pub grants: Vec<Grant>,
    clients: Vec<Conn>,
    next_client: u64,
    next_event: u64,
    /// The socket and token file, removed on exit.
    pub paths: Vec<PathBuf>,
    /// The portal's global shortcuts (#EDE-15).
    pub shortcuts: crate::shortcuts::Shortcuts,
    /// The modifiers held for `InputEvent::Keysym` (#EDE-22).
    pub keysyms: crate::keysym::Keysyms,
    /// `hold_input` (#EDE-23).
    pub hold: crate::hold::Hold,
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

pub(crate) fn now_ms() -> i64 {
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

    pub(crate) fn next_event_id(&mut self) -> u64 {
        self.next_event += 1;
        self.next_event
    }

    /// A grant the compositor hands a process it starts (the sealed lock).
    pub fn mint_for(&mut self, session: &str, capabilities: Vec<Capability>) -> Grant {
        let grant = Grant { token: new_token(), session_id: SessionId::new(session).unwrap(), run_id: None, capabilities, expires_at: None };
        self.grants.push(grant.clone());
        grant
    }

    /// Is the shell (the root grant) subscribed to events? Hotkeys go to it
    /// then; otherwise the compositor falls back ([`crate::hotkeys`]).
    pub fn shell_subscribed(&self) -> bool {
        self.clients.iter().any(|c| c.subscribed && !c.dead && c.token.as_ref().and_then(|t| self.grant(t)).is_some_and(|g| g.session_id.as_str() == ROOT_SESSION))
    }
}

impl Ede {
    pub fn shell_listening(&self) -> bool {
        self.ipc.shell_subscribed()
    }

    /// Opens `$XDG_RUNTIME_DIR/exponential-desktop/<wayland-N>.sock` (directory 0700) and
    /// writes the root token next to it (0600).
    pub fn ipc_listen(&mut self, runtime_dir: &Path, wayland_display: &str) -> std::io::Result<PathBuf> {
        let dir = runtime_dir.join("exponential-desktop");
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
                state.ipc_dropped(id);
                return Ok(PostAction::Remove);
            }
            Ok(PostAction::Continue)
        });
        if inserted.is_err() {
            self.ipc.clients.retain(|c| c.id != id);
        }
    }

    /// A connection went; its hold goes with it, the shell's last one
    /// starts the recovery clock.
    fn ipc_dropped(&mut self, id: u64) {
        let was_shell = self.ipc.shell_subscribed();
        self.ipc.clients.retain(|c| c.id != id);
        self.ipc.shortcuts.drop_client(id);
        self.release_input(id);
        if was_shell && !self.ipc.shell_subscribed() {
            self.shell_left();
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

    pub(crate) fn ipc_send(&mut self, id: u64, message: &Message, extra: Option<&[u8]>) -> bool {
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
        let envelope = Envelope::new(&format!("ev-{}", self.ipc.next_event_id()), kind, now_ms(), EVENT_SOURCE, None, data);
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
                if grant.session_id.as_str() == ROOT_SESSION {
                    self.shell_arrived();
                }
                json(serde_json::Value::Null)
            }
            Request::FocusWindow { window } => {
                let w = self.any_window_by_id(&window)?;
                if self.is_locked() {
                    return Err(fail(ErrorCode::Forbidden, "the session is locked"));
                }
                // A minimized window comes back first; a parked one brings
                // its workspace along (the taskbar).
                self.restore_window(&w);
                let ws = crate::state::window_state(&w).workspace;
                if ws != self.active_workspace() {
                    self.switch_workspace(ws);
                }
                self.focus(Some(w));
                json(serde_json::Value::Null)
            }
            Request::CloseWindow { window } => {
                // Parked and minimized windows close from the taskbar too.
                let w = self.any_window_by_id(&window)?;
                self.close_window(&w);
                json(serde_json::Value::Null)
            }
            Request::MinimizeWindow { window } => {
                let w = self.any_window_by_id(&window)?;
                if self.is_locked() {
                    return Err(fail(ErrorCode::Forbidden, "the session is locked"));
                }
                self.minimize_window(&w);
                json(serde_json::Value::Null)
            }
            Request::RestoreWindow { window } => {
                let w = self.any_window_by_id(&window)?;
                if self.is_locked() {
                    return Err(fail(ErrorCode::Forbidden, "the session is locked"));
                }
                if self.restore_window(&w) {
                    let ws = crate::state::window_state(&w).workspace;
                    if ws != self.active_workspace() {
                        self.switch_workspace(ws);
                    }
                    self.focus(Some(w));
                }
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
            Request::Capture { target, since } => {
                if since == Some(self.frame_serial) {
                    let info = CaptureInfo { width: 0, height: 0, stride: 0, format: "argb8888".into(), serial: self.frame_serial, unchanged: true };
                    return Ok((serde_json::to_value(info).unwrap(), None));
                }
                let frame = match target {
                    CaptureTarget::Output { name } => {
                        let output = self.space.outputs().find(|o| o.name() == name).cloned().ok_or_else(|| fail(ErrorCode::NotFound, "no such output"))?;
                        crate::backend::capture_output(self, &output)
                    }
                    CaptureTarget::Window { window } => {
                        // A parked or minimized window still has its last
                        // frame (the taskbar's hover preview).
                        let w = self.any_window_by_id(&window)?;
                        if !self.may_capture(&w) {
                            return Err(fail(ErrorCode::Forbidden, "a protected window is never captured"));
                        }
                        crate::backend::capture_window(&w)
                    }
                }
                .map_err(|e| fail(ErrorCode::Internal, &e))?;
                let info = CaptureInfo { width: frame.width, height: frame.height, stride: frame.width * 4, format: "argb8888".into(), serial: self.frame_serial, unchanged: false };
                let bytes = frame.pixels.iter().flat_map(|p| p.to_le_bytes()).collect();
                Ok((serde_json::to_value(info).unwrap(), Some(bytes)))
            }
            Request::Input { event, bindings } => {
                if bindings && grant.session_id.as_str() != ROOT_SESSION {
                    return Err(fail(ErrorCode::Forbidden, "only the shell's grant drives the bindings"));
                }
                // `bindings` = the person's own input (the harness): it
                // reaches the lock screen and protected surfaces like a
                // real keyboard; the agent path refuses those.
                let refused = match (event, bindings) {
                    (InputEvent::Key { key, pressed }, true) => {
                        self.keyboard_key((key + 8).into(), if pressed { KeyState::Pressed } else { KeyState::Released }, now as u32);
                        Ok(())
                    }
                    (InputEvent::PointerMotion { x, y }, true) => {
                        self.person_pointer_motion_to((x, y).into(), now as u32);
                        Ok(())
                    }
                    (InputEvent::PointerButton { button, pressed }, true) => {
                        self.person_pointer_button(button, if pressed { ButtonState::Pressed } else { ButtonState::Released }, now as u32);
                        Ok(())
                    }
                    (InputEvent::Key { key, pressed }, false) => {
                        self.drive_key((key + 8).into(), if pressed { KeyState::Pressed } else { KeyState::Released }, now as u32, false)
                    }
                    (InputEvent::PointerMotionRelative { dx, dy }, bindings) => {
                        let pos = self.seat.get_pointer().unwrap().current_location() + smithay::utils::Point::<f64, smithay::utils::Logical>::from((dx, dy));
                        if bindings {
                            self.person_pointer_motion_to(pos, now as u32);
                            Ok(())
                        } else {
                            self.drive_pointer_motion(pos, now as u32)
                        }
                    }
                    (InputEvent::Keysym { keysym, pressed }, bindings) => match self.drive_keysym(keysym, pressed, bindings, now as u32) {
                        Ok(false) => return Err(fail(ErrorCode::NotFound, &format!("no key makes keysym {keysym:#x} on this layout"))),
                        other => other.map(|_| ()),
                    },
                    (InputEvent::PointerAxis { dx, dy }, _) => self.drive_pointer_axis((dx, dy), None, now as u32),
                    (InputEvent::PointerAxisDiscrete { dx, dy }, _) => self.drive_pointer_axis((dx as f64 * 15.0, dy as f64 * 15.0), Some((dx * 120, dy * 120)), now as u32),
                    (InputEvent::PointerMotion { x, y }, false) => self.drive_pointer_motion((x, y).into(), now as u32),
                    (InputEvent::PointerButton { button, pressed }, false) => {
                        self.drive_pointer_button(button, if pressed { ButtonState::Pressed } else { ButtonState::Released }, now as u32)
                    }
                };
                refused.map_err(|r| fail(ErrorCode::Forbidden, if r == crate::protected::Refused::Locked { "the session is locked" } else { "a protected surface is never driven" }))?;
                json(serde_json::Value::Null)
            }
            Request::PointerPosition => {
                let at = self.seat.get_pointer().unwrap().current_location();
                json(serde_json::to_value(PointerInfo { x: at.x, y: at.y }).unwrap())
            }
            Request::HoldInput => {
                self.hold_input(id);
                json(serde_json::Value::Null)
            }
            Request::ReleaseInput => {
                self.release_input(id);
                json(serde_json::Value::Null)
            }
            Request::ReloadConfig => {
                self.reload_config().map_err(|e| fail(ErrorCode::Invalid, &e))?;
                json(serde_json::Value::Null)
            }
            Request::ListWorkspaces => json(serde_json::to_value(self.workspace_infos()).unwrap()),
            Request::SwitchWorkspace { index } => {
                if index >= crate::workspaces::MAX {
                    return Err(fail(ErrorCode::Invalid, &format!("workspaces go up to {}", crate::workspaces::MAX)));
                }
                self.switch_workspace(index);
                json(serde_json::Value::Null)
            }
            Request::MoveToWorkspace { window, index } => {
                if index >= crate::workspaces::MAX {
                    return Err(fail(ErrorCode::Invalid, &format!("workspaces go up to {}", crate::workspaces::MAX)));
                }
                let w = self.any_window_by_id(&window)?;
                self.move_to_workspace(&w, index, false);
                json(serde_json::Value::Null)
            }
            Request::SetOutput { name, mode, scale, position } => {
                let mode = match mode {
                    Some(m) => Some(crate::config::parse_mode(&m).ok_or_else(|| fail(ErrorCode::Invalid, "mode must be WxH or WxH@R"))?),
                    None => None,
                };
                self.set_output(&name, &crate::outputs::OutputChange { mode, scale, position }).map_err(|e| fail(ErrorCode::Invalid, &e))?;
                json(serde_json::Value::Null)
            }
            Request::BindShortcuts { owner, shortcuts } => {
                let taken = |t: &crate::shortcuts::Trigger| t.syms().iter().any(|&sym| crate::input::binding(&t.mods(), sym).is_some());
                let bound = self.ipc.shortcuts.bind(id, &owner, shortcuts, taken);
                json(serde_json::to_value(bound).unwrap())
            }
            Request::UnbindShortcuts { owner } => {
                self.ipc.shortcuts.unbind(&owner);
                json(serde_json::Value::Null)
            }
            Request::Unlock => {
                let token = self.ipc.clients.iter().find(|c| c.id == id).and_then(|c| c.token.clone()).ok_or_else(|| fail(ErrorCode::Unauthorized, "no token"))?;
                self.sealed_unlock(&token).map_err(|e| fail(ErrorCode::Forbidden, e))?;
                json(serde_json::Value::Null)
            }
            Request::Logout => {
                tracing::info!("logout requested over ede-protocol");
                self.loop_signal.stop();
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

    /// On screen or parked on another workspace.
    fn any_window_by_id(&self, id: &WindowId) -> Result<Window, Failure> {
        self.all_windows().into_iter().find(|w| &window_id(w) == id).ok_or_else(|| fail(ErrorCode::NotFound, "no such window"))
    }

    pub fn window_infos(&self) -> Vec<WindowInfo> {
        let focused = self.focused_window();
        self.all_windows()
            .iter()
            .filter(|w| w.x11_surface().is_none_or(|x| !x.is_override_redirect()))
            .filter_map(|w| {
                let st = crate::state::window_state(w);
                let g = self.space.element_geometry(w).or_else(|| st.parked_at.map(|at| smithay::utils::Rectangle::new(at, w.geometry().size)))?;
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
                    fullscreen: st.fullscreen.is_some(),
                    protected,
                    x11: w.x11_surface().is_some(),
                    workspace: st.workspace,
                    minimized: st.minimized,
                })
            })
            .collect()
    }

    pub fn output_infos(&self) -> Vec<OutputInfo> {
        self.space
            .outputs()
            .filter_map(|o| {
                let g = self.space.output_geometry(o)?;
                let current = o.current_mode();
                let preferred = o.preferred_mode();
                let modes = o
                    .modes()
                    .into_iter()
                    .map(|m| ede_protocol::ModeInfo { width: m.size.w, height: m.size.h, refresh: m.refresh, preferred: Some(m) == preferred, current: Some(m) == current })
                    .collect();
                let p = o.physical_properties();
                Some(OutputInfo { name: o.name(), x: g.loc.x, y: g.loc.y, width: g.size.w, height: g.size.h, scale: o.current_scale().fractional_scale(), modes, make: p.make, model: p.model, physical_mm: [p.size.w, p.size.h] })
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
        let mut config = crate::Config::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?.with_system_keyboard();
        config.xwayland = self.config.xwayland;
        let kb = config.keyboard.clone();
        self.config = config;
        let keyboard = self.seat.get_keyboard().unwrap();
        let xkb = smithay::input::keyboard::XkbConfig { layout: &kb.layout, variant: &kb.variant, options: Some(kb.options.clone()).filter(|o| !o.is_empty()), ..Default::default() };
        self.virtual_keymap = None;
        if let Err(e) = keyboard.set_xkb_config(self, xkb) {
            tracing::warn!("keyboard layout {:?} rejected on reload: {e:?}", kb.layout);
        }
        keyboard.change_repeat_info(kb.repeat_rate, kb.repeat_delay);
        self.apply_output_settings();
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

/// The title (else the app id, else nothing) for a frame.
pub fn window_title_of(window: &Window) -> String {
    let (title, app_id) = window_title(window);
    title.filter(|t| !t.is_empty()).or(app_id).unwrap_or_default()
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
