//! #EDE-125: the `ede-protocol` ops of agent spaces. A space grant makes
//! its session's one space; the space is found again through the session,
//! never through a connection (ePilot connects per call).

use ede_protocol::{CaptureTarget, ErrorCode, Failure, OutputInfo, Request, SessionId, SpaceId, SpaceInfo, WindowId, WindowInfo};
use smithay::desktop::Window;

use super::{
    scope::{Caller, Reply},
    AgentSpace,
};
use crate::{
    desk::DeskId,
    ipc::{window_id, window_title},
    state::Ede,
};

/// Every space is a desk with a software renderer behind it. Where launches
/// run in systemd scopes the person's `[spaces] max` (or the memory) says
/// how many (#EDE-138, [`super::slice::max_spaces`]); elsewhere this many.
pub const MAX_SPACES: usize = 4;

fn info(space: &AgentSpace) -> SpaceInfo {
    let size = space.output.current_mode().map(|m| m.size).unwrap_or_default();
    SpaceInfo {
        id: space.id.clone(),
        session_id: space.owner.clone(),
        output: space.output.name(),
        width: size.w,
        height: size.h,
        wayland_display: space.socket().to_string_lossy().into_owned(),
    }
}

impl Ede {
    /// `create_space` of a space connection: the session's space, made
    /// now unless it has one (then that one, whatever size is asked for).
    pub(crate) fn space_create(&mut self, session: &SessionId, width: i32, height: i32) -> Result<SpaceInfo, Failure> {
        if !super::size_ok(width, height) {
            return Err(super::size_refused());
        }
        if let Some(space) = self.spaces.iter().find(|s| &s.owner == session) {
            return Ok(info(space));
        }
        // The count, and the stronger fence, room under the slice's
        // `MemoryHigh`: a full desktop gives way (#EXP-1514, D 10).
        self.space_make_room(self.space_max(), session)?;
        let desk = self.create_space(session.clone(), width, height)?;
        Ok(info(self.spaces.get(desk).expect("just made")))
    }

    /// How many spaces this desktop holds at most.
    pub(crate) fn space_max(&self) -> usize {
        self.spaces.units.as_ref().map_or(MAX_SPACES, |units| super::slice::max_spaces(&self.config.spaces, units.machine) as usize)
    }

    /// `list_spaces`: one session's (a space connection), or all (`system`).
    pub(crate) fn space_infos(&self, owner: Option<&SessionId>) -> Vec<SpaceInfo> {
        self.spaces.iter().filter(|s| owner.is_none_or(|o| &s.owner == o)).map(info).collect()
    }

    /// `destroy_space`: `owner` = a space connection, which closes only
    /// its own; `None` = `system`.
    pub(crate) fn space_destroy(&mut self, owner: Option<&SessionId>, space: &SpaceId) -> Result<(), Failure> {
        let Some((desk, its)) = self.spaces.iter().find(|s| &s.id == space).map(|s| (s.desk.id, s.owner.clone())) else {
            return Err(Failure::new(ErrorCode::NotFound, "no such space"));
        };
        if owner.is_some_and(|o| *o != its) {
            return Err(Failure::new(ErrorCode::Forbidden, "another session's space"));
        }
        self.close_space(desk);
        Ok(())
    }

    /// The ops a space connection asks about its own space `desk`: lists,
    /// focus, close and capture. It searches that space only: a window id
    /// or an output name of another desk is `not_found`.
    pub(crate) fn space_call(&mut self, desk: DeskId, request: Request, caller: Caller) -> Reply {
        let json = |v: serde_json::Value| Ok(Some((v, None)));
        let Some(space) = self.spaces.get(desk) else { return Err(Failure::new(ErrorCode::NotFound, "this grant has no space")) };
        let (id, output) = (space.id.clone(), space.output.clone());
        match request {
            Request::ListWindows => {
                let focus = space.desk.seat.get_keyboard().and_then(|k| k.current_focus());
                let windows: Vec<WindowInfo> = space
                    .desk
                    .space
                    .elements()
                    // A menu or a tooltip of an X11 app is no window of its own.
                    .filter(|w| !crate::ipc::x11_override_redirect(w))
                    .filter_map(|w| {
                        let g = space.desk.space.element_geometry(w)?;
                        let protected = self.is_protected_window(w);
                        let (title, app_id) = window_title(w);
                        let focused = focus.is_some() && smithay::wayland::seat::WaylandFocus::wl_surface(w).map(|s| s.into_owned()) == focus;
                        Some(WindowInfo { id: window_id(w), title: title.filter(|_| !protected), app_id, x: g.loc.x, y: g.loc.y, width: g.size.w, height: g.size.h, focused, fullscreen: false, protected, x11: crate::ipc::is_x11(w), workspace: 0, minimized: false, space: Some(id.clone()) })
                    })
                    .collect();
                json(serde_json::to_value(windows).unwrap())
            }
            Request::ListOutputs => {
                let size = output.current_mode().map(|m| m.size).unwrap_or_default();
                let modes = vec![ede_protocol::ModeInfo { width: size.w, height: size.h, refresh: 60_000, preferred: true, current: true }];
                let p = output.physical_properties();
                let info = OutputInfo { name: output.name(), x: 0, y: 0, width: size.w, height: size.h, scale: 1.0, modes, make: p.make, model: p.model, physical_mm: [0, 0], space: Some(id) };
                json(serde_json::to_value(vec![info]).unwrap())
            }
            Request::FocusWindow { window } => {
                let window = self.space_window_by_id(desk, &window)?;
                self.space_focus(desk, Some(window));
                json(serde_json::Value::Null)
            }
            Request::CloseWindow { window } => {
                let window = self.space_window_by_id(desk, &window)?;
                self.close_window(&window);
                json(serde_json::Value::Null)
            }
            Request::Capture { target, since } => {
                match target {
                    CaptureTarget::Output { name } if name == output.name() => {}
                    CaptureTarget::Output { .. } => return Err(Failure::new(ErrorCode::NotFound, "no such output")),
                    CaptureTarget::Space { space } if space == id => {}
                    CaptureTarget::Space { .. } => return Err(Failure::new(ErrorCode::Forbidden, "another session's space")),
                    CaptureTarget::Window { .. } => return Err(Failure::new(ErrorCode::Forbidden, "not an op of an agent space")),
                }
                self.space_capture(desk, since, caller, true)
            }
            _ => Err(Failure::new(ErrorCode::Forbidden, "not an op of an agent space")),
        }
    }

    fn space_window_by_id(&self, desk: DeskId, id: &WindowId) -> Result<Window, Failure> {
        let found = self.try_desk(desk).and_then(|d| d.space.elements().find(|w| &window_id(w) == id).cloned());
        found.ok_or_else(|| Failure::new(ErrorCode::NotFound, "no such window"))
    }
}
