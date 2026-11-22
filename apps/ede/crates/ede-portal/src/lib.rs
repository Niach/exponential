//! `xdg-desktop-portal-exponential` (#NEW-105): eDE's xdg-desktop-portal backend.
//!
//! The frontend (`xdg-desktop-portal`) routes ScreenCast, Screenshot,
//! RemoteDesktop, GlobalShortcuts and Settings here (`exponential-portals.conf`);
//! everything else (FileChooser, …) stays with the GTK backend. Each one is
//! an `ede-protocol` call with the shell's root token: captures, synthetic
//! input (refused on protected surfaces and the lock, like every other
//! driver) and global shortcuts. ScreenCast streams are PipeWire nodes
//! ([`stream`]).
//!
//! What to share is picked by the chooser (`[screencast] chooser`, by
//! default `exponential-sealed share`, a sealed dialog, #EDE-17; without one the
//! focused output/window). Remote control is never granted silently: it
//! needs the chooser's `allow` or an app on `[remote_desktop] allow`.
//! Every connection holds a grant minted for the portal, never the root
//! token ([`minted_token`]); #EDE-172 (P7): a portal the compositor started
//! holds its grant on the channel it was handed and opens every connection
//! on it ([`on_channel`]). A stream polls captures with `since`, so an
//! unchanged screen costs the compositor no re-render; the GPU (dmabuf)
//! path for the udev backend is still to come.

pub mod config;
mod pick;
mod remote_desktop;
mod screencast;
mod screenshot;
mod session;
mod settings;
mod shortcuts;
pub mod stream;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use ede_protocol::{events, Client, Error};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub use config::Config;
pub use pick::Pick;
pub use session::{Kind, Session, StreamInfo};

pub const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.exponential";
pub const PATH: &str = "/org/freedesktop/portal/desktop";

/// Opens a new `ede-protocol` connection (each stream captures over its own).
pub type Connect = Arc<dyn Fn() -> Result<Client, Error> + Send + Sync>;

/// #EDE-172 (P7): the channel the compositor started the portal with
/// (`[session] helpers = "compositor"`), holding the portal's grant; taken
/// first thing in `main`, before the crash monitor or any thread starts.
/// `Ok(None)` = none (a portal its unit or D-Bus activation started).
pub fn take_channel() -> Result<Option<Client>, Error> {
    ede_protocol::process::keep_inherited_to_this_process();
    Client::inherited().transpose()
}

/// #EDE-172 (P7): every connection of a portal started on `channel` is a
/// channel opened on it, holding the same grant; no token is read.
pub fn on_channel(channel: Client) -> Connect {
    let channel = Mutex::new(channel);
    Arc::new(move || {
        let mut channel = channel.lock().unwrap_or_else(|p| p.into_inner());
        let held = channel.hello.capabilities.clone();
        channel.open_channel(None, held, None).map(|(client, _)| client)
    })
}

/// The portal's own grant (#EDE-15): minted once from the root token the
/// compositor leaves for the shell ([`Client::mint_env`], #EDE-22), holding
/// only `capture`, `input:person` (RemoteDesktop) and `ui` (global
/// shortcuts); the root connection is dropped right after.
pub fn minted_token() -> Result<ede_protocol::Token, Error> {
    Client::mint_env(
        ede_protocol::SessionId::new("ede-portal").map_err(|e| Error::Protocol(e.to_string()))?,
        vec![ede_protocol::Capability::Capture, ede_protocol::Capability::InputPerson, ede_protocol::Capability::Ui],
        None,
    )
}

pub type Vardict = HashMap<String, OwnedValue>;

/// What the portal asks of its PipeWire connection: [`stream::Pw`], or a
/// test's fake.
pub(crate) trait Pipe: Send + Sync {
    fn create(&self, key: u64, target: ede_protocol::CaptureTarget, size: (u32, u32), fps: u32) -> Result<u32, String>;
    fn destroy(&self, key: u64);
    /// False once PipeWire went away under it.
    fn alive(&self) -> bool;
}

