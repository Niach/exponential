//! `org.freedesktop.impl.portal.GlobalShortcuts`: bindings live in the
//! compositor (`bind_shortcuts`, owner = the session handle) and come back
//! as `shortcut.activated` / `shortcut.deactivated` events.

use std::{collections::HashMap, sync::Arc};

use ede_protocol::{Envelope, ShortcutEvent, ShortcutSpec};
use zbus::{
    interface,
    zvariant::{ObjectPath, OwnedObjectPath, Value},
};

use crate::{
    opt_str,
    session::{self, Kind, Session},
    value, Ctx, PortalError, Vardict, PATH, SUCCESS,
};

const IFACE: &str = "org.freedesktop.impl.portal.GlobalShortcuts";

pub struct GlobalShortcuts {
    pub ctx: Arc<Ctx>,
}

/// The session's BOUND shortcuts, as the portal answers them (#EXP-1938: a
/// shortcut whose trigger the compositor refused has none, and is no row).
fn rows(s: &Session) -> Vec<(String, Vardict)> {
    s.shortcuts
        .iter()
        .filter_map(|(id, description, trigger)| {
            let trigger = trigger.as_deref()?;
            let props: Vardict = [("description".to_string(), value(description.as_str())), ("trigger_description".to_string(), value(trigger))].into();
            Some((id.clone(), props))
        })
        .collect()
}

#[interface(name = "org.freedesktop.impl.portal.GlobalShortcuts")]
impl GlobalShortcuts {
    async fn create_session(
        &self,
        _handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        app_id: String,
        _options: Vardict,
        #[zbus(object_server)] server: &zbus::ObjectServer,
    ) -> Result<(u32, Vardict), PortalError> {
        session::open(&self.ctx, server, &session_handle, Session::new(Kind::GlobalShortcuts, &app_id)).await?;
        Ok((SUCCESS, Vardict::new()))
    }

    async fn bind_shortcuts(
        &self,
        _handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        shortcuts: Vec<(String, Vardict)>,
        _parent_window: String,
        _options: Vardict,
    ) -> Result<(u32, Vardict), PortalError> {
        let owner = session_handle.to_string();
        if !self.ctx.sessions.lock().unwrap().get(&owner).is_some_and(|s| s.kind == Kind::GlobalShortcuts) {
            return Err(PortalError::NotFound("no such session".into()));
        }
        let specs: Vec<ShortcutSpec> =
            shortcuts.iter().map(|(id, props)| ShortcutSpec { id: id.clone(), trigger: opt_str(props, "preferred_trigger").unwrap_or_default() }).collect();
        let bound = self.ctx.ede(|c| c.bind_shortcuts(&owner, specs.clone()))?;
        let mut sessions = self.ctx.sessions.lock().unwrap();
        let s = sessions.get_mut(&owner).ok_or_else(|| PortalError::NotFound("no such session".into()))?;
        s.shortcuts = shortcuts
            .iter()
            .map(|(id, props)| {
                let trigger = bound.iter().find(|b| &b.id == id).and_then(|b| b.trigger.clone());
                (id.clone(), opt_str(props, "description").unwrap_or_default(), trigger)
            })
            .collect();
        Ok((SUCCESS, [("shortcuts".to_string(), value(rows(s)))].into()))
    }

    async fn list_shortcuts(&self, _handle: OwnedObjectPath, session_handle: OwnedObjectPath) -> Result<(u32, Vardict), PortalError> {
        let sessions = self.ctx.sessions.lock().unwrap();
        let s = sessions.get(session_handle.as_str()).ok_or_else(|| PortalError::NotFound("no such session".into()))?;
        Ok((SUCCESS, [("shortcuts".to_string(), value(rows(s)))].into()))
    }

    #[zbus(property(emits_changed_signal = "const"), name = "version")]
    fn version(&self) -> u32 {
        1
    }
}

/// A compositor shortcut event → `Activated` / `Deactivated`.
pub fn forward(ctx: &Ctx, event: &Envelope) {
    let Ok(e) = serde_json::from_value::<ShortcutEvent>(event.data.clone()) else { return };
    let Some(conn) = ctx.connection() else { return };
    let Ok(session) = ObjectPath::try_from(e.owner.as_str()) else { return };
    let member = if event.kind == ede_protocol::events::SHORTCUT_ACTIVATED { "Activated" } else { "Deactivated" };
    let options: HashMap<&str, Value> = HashMap::new();
    let _ = zbus::block_on(conn.emit_signal(None::<()>, PATH, IFACE, member, &(session, e.id.as_str(), e.time as u64, options)));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #EXP-1938: the portal answers with the shortcuts that were BOUND. A
    /// trigger the compositor refused (no modifier, taken) is no row: an
    /// app reads a row as its shortcut working.
    #[test]
    fn a_refused_trigger_is_no_row() {
        let mut session = Session::new(Kind::GlobalShortcuts, "at.exponential.app");
        session.shortcuts = vec![
            ("push-to-talk".into(), "Push to talk".into(), None),
            ("pause-listening".into(), "Pause listening".into(), Some("Ctrl+Alt+P".into())),
        ];
        let bound = rows(&session);
        assert_eq!(bound.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["pause-listening"]);
        assert_eq!(bound[0].1["trigger_description"], value("Ctrl+Alt+P"));
        assert_eq!(bound[0].1["description"], value("Pause listening"));
    }
}
