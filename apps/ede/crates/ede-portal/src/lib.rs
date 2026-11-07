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
//! token ([`minted_token`]). A stream polls captures with `since`, so an
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
    pw: OnceLock<Result<stream::Pw, String>>,
    conn: OnceLock<zbus::Connection>,
    next_stream: Mutex<u64>,
}

impl Ctx {
    pub fn new(connect: Connect, config_path: Option<PathBuf>) -> Arc<Ctx> {
        let config = Config::load(config_path.as_deref());
        Arc::new(Ctx {
            connect,
            ede: Mutex::new(None),
            config: Mutex::new(config),
            config_path,
            sessions: Mutex::default(),
            pw: OnceLock::new(),
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

    pub(crate) fn pw_get(&self) -> Option<&Result<stream::Pw, String>> {
        self.pw.get()
    }

    pub fn connection(&self) -> Option<&zbus::Connection> {
        self.conn.get()
    }

    fn pw(self: &Arc<Self>) -> Result<&stream::Pw, String> {
        let ctx = Arc::downgrade(self);
        self.pw
            .get_or_init(|| {
                let on_end = Arc::new(move |key: u64| {
                    if let Some(ctx) = ctx.upgrade() {
                        ctx.stream_ended(key);
                    }
                });
                stream::Pw::start(self.connect.clone(), on_end)
            })
            .as_ref()
            .map_err(Clone::clone)
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

    /// Ends `path`'s streams and shortcuts and forgets it.
    pub fn close_session(&self, path: &str) {
        let Some(session) = self.sessions.lock().unwrap().remove(path) else { return };
        if let Some(Ok(pw)) = self.pw.get() {
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