/// Runs when a stream's source went away (or PipeWire did).
pub(crate) type OnEnd = Arc<dyn Fn(u64) + Send + Sync>;
type StartPipe = Box<dyn Fn(Connect, OnEnd) -> Result<Arc<dyn Pipe>, String> + Send + Sync>;

/// Portal responses.
pub const SUCCESS: u32 = 0;
pub const CANCELLED: u32 = 1;
pub const ENDED: u32 = 2;

/// `org.freedesktop.portal.Error.*`.
#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.portal.Error")]
pub enum PortalError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Failed(String),
    InvalidArgument(String),
    NotFound(String),
    NotAllowed(String),
}

impl From<Error> for PortalError {
    fn from(e: Error) -> Self {
        PortalError::Failed(e.to_string())
    }
}

pub type PortalResult<T> = Result<T, PortalError>;

pub fn value<'a>(v: impl Into<Value<'a>>) -> OwnedValue {
    v.into().try_to_owned().expect("no file descriptors in portal values")
}

pub fn opt<'a, T: TryFrom<&'a OwnedValue>>(options: &'a Vardict, key: &str) -> Option<T> {
    options.get(key).and_then(|v| T::try_from(v).ok())
}

/// What every portal shares.
pub struct Ctx {
    connect: Connect,
    ede: Mutex<Option<Client>>,
    pub config: Mutex<Config>,
    config_path: Option<PathBuf>,
    pub sessions: Mutex<HashMap<String, Session>>,
    /// Started on the first share, started again once PipeWire went away
    /// (#EDE-101); a failed start is never kept.
    pw: Mutex<Option<Arc<dyn Pipe>>>,
    start_pw: StartPipe,
    conn: OnceLock<zbus::Connection>,
    next_stream: Mutex<u64>,
}

impl Ctx {
    pub fn new(connect: Connect, config_path: Option<PathBuf>) -> Arc<Ctx> {
        Self::with_pipe(connect, config_path, Box::new(|connect, on_end| stream::Pw::start(connect, on_end).map(|pw| Arc::new(pw) as Arc<dyn Pipe>)))
    }

    fn with_pipe(connect: Connect, config_path: Option<PathBuf>, start_pw: StartPipe) -> Arc<Ctx> {
        let config = Config::load(config_path.as_deref());
        Arc::new(Ctx {
            connect,
            ede: Mutex::new(None),
            config: Mutex::new(config),
            config_path,
            sessions: Mutex::default(),
            pw: Mutex::new(None),
            start_pw,
            conn: OnceLock::new(),
            next_stream: Mutex::new(1),
        })
    }

    /// Runs `f` on the shared connection, reconnecting once if it broke
    /// (the compositor restarted).
    pub fn ede<T>(&self, mut f: impl FnMut(&mut Client) -> Result<T, Error>) -> Result<T, Error> {
        let mut guard = self.ede.lock().unwrap();
        for attempt in 0..2 {
            if guard.is_none() {
                *guard = Some((self.connect)()?);
            }
            match f(guard.as_mut().unwrap()) {
                Err(Error::Io(_)) if attempt == 0 => *guard = None,
                other => return other,
            }
        }
        unreachable!()
    }

    pub(crate) fn pw_get(&self) -> Option<Arc<dyn Pipe>> {
        self.pw.lock().unwrap().clone()
    }

    pub fn connection(&self) -> Option<&zbus::Connection> {
        self.conn.get()
    }

    fn pw(self: &Arc<Self>) -> Result<Arc<dyn Pipe>, String> {
        let mut slot = self.pw.lock().unwrap();
        if let Some(pw) = slot.as_ref().filter(|pw| pw.alive()) {
            return Ok(pw.clone());
        }
        let ctx = Arc::downgrade(self);
        let on_end: OnEnd = Arc::new(move |key: u64| {
            if let Some(ctx) = ctx.upgrade() {
                ctx.stream_ended(key);
            }
        });
        let pw = (self.start_pw)(self.connect.clone(), on_end)?;
        *slot = Some(pw.clone());
        Ok(pw)
    }

