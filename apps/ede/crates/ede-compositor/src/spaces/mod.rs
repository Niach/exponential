//! #EDE-125: agent spaces. A space is a second [`Desk`] beside the
//! person's: its own seat, its own virtual output, its own Wayland socket.
//! What connects to that socket belongs to the space's desk and sees that
//! seat and that output, never the person's ([`crate::desk::on_desk`]).
//!
//! Three rules. A space is new code in this module; an existing function
//! gets a guard that is `None` for the person. A tag never falls back to
//! the person: what a space tagged never enters the person's branch, also
//! after the space closed. Nothing of a space is listed, emitted or drawn
//! on the person's side.

mod close;
pub mod frames;
pub mod full;
pub mod guard;
mod input;
pub mod launch;
pub mod browsers;
pub mod render;
pub mod ipc;
mod members;
mod windows;
pub use close::{DIR_RETRIES, EXIT_BOUND};
pub mod flatpak;
pub mod readiness;
pub mod scope;
pub mod session;
pub mod slice;
#[cfg(feature = "xwayland")]
pub mod x11;

use std::{os::unix::fs::DirBuilderExt, path::PathBuf, sync::Arc};

use ede_protocol::{ErrorCode, Failure, SessionId, SpaceId};
use smithay::{
    backend::renderer::damage::OutputDamageTracker,
    desktop::Space,
    input::{keyboard::XkbConfig, pointer::CursorImageStatus, Seat},
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::{
        calloop::{generic::Generic, Interest, Mode as LoopMode, PostAction, RegistrationToken},
        wayland_server::{backend::GlobalId, protocol::wl_surface::WlSurface, Client, ListeningSocket, Resource},
    },
    utils::Transform,
    wayland::compositor::with_states,
};

use crate::{
    desk::{desk_of_client, on_desk, Desk, DeskId, DeskTag},
    state::{ClientState, Ede},
};

/// The sizes a space's output may have. The upper bound is part of the
/// design: a capture is one software pass over the whole output on the
/// thread that serves the person's frames.
pub const MIN_SIZE: (i32, i32) = (320, 240);
pub const MAX_SIZE: (i32, i32) = (1920, 1200);

pub struct Spaces {
    list: Vec<AgentSpace>,
    /// The last number given out; they count from 1 and are never reused.
    next: u32,
    /// Where the spaces' directories live
    /// (`<runtime>/exponential-desktop/<wayland-N>.spaces`); `None` = no
    /// space can be made. Sockets and the bus only: the runtime directory
    /// is a tmpfs the person's session shares.
    pub dir: Option<PathBuf>,
    /// Where this compositor's spaces keep their homes, on disk
    /// (`$XDG_CACHE_HOME/exponential-desktop/spaces/p<pid>`, [`Spaces::open_homes`]);
    /// `None` = in each space's directory.
    pub homes: Option<PathBuf>,
    /// The seats of closed spaces, until they can go ([`close`]).
    closing: Vec<close::Closing>,
    /// The directories of closed spaces that could not be removed yet.
    leftovers: Vec<close::Leftover>,
    /// Removes a space's directory with all in it; a test puts one in
    /// that fails.
    pub remove_dir: fn(&std::path::Path) -> std::io::Result<()>,
    /// Software, for the captures ([`frames`]; the damage check needs none).
    renderer: Option<smithay::backend::renderer::pixman::PixmanRenderer>,
    /// The captures waiting their turn and the spaces' stats ([`frames`]).
    frames: frames::Frames,
    /// What was launched into spaces and still runs ([`launch`]).
    launches: Vec<launch::Launch>,
    /// #EDE-183: processes of closed spaces outside their slices and their
    /// launches' groups, being ended ([`launch::Stray`]).
    strays: Vec<launch::Stray>,
    /// The person's user manager: `Some` = a launch runs in a systemd
    /// scope of its space's slice ([`slice`], #EDE-138). `None` (also in
    /// the headless harness) = as before, in the compositor's own cgroup.
    pub units: Option<slice::Units>,
    /// Why the session's start found no user manager (`units` is `None`):
    /// the cause of readiness's `no_user_manager` ([`readiness`]).
    pub units_why: Option<String>,
    /// #EXP-1514 (the latch, [`readiness`]): a space's session failed to
    /// start, and why; `None` again once a session start succeeds.
    pub session_failed: Option<String>,
    /// The readiness answer last logged ([`readiness`]).
    pub(crate) readiness_said: Option<Result<(), readiness::Block>>,
    /// The launches so far: each scope's name has its number.
    launch_serial: std::cell::Cell<u32>,
    /// The slices of closed spaces that are being stopped ([`close`]).
    stopping: Vec<close::Stopping>,
    /// A launch of this run went into a scope: there are slices to stop
    /// when the compositor ends.
    sliced: bool,
    /// The user managers of closed spaces, until they are reaped (the
    /// slice's stop ends them).
    ended_sessions: Vec<std::process::Child>,
    /// Flatpaks being brought back into their spaces ([`flatpak`]).
    adoptions: flatpak::Adoptions,
    /// X11 apps run in spaces, each space with an Xwayland of its own
    /// ([`x11`], #EDE-138): set at the session's start when X11 apps run
    /// at all (`xwayland` in `compositor.toml`, no `--no-xwayland`). Off in
    /// the headless harness unless a test sets it.
    pub x11: bool,
    /// The window managers of closed spaces, until their connection is gone.
    #[cfg(feature = "xwayland")]
    closing_xwms: Vec<smithay::xwayland::X11Wm>,
    /// How many 1 s ticks in a row a space outlives its owner's last
    /// space grant ([`close`]).
    pub expiry_grace: u32,
    /// #EXP-1514: until when (unix ms) no space expires: one grant's life
    /// after the wall clock jumped ahead of the ticks (a suspend), so its
    /// keeper can renew on a machine that is just waking up ([`close`]).
    pub grace_until: Option<i64>,
    /// The last tick's clocks, to see a jump.
    pub clock: Option<(std::time::Instant, i64)>,
    /// #EXP-1514 (D 10): how long a use, and a `space_busy`, keep a space
    /// from giving way to a new one ([`full`]).
    pub in_use_for: std::time::Duration,
    pub busy_for: std::time::Duration,
    /// #EDE-146..148: the space guard, as this session decided ([`guard`]).
    pub guard: guard::GuardState,
    /// #EDE-143: the person's session's cgroup (`/proc/<pid>/cgroup`'s
    /// text) for `space_of_process`; `None` = the compositor's own. A test
    /// names another to see every process as outside the session.
    pub session_cgroup: Option<String>,
}

