//! `org.freedesktop.impl.portal.ScreenCast`: outputs and windows as
//! PipeWire video nodes. Also the screen half of a RemoteDesktop session
//! (the frontend calls SelectSources on its session handle).

use std::sync::Arc;

use zbus::{
    interface,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

use crate::{
    opt,
    pick::{self, Ask, Pick},
    session::{self, Kind, Session, StreamInfo, MONITOR, WINDOW},
    value, Ctx, PortalError, Vardict, CANCELLED, ENDED, SUCCESS,
};

/// restore_data's vendor and version.
const VENDOR: &str = "ede";
const RESTORE_VERSION: u32 = 1;

pub struct ScreenCast {
    pub ctx: Arc<Ctx>,
}

#[interface(name = "org.freedesktop.impl.portal.ScreenCast")]
impl ScreenCast {
    async fn create_session(
        &self,
        _handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        app_id: String,
        _options: Vardict,
        #[zbus(object_server)] server: &zbus::ObjectServer,
    ) -> Result<(u32, Vardict), PortalError> {
        session::open(&self.ctx, server, &session_handle, Session::new(Kind::ScreenCast, &app_id)).await?;
        Ok((SUCCESS, Vardict::new()))
    }

    async fn select_sources(&self, _handle: OwnedObjectPath, session_handle: OwnedObjectPath, _app_id: String, options: Vardict) -> Result<(u32, Vardict), PortalError> {
        let mut sessions = self.ctx.sessions.lock().unwrap();
        let s = sessions.get_mut(session_handle.as_str()).ok_or_else(|| PortalError::NotFound("no such session".into()))?;
        if s.started {
            return Err(PortalError::Failed("the session already started".into()));
        }
        s.types = opt::<u32>(&options, "types").unwrap_or(MONITOR) & (MONITOR | WINDOW);
        s.multiple = opt::<bool>(&options, "multiple").unwrap_or(false);
        s.persist_mode = opt::<u32>(&options, "persist_mode").unwrap_or(0).min(2);
        s.restore = options.get("restore_data").map(restore_picks).unwrap_or_default();
        Ok((SUCCESS, Vardict::new()))
    }

    async fn start(
        &self,
        _handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        _app_id: String,
        _parent_window: String,
        _options: Vardict,
    ) -> Result<(u32, Vardict), PortalError> {
        let ctx = self.ctx.clone();
        let path = session_handle.to_string();
        let started = blocking(move || start(&ctx, &path, false)).await;
        match started {
            Ok(Some(results)) => Ok((SUCCESS, results)),
            Ok(None) => Ok((CANCELLED, Vardict::new())),
            Err(e) => {
                tracing::warn!("screencast: {e}");
                Ok((ENDED, Vardict::new()))
            }
        }
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn available_source_types(&self) -> u32 {
        MONITOR | WINDOW
    }

    /// Captures carry the cursor (embedded).
    #[zbus(property(emits_changed_signal = "const"))]
    fn available_cursor_modes(&self) -> u32 {
        2
    }

    #[zbus(property(emits_changed_signal = "const"), name = "version")]
    fn version(&self) -> u32 {
        5
    }
}

/// Runs blocking work (ede-protocol calls, the chooser) off the D-Bus
/// executor.
pub async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    blocking::unblock(f).await
}

/// Picks (restored or chosen), starts a stream per pick and answers the
/// Start results. `Ok(None)` = the person cancelled (or, for remote
/// control, nobody allowed it).
pub fn start(ctx: &Arc<Ctx>, path: &str, remote_control: bool) -> Result<Option<Vardict>, String> {
    let s = ctx.sessions.lock().unwrap().get(path).cloned().ok_or("no such session")?;
    if s.started {
        return Err("the session already started".into());
    }
    let types = if s.kind == Kind::ScreenCast && s.types == 0 { MONITOR } else { s.types };
    let outputs = ctx.ede(|c| c.outputs()).map_err(|e| e.to_string())?;
    let windows = ctx.ede(|c| c.windows()).map_err(|e| e.to_string())?;
    let config = ctx.config.lock().unwrap().clone();
    let allowed = config.remote_desktop.allow.iter().any(|a| a == &s.app_id);
    let restored: Vec<Pick> = s.restore.iter().filter(|p| p.geometry(&outputs, &windows).is_some()).cloned().collect();

    let picks = if !restored.is_empty() && (!remote_control || allowed) {
        restored
    } else if remote_control && config.screencast.chooser.is_empty() {
        if !allowed {
            tracing::warn!("remote control for {:?} refused: set [screencast] chooser or [remote_desktop] allow", s.app_id);
            return Ok(None);
        }
        let ask = Ask { app_id: &s.app_id, types, multiple: s.multiple, remote_control };
        pick::choose(&[], &ask, &outputs, &windows)?.map(|c| c.picks).unwrap_or_default()
    } else {
        let ask = Ask { app_id: &s.app_id, types, multiple: s.multiple, remote_control };
        match pick::choose(&config.screencast.chooser, &ask, &outputs, &windows)? {
            Some(chosen) if !remote_control || chosen.allow || allowed => chosen.picks,
            _ => return Ok(None),
        }
    };

    let mut streams = Vec::new();
    for p in &picks {
        let (position, size) = p.geometry(&outputs, &windows).ok_or("the pick went away")?;
        let (key, node_id) = ctx.create_stream(p.target(), (size.0.max(1) as u32, size.1.max(1) as u32))?;
        streams.push(StreamInfo { key, node_id, pick: p.clone(), target: p.target(), position, size });
    }
    let mut results = Vardict::new();
    let list: Vec<(u32, Vardict)> = streams
        .iter()
        .map(|st| {
            let props: Vardict = [
                ("position".to_string(), value(st.position)),
                ("size".to_string(), value(st.size)),
                ("source_type".to_string(), value(st.pick.source_type())),
                ("id".to_string(), value(st.pick.encode())),
            ]
            .into();
            (st.node_id, props)
        })
        .collect();
    results.insert("streams".into(), value(list));
    if s.persist_mode > 0 && !picks.is_empty() {
        let data = picks.iter().map(Pick::encode).collect::<Vec<_>>().join("\n");
        results.insert("persist_mode".into(), value(s.persist_mode));
        results.insert("restore_data".into(), value((VENDOR, RESTORE_VERSION, Value::new(data))));
    }
    {
        let mut sessions = ctx.sessions.lock().unwrap();
        let Some(live) = sessions.get_mut(path) else {
            // Closed while the chooser was up.
            drop(sessions);
            if let Some(Ok(pw)) = ctx_pw(ctx) {
                for st in &streams {
                    pw.destroy(st.key);
                }
            }
            return Ok(None);
        };
        live.started = true;
        live.streams = streams;
    }
    if !picks.is_empty() {
        let who = if s.app_id.is_empty() { "An app".to_string() } else { s.app_id.clone() };
        let what = if remote_control { "is sharing and controlling your screen" } else { "is sharing your screen" };
        ctx.notify("Screen sharing", &format!("{who} {what}"));
    }
    Ok(Some(results))
}

fn ctx_pw(ctx: &Ctx) -> Option<Result<&crate::stream::Pw, &String>> {
    ctx.pw_get().map(|r| r.as_ref())
}

/// The picks of our own `(suv)` restore data; anything else = none.
fn restore_picks(v: &OwnedValue) -> Vec<Pick> {
    let Ok(v) = v.try_clone() else { return vec![] };
    let Ok((vendor, version, data)) = <(String, u32, OwnedValue)>::try_from(Value::from(v)) else { return vec![] };
    if vendor != VENDOR || version != RESTORE_VERSION {
        return vec![];
    }
    let mut data = Value::from(data);
    while let Value::Value(inner) = data {
        data = *inner;
    }
    match data {
        Value::Str(s) => s.as_str().lines().filter_map(Pick::decode).collect(),
        _ => vec![],
    }
}