    /// A new PipeWire node capturing `target`.
    pub fn create_stream(self: &Arc<Self>, target: ede_protocol::CaptureTarget, size: (u32, u32)) -> Result<(u64, u32), String> {
        let key = {
            let mut n = self.next_stream.lock().unwrap();
            *n += 1;
            *n
        };
        let fps = self.config.lock().unwrap().screencast.framerate;
        let node = self.pw()?.create(key, target, size, fps)?;
        Ok((key, node))
    }

    /// Ends `path`'s streams and shortcuts, lets go of the keys and buttons
    /// it holds down, and forgets it.
    pub fn close_session(&self, path: &str) {
        let Some(session) = self.sessions.lock().unwrap().remove(path) else { return };
        for release in session.pressed.iter().rev() {
            let _ = self.ede(|c| c.input(release.clone()));
        }
        if let Some(pw) = self.pw_get() {
            for s in &session.streams {
                pw.destroy(s.key);
            }
        }
        if session.kind == Kind::GlobalShortcuts {
            let _ = self.ede(|c| c.call(ede_protocol::Request::UnbindShortcuts { owner: path.into() }).map(|_| ()));
        }
    }

    /// A stream's source went away: the session ends, and the app hears it.
    fn stream_ended(&self, key: u64) {
        let path = self.sessions.lock().unwrap().iter().find(|(_, s)| s.streams.iter().any(|st| st.key == key)).map(|(p, _)| p.clone());
        if let Some(path) = path {
            self.close_session(&path);
            self.emit_closed(&path);
        }
    }

    fn emit_closed(&self, path: &str) {
        let Some(conn) = self.conn.get() else { return };
        let _ = zbus::block_on(conn.emit_signal(None::<()>, path, "org.freedesktop.impl.portal.Session", "Closed", &()));
        if let Ok(path) = OwnedObjectPath::try_from(path.to_string()) {
            let _ = zbus::block_on(conn.object_server().remove::<session::SessionObject, _>(path));
        }
    }

    /// Tells the person something started (screen sharing), best effort.
    pub fn notify(&self, summary: &str, body: &str) {
        let Some(conn) = self.conn.get().cloned() else { return };
        let (summary, body) = (summary.to_string(), body.to_string());
        std::thread::spawn(move || {
            let hints: HashMap<&str, Value> = HashMap::new();
            let args = ("Exponential Desktop", 0u32, "", summary.as_str(), body.as_str(), Vec::<&str>::new(), hints, 5000i32);
            let call = conn.call_method(
                Some("org.freedesktop.Notifications"),
                "/org/freedesktop/Notifications",
                Some("org.freedesktop.Notifications"),
                "Notify",
                &args,
            );
            if let Err(e) = zbus::block_on(call) {
                tracing::debug!("no notification daemon: {e}");
            }
        });
    }

    fn reload_config(&self) -> (Config, Config) {
        let new = Config::load(self.config_path.as_deref());
        let old = std::mem::replace(&mut *self.config.lock().unwrap(), new.clone());
        (old, new)
    }
}

/// Serves every portal on `builder`'s bus, takes [`BUS_NAME`], and starts
/// listening to the compositor's events (shortcuts, config reloads).
pub async fn serve(builder: zbus::connection::Builder<'_>, ctx: Arc<Ctx>) -> zbus::Result<zbus::Connection> {
    let conn = builder
        .serve_at(PATH, screenshot::Screenshot { ctx: ctx.clone() })?
        .serve_at(PATH, screencast::ScreenCast { ctx: ctx.clone() })?
        .serve_at(PATH, remote_desktop::RemoteDesktop { ctx: ctx.clone() })?
        .serve_at(PATH, shortcuts::GlobalShortcuts { ctx: ctx.clone() })?
        .serve_at(PATH, settings::Settings { ctx: ctx.clone() })?
        .name(BUS_NAME)?
        .build()
        .await?;
    let _ = ctx.conn.set(conn.clone());
    let events_ctx = ctx.clone();
    std::thread::Builder::new().name("ede-portal-events".into()).spawn(move || listen(events_ctx)).map_err(|e| zbus::Error::Failure(e.to_string()))?;
    Ok(conn)
}

