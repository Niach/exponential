//! Portal sessions (ScreenCast, RemoteDesktop, GlobalShortcuts): the state
//! behind a session handle, and the `org.freedesktop.impl.portal.Session`
//! object the frontend closes.

use std::sync::Arc;

use ede_protocol::CaptureTarget;
use zbus::{interface, zvariant::OwnedObjectPath};

use crate::{pick::Pick, Ctx};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    ScreenCast,
    RemoteDesktop,
    GlobalShortcuts,
}

/// ScreenCast source types.
pub const MONITOR: u32 = 1;
pub const WINDOW: u32 = 2;
/// RemoteDesktop device types.
pub const KEYBOARD: u32 = 1;
pub const POINTER: u32 = 2;

#[derive(Clone, Debug)]
pub struct StreamInfo {
    pub key: u64,
    pub node_id: u32,
    pub pick: Pick,
    pub target: CaptureTarget,
    pub position: (i32, i32),
    pub size: (i32, i32),
}

#[derive(Clone, Debug)]
pub struct Session {
    pub kind: Kind,
    pub app_id: String,
    /// Sources picked by SelectSources (0 = none asked for).
    pub types: u32,
    pub multiple: bool,
    pub persist_mode: u32,
    pub restore: Vec<Pick>,
    pub devices: u32,
    pub started: bool,
    pub streams: Vec<StreamInfo>,
    /// GlobalShortcuts: (id, description, trigger).
    pub shortcuts: Vec<(String, String, Option<String>)>,
}

impl Session {
    pub fn new(kind: Kind, app_id: &str) -> Session {
        Session {
            kind,
            app_id: app_id.into(),
            types: 0,
            multiple: false,
            persist_mode: 0,
            restore: Vec::new(),
            devices: 0,
            started: false,
            streams: Vec::new(),
            shortcuts: Vec::new(),
        }
    }
}

pub struct SessionObject {
    pub ctx: Arc<Ctx>,
    pub path: String,
}

#[interface(name = "org.freedesktop.impl.portal.Session")]
impl SessionObject {
    async fn close(&self, #[zbus(object_server)] server: &zbus::ObjectServer) {
        self.ctx.close_session(&self.path);
        if let Ok(path) = OwnedObjectPath::try_from(self.path.clone()) {
            let _ = server.remove::<SessionObject, _>(path).await;
        }
    }

    #[zbus(signal)]
    async fn closed(emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(property(emits_changed_signal = "const"), name = "version")]
    fn version(&self) -> u32 {
        1
    }
}

/// Registers `session` at `path` and exports its Session object.
pub async fn open(ctx: &Arc<Ctx>, server: &zbus::ObjectServer, path: &OwnedObjectPath, session: Session) -> Result<(), crate::PortalError> {
    ctx.sessions.lock().unwrap().insert(path.to_string(), session);
    server.at(path.clone(), SessionObject { ctx: ctx.clone(), path: path.to_string() }).await?;
    Ok(())
}
