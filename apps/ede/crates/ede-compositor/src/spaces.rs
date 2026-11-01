//! Agent spaces (#NEW-110): one offscreen desktop per agent session.
//!
//! A space is a world of its own inside the compositor: its own
//! [`Space`] of windows on a virtual output that no backend ever scans out,
//! its own `wl_seat` (so focus, input and the clipboard never touch the
//! person's), and its own Wayland socket. A client is in a space when it
//! connected through that socket ([`ClientState::space`]); it then sees only
//! the space's output and seat, never the desktop's, and none of the
//! privileged globals (layer-shell, session lock, input methods, virtual
//! keyboards). Its windows get frame callbacks from the space's own clock,
//! capped at [`MAX_FPS`]. Every surface keeps its own buffer, so a covered
//! window still captures whole.
//!
//! Processes launched into a space run in a systemd user slice
//! (`ede-space-<pid>_<id>.slice`, CPU and memory limits) when the user
//! manager answers, else in a process group of their own without limits
//! ([`Isolation`]). Closing the space disconnects its clients, stops the
//! slice and kills the groups.
//!
//! Built for #NEW-111 (ePilot driving apps in the background: launch,
//! capture, input, all space-local) and #NEW-113 (lending: a window moves
//! between the desktop's [`Space`] and an agent's, see
//! [`Ede::agent_space_of`]).