/// The compositor's events, over a connection of their own. Reconnects
/// when the compositor restarts.
fn listen(ctx: Arc<Ctx>) {
    loop {
        let mut client = match (ctx.connect)().and_then(|mut c| c.subscribe().map(|_| c)) {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!("ede-protocol events: {e}; retrying");
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
        };
        while let Ok(event) = client.next_event() {
            match event.kind.as_str() {
                events::SHORTCUT_ACTIVATED | events::SHORTCUT_DEACTIVATED => shortcuts::forward(&ctx, &event),
                events::CONFIG_RELOADED => {
                    let (old, new) = ctx.reload_config();
                    settings::announce_changes(&ctx, &old, &new);
                }
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

pub fn opt_str(options: &Vardict, key: &str) -> Option<String> {
    options.get(key).and_then(|v| <&str>::try_from(v).ok()).map(String::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct FakePipe {
        alive: AtomicBool,
        on_end: OnEnd,
        keys: Mutex<Vec<u64>>,
    }

    impl Pipe for FakePipe {
        fn create(&self, key: u64, _: ede_protocol::CaptureTarget, _: (u32, u32), _: u32) -> Result<u32, String> {
            if !self.alive() {
                return Err("the PipeWire thread is gone".into());
            }
            self.keys.lock().unwrap().push(key);
            Ok(40 + key as u32)
        }
        fn destroy(&self, key: u64) {
            self.keys.lock().unwrap().retain(|k| *k != key);
        }
        fn alive(&self) -> bool {
            self.alive.load(Ordering::Relaxed)
        }
    }

    impl FakePipe {
        /// What stream::Pw does when its core errors out: dead, and every
        /// stream it carried ends.
        fn go_away(&self) {
            self.alive.store(false, Ordering::Relaxed);
            let keys = std::mem::take(&mut *self.keys.lock().unwrap());
            for key in keys {
                (self.on_end)(key);
            }
        }
    }

    /// #EDE-101: PipeWire not there at the first share, then there; then
    /// gone (a restart) under a live share: the share ends, and the next
    /// one connects again instead of failing until re-login.
    #[test]
    fn pipewire_is_reconnected_after_a_failed_start_and_a_restart() {
        let starts = Arc::new(AtomicUsize::new(0));
        let pipes: Arc<Mutex<Vec<Arc<FakePipe>>>> = Arc::default();
        let (n, made) = (starts.clone(), pipes.clone());
        let start: StartPipe = Box::new(move |_, on_end| {
            if n.fetch_add(1, Ordering::Relaxed) == 0 {
                return Err("PipeWire: no such file or directory".into());
            }
            let pipe = Arc::new(FakePipe { alive: AtomicBool::new(true), on_end, keys: Mutex::default() });
            made.lock().unwrap().push(pipe.clone());
            Ok(pipe)
        });
        let connect: Connect = Arc::new(|| Err(Error::Io(std::io::ErrorKind::NotFound.into())));
        let ctx = Ctx::with_pipe(connect, None, start);
        let target = || ede_protocol::CaptureTarget::Output { name: "HEADLESS-1".into() };

        assert!(ctx.create_stream(target(), (800, 600)).is_err(), "no PipeWire yet");
        let (key, node_id) = ctx.create_stream(target(), (800, 600)).expect("PipeWire is up now: the failed start is not cached");
        let mut session = Session::new(Kind::ScreenCast, "org.example.Meet");
        session.started = true;
        let pick = Pick::Output("HEADLESS-1".into());
        session.streams.push(StreamInfo { key, node_id, target: pick.target(), pick, position: (0, 0), size: (800, 600) });
        let path = "/org/freedesktop/portal/desktop/session/1_1/sc";
        ctx.sessions.lock().unwrap().insert(path.into(), session);

        pipes.lock().unwrap()[0].go_away();
        assert!(ctx.sessions.lock().unwrap().is_empty(), "the share on the dead PipeWire ended");
        ctx.create_stream(target(), (800, 600)).expect("a new share connects to the new PipeWire");
        assert_eq!(starts.load(Ordering::Relaxed), 3);
        assert!(pipes.lock().unwrap()[1].alive());
    }
}