impl Default for Spaces {
    fn default() -> Self {
        Self { list: Vec::new(), next: 0, dir: None, homes: None, closing: Vec::new(), leftovers: Vec::new(), remove_dir: close::remove_tree, renderer: None, frames: Default::default(), launches: Vec::new(), strays: Vec::new(), units: None, units_why: None, session_failed: None, readiness_said: None, launch_serial: std::cell::Cell::new(0), stopping: Vec::new(), sliced: false, ended_sessions: Vec::new(), adoptions: Default::default(), x11: false, #[cfg(feature = "xwayland")] closing_xwms: Vec::new(), expiry_grace: close::EXPIRY_GRACE, grace_until: None, clock: None, in_use_for: full::IN_USE_FOR, busy_for: full::BUSY_FOR, guard: guard::GuardState::default(), session_cgroup: None }
    }
}

pub(crate) fn size_ok(width: i32, height: i32) -> bool {
    (MIN_SIZE.0..=MAX_SIZE.0).contains(&width) && (MIN_SIZE.1..=MAX_SIZE.1).contains(&height)
}

pub(crate) fn size_refused() -> Failure {
    Failure::new(ErrorCode::Invalid, &format!("a space is {}x{} to {}x{}", MIN_SIZE.0, MIN_SIZE.1, MAX_SIZE.0, MAX_SIZE.1))
}

pub struct AgentSpace {
    /// `space-n`; its desk's id is number n.
    pub id: SpaceId,
    pub desk: Desk,
    /// The session whose grant made it.
    pub owner: SessionId,
    /// Its virtual output `space-n`, at (0, 0) of its desk's own space and
    /// in nothing of the person's: no layout, no pointer clamp, no list.
    pub output: Output,
    pub(crate) output_global: GlobalId,
    /// Its own directory (0700), holding its socket `wayland-0`.
    pub(crate) dir: PathBuf,
    pub(crate) listener: RegistrationToken,
    /// What connected to its socket and may still live.
    pub(crate) clients: Vec<Client>,
    /// Its frame clock ([`frames`]): whether a tick is due, how long the
    /// last one was armed for, how many ran.
    pub(crate) tracker: OutputDamageTracker,
    pub scheduled: bool,
    pub pace: std::time::Duration,
    pub ticks: u64,
    /// Counts the ticks and captures that found its scene changed.
    pub serial: u64,
    /// When it was last driven or captured.
    pub(crate) last_use: Option<std::time::Instant>,
    /// When it was made: a space never used counts from here ([`full`]).
    pub(crate) created: std::time::Instant,
    /// Until when its keeper said its session's turn runs ([`full`]).
    pub(crate) busy_until: Option<std::time::Instant>,
    /// The ticks in a row that found its owner without a live space grant.
    pub(crate) expired_ticks: u32,
    /// A launch of it went into a scope: its slice is stopped at the close.
    pub(crate) sliced: bool,
    /// Its own user manager and bus ([`session`]), from its first launch.
    pub(crate) session: session::Session,
    /// Its programs draw in software ([`render`]): Vulkan sees lavapipe
    /// alone. On for every space; #EDE-162 (F7) turns it off for a space
    /// whose GPU path is ready, before its first launch.
    pub software_rendering: bool,
    /// Its browsers' profiles inside snaps' and Flatpaks' own data
    /// ([`browsers`]), removed at its close with its directories.
    pub(crate) browser_dirs: Vec<PathBuf>,
    /// Its own Xwayland ([`x11`]), from its first launch.
    #[cfg(feature = "xwayland")]
    pub(crate) x11: Option<x11::SpaceX11>,
}

impl AgentSpace {
    /// Its Xwayland's Wayland client and display number, while it runs.
    #[cfg(feature = "xwayland")]
    pub fn x11(&self) -> Option<(&Client, u32)> {
        self.x11.as_ref().map(|x11| (x11.client(), x11.display))
    }
}

impl AgentSpace {
    /// The absolute path its clients connect to (`WAYLAND_DISPLAY`).
    pub fn socket(&self) -> PathBuf {
        self.dir.join("wayland-0")
    }
}

impl Spaces {
    /// #EDE-143: no agent space exists: none open, no closed one's slice
    /// still stopping, no launch and no stray of a closed one alive.
    pub fn none_alive(&self) -> bool {
        self.list.is_empty() && self.stopping.is_empty() && self.launches.is_empty() && self.strays.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &AgentSpace> {
        self.list.iter()
    }

    pub fn get(&self, desk: DeskId) -> Option<&AgentSpace> {
        self.list.iter().find(|s| s.desk.id == desk)
    }

    pub(crate) fn get_mut(&mut self, desk: DeskId) -> Option<&mut AgentSpace> {
        self.list.iter_mut().find(|s| s.desk.id == desk)
    }

    /// Space `space`'s home: its profile (`XDG_CONFIG_HOME` and the other
    /// three), what a browser, pip or Playwright keeps. On disk, beside
    /// the other spaces' of this compositor (#EDE-138): in the runtime
    /// directory it would be memory nobody sees and a full tmpfs the
    /// person's apps share. Removed when the space closes.
    pub(crate) fn home_of(&self, space: &AgentSpace) -> PathBuf {
        match &self.homes {
            Some(homes) => homes.join(space.id.as_str()),
            None => space.dir.join("home"),
        }
    }
}

/// Copies the desk of a new surface's client onto the surface: a dead
/// client leaves the store before its surfaces' destructors run, so a
/// lookup by client would come too late.
pub(crate) fn tag_surface(state: &Ede, surface: &WlSurface) {
    let desk = state.display_handle.get_client(surface.id()).ok().and_then(|c| desk_of_client(&c));
    if let Some(desk) = desk {
        with_states(surface, |states| states.data_map.insert_if_missing_threadsafe(|| DeskTag(desk)));
    }
}

/// The space a surface was made in, whether that space still lives or
/// not; `None` for the person's (and for nobody's).
pub fn space_of_surface(surface: &WlSurface) -> Option<DeskId> {
    with_states(surface, |states| states.data_map.get::<DeskTag>().map(|tag| tag.0)).filter(|id| *id != DeskId::PERSON)
}

/// The space a seat belongs to, live or not; `None` for the person's.
pub fn space_of_seat(seat: &Seat<Ede>) -> Option<DeskId> {
    seat.user_data().get::<DeskTag>().map(|tag| tag.0).filter(|id| *id != DeskId::PERSON)
}

impl Ede {
    /// The desk `id` names, if it is the person's or a space that lives.
    /// Everything that keeps an id across a turn of the loop asks here.
    pub fn try_desk(&self, id: DeskId) -> Option<&Desk> {
        if id == DeskId::PERSON {
            return Some(&self.desk);
        }
        self.spaces.get(id).map(|s| &s.desk)
    }