use std::{
    os::unix::{fs::DirBuilderExt, net::UnixStream, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use ede_protocol::{events, ErrorCode, Failure, InputEvent, Isolation, SessionId, SpaceId, SpaceInfo, SpaceLimits, WindowId};
use smithay::{
    backend::input::{ButtonState, KeyState},
    desktop::{Space, Window, WindowSurfaceType},
    input::{
        keyboard::{FilterResult, XkbConfig},
        pointer::{ButtonEvent, MotionEvent},
        Seat,
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::{
        calloop::{
            generic::Generic,
            timer::{TimeoutAction, Timer},
            Interest, Mode as SourceMode, PostAction, RegistrationToken,
        },
        wayland_server::{
            backend::{DisconnectReason, GlobalId},
            protocol::wl_surface::WlSurface,
            Client, ListeningSocket, Resource,
        },
    },
    utils::{Logical, Point, Rectangle, Transform, SERIAL_COUNTER},
    wayland::seat::WaylandFocus,
};

use crate::{
    layout,
    protected::Refused,
    state::{ClientState, Ede},
};

/// Frames per second a space offers at most: agents read pixels, they do
/// not watch video.
pub const MAX_FPS: u32 = 30;
/// Largest space (an 8K capture still fits one ede-protocol frame).
pub const MAX_SIZE: (i32, i32) = (7680, 4320);
pub const MIN_SIZE: i32 = 64;
/// Output and seat names of a space: `space:<id>`.
const PREFIX: &str = "space:";

pub struct AgentSpace {
    pub id: SpaceId,
    pub session_id: SessionId,
    /// Its windows on its one output at (0, 0).
    pub space: Space<Window>,
    pub output: Output,
    pub seat: Seat<Ede>,
    pub size: (i32, i32),
    pub fps: u32,
    pub limits: SpaceLimits,
    pub isolation: Isolation,
    /// Its Wayland socket (an absolute `WAYLAND_DISPLAY`).
    pub socket: PathBuf,
    output_global: Option<GlobalId>,
    sources: Vec<RegistrationToken>,
    clients: Vec<Client>,
    children: Vec<Child>,
    launches: u64,
    /// The session held a grant once: it closes when the last one goes.
    pub(crate) had_grant: bool,
}

impl AgentSpace {
    /// `ede-space-<compositor pid>_<id>.slice`: under `ede-space.slice`,
    /// unique across compositors.
    pub fn slice(&self) -> String {
        format!("ede-space-{}_{}.slice", std::process::id(), self.id)
    }

    pub fn window(&self, id: &WindowId) -> Option<Window> {
        self.space.elements().find(|w| &crate::ipc::window_id(w) == id).cloned()
    }

    /// The topmost surface under `pos` (space-local) with its origin.
    pub fn surface_under(&self, pos: Point<f64, Logical>) -> Option<(WlSurface, Point<f64, Logical>)> {
        let (window, loc) = self.space.element_under(pos)?;
        window.surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL).map(|(s, p)| (s, (p + loc).to_f64()))
    }

    /// SIGTERM to every launch's process group and the slice stopped (both
    /// right away, so a compositor that exits still cleans up), then a
    /// thread that reaps (SIGKILL after 3 s).
    fn stop_processes(&mut self) {
        let children = std::mem::take(&mut self.children);
        for c in &children {
            // Safety: kill(2) on our own child's process group.
            unsafe { libc::kill(-(c.id() as i32), libc::SIGTERM) };
        }
        if self.isolation == Isolation::Systemd {
            let slice = self.slice();
            let _ = systemctl(&["stop", "--no-block", &slice]);
            let _ = systemctl(&["revert", &slice]);
        }
        if children.is_empty() {
            return;
        }
        let _ = std::thread::Builder::new().name("ede-space-reaper".into()).spawn(move || {
            for mut c in children {
                for _ in 0..30 {
                    if !matches!(c.try_wait(), Ok(None)) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                if matches!(c.try_wait(), Ok(None)) {
                    unsafe { libc::kill(-(c.id() as i32), libc::SIGKILL) };
                    let _ = c.wait();
                }
            }
        });
    }
}

impl Drop for AgentSpace {
    fn drop(&mut self) {
        self.stop_processes();
    }
}

#[derive(Default)]
pub struct Spaces {
    pub list: Vec<AgentSpace>,
    /// Where space sockets go: `<runtime>/ede/<wayland-N>-spaces` once
    /// ede-protocol listens, else a private directory under /tmp.
    pub dir: Option<PathBuf>,
    owns_dir: bool,
    systemd: Option<bool>,
    sweeping: bool,
}

impl Spaces {
    pub fn get(&self, id: &SpaceId) -> Option<&AgentSpace> {
        self.list.iter().find(|a| &a.id == id)
    }

    pub fn get_mut(&mut self, id: &SpaceId) -> Option<&mut AgentSpace> {
        self.list.iter_mut().find(|a| &a.id == id)
    }

    fn dir(&mut self) -> std::io::Result<PathBuf> {
        if self.dir.is_none() {
            static N: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!("ede-spaces-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
            self.dir = Some(dir);
            self.owns_dir = true;
        }
        let dir = self.dir.clone().unwrap();
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
        Ok(dir)
    }

    /// Is the user's systemd manager there to hold slices? Asked once.
    fn systemd(&mut self) -> bool {
        *self.systemd.get_or_insert_with(|| systemctl(&["show-environment"]).is_ok_and(|s| s.success()))
    }
}

impl Drop for Spaces {
    fn drop(&mut self) {
        self.list.clear();
        if self.owns_dir {
            if let Some(dir) = &self.dir {
                let _ = std::fs::remove_dir_all(dir);
            }
        }
    }
}

fn systemctl(args: &[&str]) -> std::io::Result<std::process::ExitStatus> {
    Command::new("systemctl").arg("--user").args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status()
}

fn fail(code: ErrorCode, message: &str) -> Failure {
    Failure::new(code, message)
}

/// The space a client connected into (`None` = the desktop, Xwayland too).
pub fn client_space(client: &Client) -> Option<SpaceId> {
    client.get_data::<ClientState>().and_then(|c| c.space.clone())
}

/// Privileged globals (layer-shell, session lock, input methods, virtual
/// keyboards) are the desktop's alone.
pub fn desktop_client(client: &Client) -> bool {
    client_space(client).is_none()
}

/// The space a seat or output global belongs to, from its name: smithay
/// keeps the global data private, but its `Debug` starts with the name, so
/// a writer that stops right after it reads only that much.
pub fn global_space(data: &dyn std::fmt::Debug) -> Option<SpaceId> {
    struct Name {
        seen: String,
        name: Option<String>,
    }
    impl std::fmt::Write for Name {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.seen.push_str(s);
            let Some(start) = self.seen.find("name: \"") else { return Ok(()) };
            let rest = &self.seen[start + 7..];
            match rest.find('"') {
                Some(end) => {
                    self.name = Some(rest[..end].to_string());
                    Err(std::fmt::Error)
                }
                None => Ok(()),
            }
        }
    }
    let mut w = Name { seen: String::new(), name: None };
    let _ = std::fmt::write(&mut w, format_args!("{data:?}"));
    w.name?.strip_prefix(PREFIX).and_then(|id| SpaceId::new(id).ok())
}

/// `can_view` of the seat and output globals: a space's clients see its
/// own, everyone else the desktop's.
pub fn global_visible(client: &Client, data: &dyn std::fmt::Debug) -> bool {
    client_space(client) == global_space(data)
}

impl Ede {
    /// A new space for `session_id`; `fps` and `limits` default to
    /// `[spaces]` in the config.
    pub fn create_space(&mut self, session_id: SessionId, size: (i32, i32), fps: Option<u32>, limits: Option<SpaceLimits>) -> Result<SpaceId, Failure> {
        let cfg = self.config.spaces.clone();
        if self.spaces.list.len() >= cfg.max {
            return Err(fail(ErrorCode::Limit, &format!("at most {} spaces", cfg.max)));
        }
        let (w, h) = size;
        if !(MIN_SIZE..=MAX_SIZE.0).contains(&w) || !(MIN_SIZE..=MAX_SIZE.1).contains(&h) {
            return Err(fail(ErrorCode::Invalid, &format!("a space is {MIN_SIZE}…{}×{MIN_SIZE}…{} px", MAX_SIZE.0, MAX_SIZE.1)));
        }
        let fps = fps.unwrap_or(cfg.fps).clamp(1, MAX_FPS);
        let limits = limits.unwrap_or(SpaceLimits { cpu_percent: cfg.cpu_percent, memory_mb: cfg.memory_mb });
        if limits.cpu_percent == 0 || limits.memory_mb == 0 {
            return Err(fail(ErrorCode::Invalid, "limits must be above zero"));
        }
        // Unique in the process, so slice and unit names never collide.
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = SpaceId::new(format!("s{}", NEXT.fetch_add(1, Ordering::Relaxed))).unwrap();
        let name = format!("{PREFIX}{id}");

        let dir = self.spaces.dir().map_err(|e| fail(ErrorCode::Internal, &format!("space socket directory: {e}")))?;
        let socket_path = dir.join(format!("{id}.wayland"));
        let socket = ListeningSocket::bind_absolute(socket_path.clone()).map_err(|e| fail(ErrorCode::Internal, &format!("space socket: {e}")))?;

        let output = Output::new(
            name.clone(),
            PhysicalProperties { size: (0, 0).into(), subpixel: Subpixel::Unknown, make: "Exponential".into(), model: "eDE agent space".into() },
        );
        let mode = Mode { size: size.into(), refresh: fps as i32 * 1000 };
        let output_global = output.create_global::<Ede>(&self.display_handle);
        output.change_current_state(Some(mode), Some(Transform::Normal), None, Some((0, 0).into()));
        output.set_preferred(mode);
        let mut space = Space::default();
        space.map_output(&output, (0, 0));

        let mut seat = self.seat_state.new_wl_seat(&self.display_handle, name);
        let kb = &self.config.keyboard;
        let xkb = XkbConfig { layout: &kb.layout, variant: &kb.variant, options: Some(kb.options.clone()).filter(|o| !o.is_empty()), ..Default::default() };
        if seat.add_keyboard(xkb, kb.repeat_delay, kb.repeat_rate).is_err() {
            seat.add_keyboard(XkbConfig::default(), kb.repeat_delay, kb.repeat_rate).expect("default xkb keymap");
        }
        seat.add_pointer();

        let had_grant = self.ipc.live_sessions().contains(&session_id);
        let isolation = if cfg.cgroup && self.spaces.systemd() { Isolation::Systemd } else { Isolation::ProcessGroup };
        let mut agent = AgentSpace {
            id: id.clone(),
            session_id,
            space,
            output,
            seat,
            size,
            fps,
            limits,
            isolation,
            socket: socket_path,
            output_global: Some(output_global),
            sources: Vec::new(),
            clients: Vec::new(),
            children: Vec::new(),
            launches: 0,
            had_grant,
        };
        if isolation == Isolation::Systemd {
            let cpu = format!("CPUQuota={}%", limits.cpu_percent);
            let mem = format!("MemoryMax={}M", limits.memory_mb);
            if !systemctl(&["set-property", "--runtime", &agent.slice(), &cpu, &mem]).is_ok_and(|s| s.success()) {
                tracing::warn!("space {id}: systemd refused the slice limits; launching into process groups instead");
                agent.isolation = Isolation::ProcessGroup;
            }
        }

        let sid = id.clone();
        let accept = self.loop_handle.insert_source(Generic::new(socket, Interest::READ, SourceMode::Level), move |_, socket, state| {
            while let Ok(Some(stream)) = socket.accept() {
                state.insert_space_client(&sid, stream);
            }
            Ok(PostAction::Continue)
        });
        let interval = Duration::from_millis(1000 / fps as u64);
        let sid = id.clone();
        let clock = self.loop_handle.insert_source(Timer::from_duration(interval), move |_, _, state| {
            state.space_frame(&sid);
            TimeoutAction::ToDuration(interval)
        });
        match (accept, clock) {
            (Ok(a), Ok(c)) => agent.sources = vec![a, c],
            _ => return Err(fail(ErrorCode::Internal, "could not start the space's event sources")),
        }
        self.spaces.list.push(agent);
        self.start_space_sweep();
        tracing::info!("space {id} ({w}×{h} @ {fps} fps, {isolation:?})");
        self.emit(events::SPACE_CREATED, serde_json::json!({ "space": id, "sessionId": self.spaces.get(&id).unwrap().session_id }));
        Ok(id)
    }

    /// Closes a space: its clients are disconnected, its processes stopped,
    /// its output and seat withdrawn. `false` = no such space.
    pub fn destroy_space(&mut self, id: &SpaceId) -> bool {
        let Some(i) = self.spaces.list.iter().position(|a| &a.id == id) else { return false };
        let mut agent = self.spaces.list.remove(i);
        for t in agent.sources.drain(..) {
            // Drops the listening socket, which unlinks it.
            self.loop_handle.remove(t);
        }
        for client in agent.clients.drain(..) {
            self.display_handle.backend_handle().kill_client(client.id(), DisconnectReason::ConnectionClosed);
        }
        let windows: Vec<Window> = agent.space.elements().cloned().collect();
        for w in &windows {
            agent.space.unmap_elem(w);
        }
        agent.space.unmap_output(&agent.output);
        if let Some(k) = agent.seat.get_keyboard() {
            k.set_focus(self, None, SERIAL_COUNTER.next_serial());
        }
        agent.seat.remove_keyboard();
        agent.seat.remove_pointer();
        if let Some(g) = agent.seat.global() {
            self.display_handle.remove_global::<Ede>(g);
        }
        if let Some(g) = agent.output_global.take() {
            self.display_handle.remove_global::<Ede>(g);
        }
        agent.stop_processes();
        let session = agent.session_id.clone();
        drop(agent);
        tracing::info!("space {id} closed");
        self.emit(events::SPACE_DESTROYED, serde_json::json!({ "space": id, "sessionId": session }));
        true
    }

    /// Serves `stream` as a client of space `id` (its socket, or a test's
    /// socketpair).
    pub fn insert_space_client(&mut self, id: &SpaceId, stream: UnixStream) -> Option<Client> {
        self.spaces.get(id)?;
        let data = ClientState { space: Some(id.clone()), ..Default::default() };
        match self.display_handle.insert_client(stream, Arc::new(data)) {
            Ok(client) => {
                self.spaces.get_mut(id)?.clients.push(client.clone());
                Some(client)
            }
            Err(e) => {
                tracing::warn!("space {id}: client refused: {e}");
                None
            }
        }
    }

    /// Starts `argv` in space `id`; replies its pid.
    pub fn space_launch(&mut self, id: &SpaceId, argv: &[String], env: &[(String, String)]) -> Result<u32, Failure> {
        let Some((program, args)) = argv.split_first() else { return Err(fail(ErrorCode::Invalid, "argv is empty")) };
        let agent = self.spaces.get_mut(id).ok_or_else(|| fail(ErrorCode::NotFound, "no such space"))?;
        agent.launches += 1;
        let mut cmd = match agent.isolation {
            Isolation::Systemd => {
                let mut c = Command::new("systemd-run");
                let slice = agent.slice();
                let unit = format!("{}-{}", slice.trim_end_matches(".slice"), agent.launches);
                c.args(["--user", "--scope", "--quiet", "--collect", "-p", "TimeoutStopSec=5s"])
                    .arg(format!("--unit={unit}"))
                    .arg(format!("--slice={slice}"))
                    .arg("--")
                    .arg(program)
                    .args(args);
                c
            }
            Isolation::ProcessGroup => {
                let mut c = Command::new(program);
                c.args(args);
                c
            }
        };
        // Never the person's displays: only the space's socket.
        cmd.env_remove("DISPLAY")
            .env_remove("WAYLAND_SOCKET")
            .env("WAYLAND_DISPLAY", &agent.socket)
            .env("XDG_SESSION_TYPE", "wayland")
            .env("GDK_BACKEND", "wayland")
            .env("QT_QPA_PLATFORM", "wayland")
            .env("SDL_VIDEODRIVER", "wayland")
            .env("MOZ_ENABLE_WAYLAND", "1")
            .env("EDE_SPACE", agent.id.as_str())
            .envs(env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::null())
            .process_group(0);
        let child = cmd.spawn().map_err(|e| fail(ErrorCode::Invalid, &format!("{program}: {e}")))?;
        let pid = child.id();
        agent.children.push(child);
        Ok(pid)
    }

    /// The space a window's client belongs to.
    pub fn window_client_space(&self, window: &Window) -> Option<SpaceId> {
        let surface = window.wl_surface()?;
        self.surface_space(&surface)
    }

    pub fn surface_space(&self, surface: &WlSurface) -> Option<SpaceId> {
        let client = self.display_handle.get_client(surface.id()).ok()?;
        client_space(&client)
    }

    /// The agent space a window is mapped in (`None` = the desktop or
    /// unmapped). #NEW-113 lends a window by moving it between the two.
    pub fn agent_space_of(&self, window: &Window) -> Option<SpaceId> {
        self.spaces.list.iter().find(|a| a.space.elements().any(|w| w == window)).map(|a| a.id.clone())
    }

    /// A space client's first buffer: cascade it in the space and give it
    /// the space seat's focus.
    pub fn place_in_space(&mut self, id: &SpaceId, window: Window) {
        let Some(agent) = self.spaces.get_mut(id) else { return };
        let area = Rectangle::from_size(agent.size.into());
        let taken: Vec<_> = agent.space.elements().filter_map(|w| agent.space.element_location(w)).collect();
        let loc = layout::place_new(window.geometry().size, area, &taken, 32);
        agent.space.map_element(window.clone(), loc, true);
        self.window_event(events::WINDOW_OPENED, &window);
        self.space_focus(id, Some(window));
    }

    /// Maximize and fullscreen in a space both fill its output.
    pub fn space_fill(&mut self, window: &Window, fullscreen: bool) {
        let Some(id) = self.agent_space_of(window) else { return };
        let Some(agent) = self.spaces.get_mut(&id) else { return };
        let rect = Rectangle::from_size(agent.size.into());
        agent.space.map_element(window.clone(), (0, 0), true);
        self.configure_window(window, Some(rect), Some(crate::layout::Tile::Maximized).filter(|_| !fullscreen), fullscreen);
    }

    /// A window left: out of its space, focus to the one now on top.
    pub fn space_window_destroyed(&mut self, surface: &WlSurface) {
        for i in 0..self.spaces.list.len() {
            let agent = &self.spaces.list[i];
            let Some(window) = agent.space.elements().find(|w| w.wl_surface().is_some_and(|s| *s == *surface)).cloned() else { continue };
            let id = agent.id.clone();
            self.window_event(events::WINDOW_CLOSED, &window);
            let agent = &mut self.spaces.list[i];
            agent.space.unmap_elem(&window);
            let next = agent.space.elements().last().cloned();
            self.space_focus(&id, next);
            return;
        }
    }

    /// Raises `window` in its space and gives it the space seat's keyboard.
    pub fn space_focus(&mut self, id: &SpaceId, window: Option<Window>) {
        let Some(agent) = self.spaces.get_mut(id) else { return };
        if let Some(w) = &window {
            agent.space.raise_element(w, true);
        } else {
            for w in agent.space.elements() {
                w.set_activated(false);
            }
        }
        for w in agent.space.elements() {
            if let Some(t) = w.toplevel() {
                t.send_pending_configure();
            }
        }
        let keyboard = agent.seat.get_keyboard().unwrap();
        let surface = window.as_ref().and_then(|w| w.wl_surface()).map(|s| s.into_owned());
        keyboard.set_focus(self, surface, SERIAL_COUNTER.next_serial());
    }

    /// One tick of a space's frame clock.
    fn space_frame(&mut self, id: &SpaceId) {
        let now = self.start_time.elapsed();
        let Some(agent) = self.spaces.get_mut(id) else { return };
        let output = agent.output.clone();
        for w in agent.space.elements() {
            w.send_frame(&output, now, Some(Duration::ZERO), |_, _| Some(output.clone()));
        }
        agent.space.refresh();
        let _ = self.display_handle.flush_clients();
    }

    /// Input on a space's own seat, never the person's. Pointer positions
    /// are space-local; protected surfaces refuse it as on the desktop.
    pub fn space_input(&mut self, id: &SpaceId, event: InputEvent, time: u32) -> Result<(), Refused> {
        let Some((seat, (w, h))) = self.spaces.get(id).map(|a| (a.seat.clone(), a.size)) else { return Ok(()) };
        let under = |ede: &Ede, pos| ede.spaces.get(id).and_then(|a| a.surface_under(pos));
        let pointer = seat.get_pointer().unwrap();
        let keyboard = seat.get_keyboard().unwrap();
        match event {
            InputEvent::Key { key, pressed } => {
                if keyboard.current_focus().is_some_and(|s| self.is_protected_surface(&s)) {
                    return Err(Refused::Protected);
                }
                let state = if pressed { KeyState::Pressed } else { KeyState::Released };
                keyboard.input::<(), _>(self, (key + 8).into(), state, SERIAL_COUNTER.next_serial(), time, |_, _, _| FilterResult::Forward);
            }
            InputEvent::PointerMotion { x, y } => {
                let pos = Point::from((x.clamp(0.0, w as f64 - 1.0), y.clamp(0.0, h as f64 - 1.0)));
                let target = under(self, pos);
                if target.as_ref().is_some_and(|(s, _)| self.is_protected_surface(s)) {
                    return Err(Refused::Protected);
                }
                pointer.motion(self, target, &MotionEvent { location: pos, serial: SERIAL_COUNTER.next_serial(), time });
                pointer.frame(self);
            }
            InputEvent::PointerButton { button, pressed } => {
                let pos = pointer.current_location();
                if under(self, pos).is_some_and(|(s, _)| self.is_protected_surface(&s)) {
                    return Err(Refused::Protected);
                }
                let serial = SERIAL_COUNTER.next_serial();
                if pressed && !pointer.is_grabbed() {
                    let window = self.spaces.get(id).and_then(|a| a.space.element_under(pos).map(|(w, _)| w.clone()));
                    self.space_focus(id, window);
                }
                let state = if pressed { ButtonState::Pressed } else { ButtonState::Released };
                pointer.button(self, &ButtonEvent { button, state, serial, time });
                pointer.frame(self);
            }
        }
        Ok(())
    }

    pub fn space_info(&self, agent: &AgentSpace) -> SpaceInfo {
        SpaceInfo {
            id: agent.id.clone(),
            session_id: agent.session_id.clone(),
            width: agent.size.0,
            height: agent.size.1,
            fps: agent.fps,
            wayland_display: agent.socket.to_string_lossy().into_owned(),
            limits: agent.limits,
            isolation: agent.isolation,
            windows: self.window_infos_in(&agent.space, agent.seat.get_keyboard().and_then(|k| k.current_focus())),
        }
    }

    /// Spaces whose session's last grant was revoked or expired close; a
    /// 1 s sweep also reaps launched processes that exited.
    pub fn close_orphan_spaces(&mut self) {
        let live = self.ipc.live_sessions();
        let mut orphans = Vec::new();
        for agent in &mut self.spaces.list {
            if live.contains(&agent.session_id) {
                agent.had_grant = true;
            } else if agent.had_grant {
                orphans.push(agent.id.clone());
            }
            agent.children.retain_mut(|c| matches!(c.try_wait(), Ok(None)));
        }
        for id in orphans {
            tracing::info!("space {id}: its session has no grant left");
            self.destroy_space(&id);
        }
    }

    /// Every space of `session` closes.
    pub fn close_session_spaces(&mut self, session: &SessionId) {
        let ids: Vec<SpaceId> = self.spaces.list.iter().filter(|a| &a.session_id == session).map(|a| a.id.clone()).collect();
        for id in ids {
            self.destroy_space(&id);
        }
    }

    fn start_space_sweep(&mut self) {
        if std::mem::replace(&mut self.spaces.sweeping, true) {
            return;
        }
        let _ = self.loop_handle.insert_source(Timer::from_duration(Duration::from_secs(1)), |_, _, state| {
            state.close_orphan_spaces();
            TimeoutAction::ToDuration(Duration::from_secs(1))
        });
    }

    /// Where space sockets go, next to the ede-protocol socket.
    pub fn set_spaces_dir(&mut self, runtime_dir: &Path, wayland_display: &str) {
        if self.spaces.list.is_empty() {
            self.spaces.dir = Some(runtime_dir.join("ede").join(format!("{wayland_display}-spaces")));
            self.spaces.owns_dir = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_global_names_its_space() {
        #[derive(Debug)]
        #[allow(dead_code)]
        struct Global {
            name: String,
            rest: u32,
        }
        let space = Global { name: "space:s3".into(), rest: 1 };
        assert_eq!(global_space(&space), Some(SpaceId::new("s3").unwrap()));
        assert_eq!(global_space(&Global { name: "seat0".into(), rest: 1 }), None);
    }
}
