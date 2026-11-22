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

/// What an op answers.
pub(crate) type Reply = Result<Option<(serde_json::Value, Option<Vec<u8>>)>, Failure>;

/// The call an op serves: its reply can come later
/// ([`Ede::ipc_reply_later`]), when a space's capture waits its turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Caller {
    pub conn: u64,
    pub call: u64,
}

/// What [`Ede::space_scope_call`] made of a space connection's call.
pub(crate) enum Scoped {
    /// Answered here.
    Reply(Option<(serde_json::Value, Option<Vec<u8>>)>),
    /// Input without bindings, the pointer's position, the clipboard: the
    /// op's own arm serves it, on this desk, the caller's space (#EDE-125).
    Through(DeskId, Request),
}

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
    pub(crate) fn space_scope_call(&mut self, session: &SessionId, grant: &Grant, now: i64, request: Request, caller: Caller) -> Result<Scoped, Failure> {
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
            Request::CreateSpace { width, height } => Ok(Scoped::Reply(Some((serde_json::to_value(self.space_create(session, width, height)?).unwrap(), None)))),
            Request::ListSpaces => Ok(Scoped::Reply(Some((serde_json::to_value(self.space_infos(Some(session))).unwrap(), None)))),
            // #EXP-1514 (D 10): its session's turn runs.
            Request::SpaceBusy => {
                let Some(desk) = bound else { return fail(ErrorCode::NotFound, "this grant has no space") };
                self.space_mark_busy(desk);
                Ok(Scoped::Reply(Some((serde_json::Value::Null, None))))
            }
            Request::DestroySpace { space } => {
                self.space_destroy(Some(session), &space)?;
                Ok(Scoped::Reply(Some((serde_json::Value::Null, None))))
            }
            request @ (Request::Launch { .. } | Request::ListWindows | Request::ListOutputs | Request::FocusWindow { .. } | Request::CloseWindow { .. } | Request::Capture { .. } | Request::Input { .. } | Request::PointerPosition | Request::ClipboardSerial | Request::ClipboardSnapshot | Request::ClipboardSet { .. } | Request::ClipboardTaken { .. } | Request::ClipboardText) => {
                let Some(desk) = bound else { return fail(ErrorCode::NotFound, "this grant has no space") };
                match request {
                    Request::Launch { argv, env, cwd } => {
                        let app = argv.first().cloned().unwrap_or_default();
                        // #EDE-162: a space that draws on the GPU starts its
                        // programs with the GPU (F3's switch off), from the
                        // first one on, whose user manager keeps that
                        // environment; a latch turns it back on (`frames::net`).
                        if self.space_gpu_allowed(desk) {
                            self.set_space_software_rendering(desk, false);
                        }
                        let launched = self.space_launch(desk, argv, env, cwd)?;
                        // #EDE-162: a GPU space watches how it starts.
                        self.space_net_launched(desk, launched.pid, &app);
                        Ok(Scoped::Reply(Some((serde_json::to_value(launched).unwrap(), None))))
                    }
                    Request::ListWindows | Request::ListOutputs | Request::FocusWindow { .. } | Request::CloseWindow { .. } | Request::Capture { .. } => self.space_call(desk, request, caller).map(Scoped::Reply),
                    through => Ok(Scoped::Through(desk, through)),
                }
            }
            // `in_space` refused these above; named so that a new op has to
            // be placed here by hand.
            Request::MoveWindow { .. } | Request::MinimizeWindow { .. } | Request::RestoreWindow { .. } | Request::ListWorkspaces | Request::SwitchWorkspace { .. } | Request::MoveToWorkspace { .. } | Request::HoldInput | Request::ReleaseInput | Request::Subscribe | Request::BindShortcuts { .. } | Request::UnbindShortcuts { .. } | Request::SetOutput { .. } | Request::ReloadConfig | Request::Lock | Request::Logout | Request::Unlock | Request::MintGrant { .. } | Request::RevokeGrant { .. } | Request::LaunchToken | Request::SpaceOfProcess { .. } | Request::SpaceReadiness | Request::OpenChannel { .. } => fail(ErrorCode::Forbidden, "not an op of an agent space"),
        }
    }
}
