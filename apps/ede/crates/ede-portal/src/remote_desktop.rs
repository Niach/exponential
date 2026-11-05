//! `org.freedesktop.impl.portal.RemoteDesktop`: keyboard and pointer over
//! `ede-protocol` `input` (refused on protected surfaces and the lock),
//! with the screens as ScreenCast streams. Never granted silently.

use std::sync::Arc;

use ede_protocol::InputEvent;
use zbus::{interface, zvariant::OwnedObjectPath};

use crate::{
    opt,
    screencast::{blocking, start},
    session::{self, Kind, Session, KEYBOARD, POINTER},
    value, Ctx, PortalError, Vardict, CANCELLED, ENDED, SUCCESS,
};

pub struct RemoteDesktop {
    pub ctx: Arc<Ctx>,
}

impl RemoteDesktop {
    fn drive(&self, session: &OwnedObjectPath, needs: u32, event: InputEvent) -> Result<(), PortalError> {
        {
            let sessions = self.ctx.sessions.lock().unwrap();
            let s = sessions.get(session.as_str()).ok_or_else(|| PortalError::NotFound("no such session".into()))?;
            if s.kind != Kind::RemoteDesktop || !s.started || s.devices & needs == 0 {
                return Err(PortalError::NotAllowed("the session has no such device".into()));
            }
        }
        self.ctx.ede(|c| c.input(event.clone())).map_err(|e| PortalError::NotAllowed(e.to_string()))
    }

    fn stream_origin(&self, session: &OwnedObjectPath, node: u32) -> Result<(f64, f64), PortalError> {
        let sessions = self.ctx.sessions.lock().unwrap();
        let s = sessions.get(session.as_str()).ok_or_else(|| PortalError::NotFound("no such session".into()))?;
        let st = s.streams.iter().find(|st| st.node_id == node).ok_or_else(|| PortalError::InvalidArgument("no such stream".into()))?;
        Ok((st.position.0 as f64, st.position.1 as f64))
    }
}

fn pressed(state: u32) -> bool {
    state != 0
}

#[interface(name = "org.freedesktop.impl.portal.RemoteDesktop")]
impl RemoteDesktop {
    async fn create_session(
        &self,
        _handle: OwnedObjectPath,
        session_handle: OwnedObjectPath,
        app_id: String,
        _options: Vardict,
        #[zbus(object_server)] server: &zbus::ObjectServer,
    ) -> Result<(u32, Vardict), PortalError> {
        session::open(&self.ctx, server, &session_handle, Session::new(Kind::RemoteDesktop, &app_id)).await?;
        Ok((SUCCESS, Vardict::new()))
    }

    async fn select_devices(&self, _handle: OwnedObjectPath, session_handle: OwnedObjectPath, _app_id: String, options: Vardict) -> Result<(u32, Vardict), PortalError> {
        let mut sessions = self.ctx.sessions.lock().unwrap();
        let s = sessions.get_mut(session_handle.as_str()).ok_or_else(|| PortalError::NotFound("no such session".into()))?;
        s.devices = opt::<u32>(&options, "types").unwrap_or(KEYBOARD | POINTER) & (KEYBOARD | POINTER);
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
        {
            let mut sessions = self.ctx.sessions.lock().unwrap();
            if let Some(s) = sessions.get_mut(session_handle.as_str()) {
                if s.devices == 0 {
                    s.devices = KEYBOARD | POINTER;
                }
            }
        }
        let ctx = self.ctx.clone();
        let path = session_handle.to_string();
        match blocking(move || start(&ctx, &path, true)).await {
            Ok(Some(mut results)) => {
                let devices = self.ctx.sessions.lock().unwrap().get(session_handle.as_str()).map(|s| s.devices).unwrap_or(0);
                results.insert("devices".into(), value(devices));
                results.insert("clipboard_enabled".into(), value(false));
                Ok((SUCCESS, results))
            }
            Ok(None) => Ok((CANCELLED, Vardict::new())),
            Err(e) => {
                tracing::warn!("remote desktop: {e}");
                Ok((ENDED, Vardict::new()))
            }
        }
    }

    async fn notify_pointer_motion(&self, session_handle: OwnedObjectPath, _options: Vardict, dx: f64, dy: f64) -> Result<(), PortalError> {
        self.drive(&session_handle, POINTER, InputEvent::PointerMotionRelative { dx, dy })
    }

    async fn notify_pointer_motion_absolute(&self, session_handle: OwnedObjectPath, _options: Vardict, stream: u32, x: f64, y: f64) -> Result<(), PortalError> {
        let (ox, oy) = self.stream_origin(&session_handle, stream)?;
        self.drive(&session_handle, POINTER, InputEvent::PointerMotion { x: ox + x, y: oy + y })
    }

    async fn notify_pointer_button(&self, session_handle: OwnedObjectPath, _options: Vardict, button: i32, state: u32) -> Result<(), PortalError> {
        self.drive(&session_handle, POINTER, InputEvent::PointerButton { button: button as u32, pressed: pressed(state) })
    }

    async fn notify_pointer_axis(&self, session_handle: OwnedObjectPath, _options: Vardict, dx: f64, dy: f64) -> Result<(), PortalError> {
        self.drive(&session_handle, POINTER, InputEvent::PointerAxis { dx, dy })
    }

    /// `axis` 0 = vertical, 1 = horizontal.
    async fn notify_pointer_axis_discrete(&self, session_handle: OwnedObjectPath, _options: Vardict, axis: u32, steps: i32) -> Result<(), PortalError> {
        let (dx, dy) = if axis == 1 { (steps, 0) } else { (0, steps) };
        self.drive(&session_handle, POINTER, InputEvent::PointerAxisDiscrete { dx, dy })
    }

    /// Evdev keycodes.
    async fn notify_keyboard_keycode(&self, session_handle: OwnedObjectPath, _options: Vardict, keycode: i32, state: u32) -> Result<(), PortalError> {
        self.drive(&session_handle, KEYBOARD, InputEvent::Key { key: keycode as u32, pressed: pressed(state) })
    }

    async fn notify_keyboard_keysym(&self, _session_handle: OwnedObjectPath, _options: Vardict, _keysym: i32, _state: u32) -> Result<(), PortalError> {
        Err(PortalError::NotAllowed("keysyms are not supported; send keycodes".into()))
    }

    async fn notify_touch_down(&self, _session_handle: OwnedObjectPath, _options: Vardict, _stream: u32, _slot: u32, _x: f64, _y: f64) -> Result<(), PortalError> {
        Err(PortalError::NotAllowed("no touchscreen device".into()))
    }

    async fn notify_touch_motion(&self, _session_handle: OwnedObjectPath, _options: Vardict, _stream: u32, _slot: u32, _x: f64, _y: f64) -> Result<(), PortalError> {
        Err(PortalError::NotAllowed("no touchscreen device".into()))
    }

    async fn notify_touch_up(&self, _session_handle: OwnedObjectPath, _options: Vardict, _slot: u32) -> Result<(), PortalError> {
        Err(PortalError::NotAllowed("no touchscreen device".into()))
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn available_device_types(&self) -> u32 {
        KEYBOARD | POINTER
    }

    #[zbus(property(emits_changed_signal = "const"), name = "version")]
    fn version(&self) -> u32 {
        1
    }
}