    pub fn try_desk_mut(&mut self, id: DeskId) -> Option<&mut Desk> {
        if id == DeskId::PERSON {
            return Some(&mut self.desk);
        }
        self.spaces.list.iter_mut().find(|s| s.desk.id == id).map(|s| &mut s.desk)
    }

    /// Remembers a client of space `id` for its close, forgetting those
    /// that left.
    fn space_client(&mut self, id: DeskId, client: Client) {
        let backend = self.display_handle.backend_handle();
        if let Some(space) = self.spaces.list.iter_mut().find(|s| s.desk.id == id) {
            space.clients.retain(|c| backend.get_client_data(c.id()).is_ok());
            space.clients.push(client);
        }
    }

    /// A new space of `width`×`height` for `owner`: its directory and
    /// socket, its desk, its output and its seat. What can fail comes
    /// first and the globals last, so a failure never has to take a global
    /// back (a removal is told to every client, the person's too).
    pub fn create_space(&mut self, owner: SessionId, width: i32, height: i32) -> Result<DeskId, Failure> {
        if !size_ok(width, height) {
            return Err(size_refused());
        }
        let unavailable = |what: &str, e: &dyn std::fmt::Debug| Failure::new(ErrorCode::Unavailable, &format!("{what}: {e:?}"));
        let Some(root) = self.spaces.dir.clone() else {
            return Err(Failure::new(ErrorCode::Unavailable, "no runtime directory for spaces"));
        };
        self.spaces.next += 1;
        let n = self.spaces.next;
        let (id, name) = (DeskId::numbered(n), format!("space-{n}"));

        let dir = root.join(&name);
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).map_err(|e| unavailable("the space's directory", &e))?;
        let listening = (|| {
            let socket = ListeningSocket::bind_absolute(dir.join("wayland-0")).map_err(|e| unavailable("the space's socket", &e))?;
            self.loop_handle
                .insert_source(Generic::new(socket, Interest::READ, LoopMode::Level), move |_, socket, state| {
                    while let Some(stream) = socket.accept()? {
                        let client = ClientState { desk: id, gpu: state.space_gpu_allowed(id), ..Default::default() };
                        match state.display_handle.insert_client(stream, Arc::new(client)) {
                            Ok(client) => state.space_client(id, client),
                            Err(e) => tracing::warn!("space client refused: {e}"),
                        }
                    }
                    Ok(PostAction::Continue)
                })
                .map_err(|e| unavailable("the space's socket", &e.error))
        })();
        let listener = match listening {
            Ok(token) => token,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&dir);
                return Err(e);
            }
        };

