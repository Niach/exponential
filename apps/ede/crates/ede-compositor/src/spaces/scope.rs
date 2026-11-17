//! #EDE-126: whose desk an `ede-protocol` connection acts on. Fixed by its
//! hello and kept on the connection: every later call and the drop read it
//! there, never from the grant (a revoked grant's connection still lets go
//! of what it held).
//!
//! A space grant holds `space` and not `system`. Its connection never
//! reaches an arm of the person's desk: [`Ede::space_scope_call`] answers
//! every op, with no wildcard arm. This is the wall; the capability check
//! is not (six ops need no capability at all).

use ede_protocol::{Capability, ErrorCode, Failure, Grant, Request, SessionId};

use crate::{desk::DeskId, ipc::ROOT_SESSION, state::Ede};

/// What [`Ede::space_scope_call`] and the ops answer.
pub(crate) type Reply = Result<Option<(serde_json::Value, Option<Vec<u8>>)>, Failure>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The person's desk: every grant but a space grant.
    Person,
    /// The space this session owns (none yet = unbound).
    Space(SessionId),
}

impl Scope {
    pub fn of(grant: &Grant) -> Scope {
        if grant.capabilities.contains(&Capability::Space) && !grant.capabilities.contains(&Capability::System) {
            Scope::Space(grant.session_id.clone())
        } else {
            Scope::Person
        }
    }
}

/// Why a mint is refused before anything else is looked at: a space grant
/// is `[space]` alone (so [`Scope::of`] has one answer), and no minted
/// grant bears the shell's session name (the bindings, the shell's
/// subscription and the root's revoke guard go by that name).
pub(crate) fn mint_refused(session: &SessionId, capabilities: &[Capability]) -> Option<&'static str> {
    if session.as_str() == ROOT_SESSION {
        return Some("that session name is the shell's");
    }
    if capabilities.contains(&Capability::Space) && capabilities.iter().any(|c| *c != Capability::Space) {
        return Some("a space grant holds space and nothing else");
    }
    None
}

impl Ede {
    /// The desk a connection of `scope` acts on now; `None` = a space
    /// scope whose session owns no live space.
    pub(crate) fn desk_of_scope(&self, scope: &Scope) -> Option<DeskId> {
        match scope {
            Scope::Person => Some(DeskId::PERSON),
            Scope::Space(session) => self.spaces.iter().find(|s| &s.owner == session).map(|s| s.desk.id),
        }
    }

    /// Every call of a space connection after its hello. Outside the
    /// allowlist: `forbidden`. Unbound, only `create_space` and
    /// `list_spaces` pass, the rest is `not_found`.
    pub(crate) fn space_scope_call(&mut self, session: &SessionId, grant: &Grant, now: i64, request: Request) -> Reply {
        let fail = |code, message: &str| Err(Failure::new(code, message));
        if !request.in_space() {
            return fail(ErrorCode::Forbidden, "not an op of an agent space");
        }
        if !grant.admits(Capability::Space, now) {
            return fail(ErrorCode::Forbidden, "needs the space capability");
        }
        let bound = self.desk_of_scope(&Scope::Space(session.clone()));
        match request {
            Request::Hello { .. } => unreachable!("hello is answered before the scope"),
            Request::CreateSpace { width, height } => Ok(Some((serde_json::to_value(self.space_create(session, width, height)?).unwrap(), None))),
            Request::ListSpaces => Ok(Some((serde_json::to_value(self.space_infos(Some(session))).unwrap(), None))),
            Request::DestroySpace { space } => {
                self.space_destroy(Some(session), &space)?;
                Ok(Some((serde_json::Value::Null, None)))
            }
            Request::Launch { .. } | Request::ListWindows | Request::ListOutputs | Request::FocusWindow { .. } | Request::CloseWindow { .. } | Request::Capture { .. } | Request::Input { .. } | Request::PointerPosition | Request::ClipboardSerial | Request::ClipboardSnapshot | Request::ClipboardSet { .. } | Request::ClipboardTaken { .. } | Request::ClipboardText => {
                if bound.is_some() {
                    return fail(ErrorCode::Unavailable, "not served in a space yet");
                }
                fail(ErrorCode::NotFound, "this grant has no space")
            }
            // `in_space` refused these above; named so that a new op has to
            // be placed here by hand.
            Request::MoveWindow { .. } | Request::MinimizeWindow { .. } | Request::RestoreWindow { .. } | Request::ListWorkspaces | Request::SwitchWorkspace { .. } | Request::MoveToWorkspace { .. } | Request::HoldInput | Request::ReleaseInput | Request::Subscribe | Request::BindShortcuts { .. } | Request::UnbindShortcuts { .. } | Request::SetOutput { .. } | Request::ReloadConfig | Request::Lock | Request::Logout | Request::Unlock | Request::MintGrant { .. } | Request::RevokeGrant { .. } | Request::LaunchToken => fail(ErrorCode::Forbidden, "not an op of an agent space"),
        }
    }
}
