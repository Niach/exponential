//! `org.freedesktop.impl.portal.ScreenCast`: outputs and windows as
//! PipeWire video nodes. Also the screen half of a RemoteDesktop session
//! (the frontend calls SelectSources on its session handle).

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

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

/// How long a chooser's pick stays a handoff ([`Ctx::handoffs`]): Discord
/// shows its own "Go Live" step (quality, sound) between the two sessions.
pub const HANDOFF: Duration = Duration::from_secs(300);

/// A pick the person just made in the chooser for `app_id`, for a session
/// that persists only while the app runs: the app's next session restoring
/// exactly these picks shares them once, without the chooser again (one
/// picker per share, as on GNOME and Cinnamon). Restoring anything else, a
/// persistent token (`persist_mode` 2), a later restore or remote control
/// still goes through the chooser (#EDE-172, D6 a).
#[derive(Clone, Debug)]
pub struct Handoff {
    pub app_id: String,
    pub picks: Vec<Pick>,
    pub at: Instant,
}

/// Takes `s`'s handoff: its restored picks are the ones the person picked
/// for the same app less than [`HANDOFF`] ago (used up either way).
fn take_handoff(ctx: &Ctx, s: &Session, now: Instant) -> Option<Vec<Pick>> {
    let mut handoffs = ctx.handoffs.lock().unwrap();
    handoffs.retain(|h| now.saturating_duration_since(h.at) < HANDOFF);
    if s.kind != Kind::ScreenCast || s.restore.is_empty() {
        return None;
    }
    let i = handoffs.iter().position(|h| h.app_id == s.app_id && h.picks == s.restore)?;
    Some(handoffs.remove(i).picks)
}

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

/// Picks (the chooser's, or without one the focused output/window),
/// starts a stream per pick and answers the Start results. `Ok(None)` =
/// the person cancelled (or, for remote control, nobody allowed it).
/// #EDE-172 (D6 a): restore data only preselects the chooser's rows; a
/// restored share shows the chooser like a new one, so a restore token
/// alone (any process of the person can present a host app's) never
/// shares.
pub fn start(ctx: &Arc<Ctx>, path: &str, remote_control: bool) -> Result<Option<Vardict>, String> {
    let s = ctx.sessions.lock().unwrap().get(path).cloned().ok_or("no such session")?;
    if s.started {
        return Err("the session already started".into());
    }
    // #EDE-101: remote control only through RemoteDesktop.Start, which
    // asks for `allow`; a share never starts any other kind of session.
    if s.kind == Kind::GlobalShortcuts || (s.kind == Kind::RemoteDesktop) != remote_control {
        return Err(format!("a {:?} session does not start here", s.kind));
    }
    let types = if s.kind == Kind::ScreenCast && s.types == 0 { MONITOR } else { s.types };
    let outputs = ctx.ede(|c| c.outputs()).map_err(|e| e.to_string())?;
    let windows = ctx.ede(|c| c.windows()).map_err(|e| e.to_string())?;
    let config = ctx.config.lock().unwrap().clone();
    let allowed = config.remote_desktop.allow.iter().any(|a| a == &s.app_id);
    let ask = Ask { app_id: &s.app_id, types, multiple: s.multiple, remote_control, selected: &s.restore };

    let handoff = if remote_control { None } else { take_handoff(ctx, &s, Instant::now()) };
    // Still there, of the types asked for, never a protected window.
    let handoff = handoff.filter(|picks| picks.iter().all(|p| p.source_type() & types != 0 && p.geometry(&outputs, &windows).is_some()));
    let from_chooser = handoff.is_none() && !config.screencast.chooser.is_empty();
    let picks = if let Some(picks) = handoff {
        tracing::info!(app = %s.app_id, "screencast: the pick the person just made, handed to the app's capture session");
        picks
    } else if remote_control && config.screencast.chooser.is_empty() {
        if !allowed {
            tracing::warn!("remote control for {:?} refused: set [screencast] chooser or [remote_desktop] allow", s.app_id);
            return Ok(None);
        }
        pick::choose(&[], &ask, &outputs, &windows)?.map(|c| c.picks).unwrap_or_default()
    } else {
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
    if from_chooser && !remote_control && s.kind == Kind::ScreenCast && s.persist_mode == 1 && !picks.is_empty() {
        ctx.handoffs.lock().unwrap().push(Handoff { app_id: s.app_id.clone(), picks: picks.clone(), at: Instant::now() });
    }
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
            if let Some(pw) = ctx.pw_get() {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Arc<Ctx> {
        Ctx::new(Arc::new(|| Err(ede_protocol::Error::Io(std::io::ErrorKind::NotFound.into()))), None)
    }

    fn restoring(app_id: &str, picks: &[Pick]) -> Session {
        let mut s = Session::new(Kind::ScreenCast, app_id);
        s.restore = picks.to_vec();
        s
    }

    /// Chromium's pick-then-capture: the restore of the pick the person
    /// just made, for the same app, shares once; another app, other picks,
    /// a second restore or an old pick go through the chooser.
    #[test]
    fn a_handoff_shares_once_for_the_same_app_and_picks() {
        let ctx = ctx();
        let screen = vec![Pick::Output("Virtual-1".into())];
        let now = Instant::now();
        let handoff = |app: &str, at| Handoff { app_id: app.into(), picks: screen.clone(), at };
        ctx.handoffs.lock().unwrap().extend([handoff("com.discordapp.Discord", now), handoff("", now - HANDOFF - Duration::from_secs(1))]);

        assert_eq!(take_handoff(&ctx, &restoring("org.example.Other", &screen), now), None, "another app");
        assert_eq!(take_handoff(&ctx, &restoring("com.discordapp.Discord", &[Pick::Output("HDMI-A-1".into())]), now), None, "other picks");
        assert_eq!(take_handoff(&ctx, &restoring("com.discordapp.Discord", &[]), now), None, "no restore");
        let mut remote = restoring("com.discordapp.Discord", &screen);
        remote.kind = Kind::RemoteDesktop;
        assert_eq!(take_handoff(&ctx, &remote, now), None, "remote control never");
        assert_eq!(take_handoff(&ctx, &restoring("com.discordapp.Discord", &screen), now), Some(screen.clone()));
        assert_eq!(take_handoff(&ctx, &restoring("com.discordapp.Discord", &screen), now), None, "used up");
        assert_eq!(take_handoff(&ctx, &restoring("", &screen), now), None, "older than HANDOFF");
        assert!(ctx.handoffs.lock().unwrap().is_empty());
    }
}