        let output = Output::new(
            name.clone(),
            PhysicalProperties { size: (0, 0).into(), subpixel: Subpixel::Unknown, make: "Exponential".into(), model: "Desktop (space)".into(), serial_number: "Unknown".into() },
        );
        let mode = Mode { size: (width, height).into(), refresh: 60_000 };
        output.change_current_state(Some(mode), Some(Transform::Normal), Some(Scale::Integer(1)), Some((0, 0).into()));
        output.set_preferred(mode);
        let mut space = Space::default();
        space.map_output(&output, (0, 0));

        let dh = self.display_handle.clone();
        let mut seat = self.seat_state.new_wl_seat_with_filter(&dh, format!("seat-{name}"), on_desk(id));
        seat.user_data().insert_if_missing(|| DeskTag(id));
        let kb = &self.config.keyboard;
        let xkb = XkbConfig { layout: &kb.layout, variant: &kb.variant, options: Some(kb.options.clone()).filter(|o| !o.is_empty()), ..Default::default() };
        if seat.add_keyboard(xkb, kb.repeat_delay, kb.repeat_rate).is_err() {
            seat.add_keyboard(XkbConfig::default(), kb.repeat_delay, kb.repeat_rate).expect("default xkb keymap");
        }
        seat.add_pointer();
        let output_global = output.create_global_with_filter::<Ede, _>(&dh, on_desk(id));
        let tracker = OutputDamageTracker::from_output(&output);

        let desk = Desk {
            id,
            cursor_status: CursorImageStatus::default_named(),
            virtual_keymap: None,
            #[cfg(feature = "xwayland")]
            pending_x11_focus: None,
            // No X11 in a space yet: the slot holds its own seat.
            #[cfg(feature = "xwayland")]
            x11_focus: (seat.clone(), None),
            seat,
            keysyms: Default::default(),
            hold: Default::default(),
            driven: Default::default(),
            clipboard: Default::default(),
            space,
            unmapped: Vec::new(),
            minimized: Vec::new(),
            workspaces: Default::default(),
            title_click: None,
        };
        tracing::info!("{name}: {width}x{height} for {}", owner.as_str());
        self.spaces.list.push(AgentSpace { id: SpaceId::new(&name).expect("space-n is a space id"), desk, owner, output, output_global, dir, listener, clients: Vec::new(), tracker, scheduled: false, pace: frames::SLOW, ticks: 0, serial: 0, last_use: None, created: std::time::Instant::now(), busy_until: None, expired_ticks: 0, sliced: false, session: session::Session::new(), software_rendering: true, browser_dirs: Vec::new(), #[cfg(feature = "xwayland")] x11: None });
        Ok(id)
    }
}
