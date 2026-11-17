//! #EDE-125: the `ede-protocol` ops of agent spaces. A space grant makes
//! its session's one space; the space is found again through the session,
//! never through a connection (ePilot connects per call).

use ede_protocol::{ErrorCode, Failure, SessionId, SpaceId, SpaceInfo};

use super::AgentSpace;
use crate::state::Ede;

/// Every space is a desk with a software renderer behind it.
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
        if self.spaces.iter().count() >= MAX_SPACES {
            return Err(Failure::new(ErrorCode::Limit, &format!("{MAX_SPACES} spaces at most")));
        }
        let desk = self.create_space(session.clone(), width, height)?;
        Ok(info(self.spaces.get(desk).expect("just made")))
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
}
