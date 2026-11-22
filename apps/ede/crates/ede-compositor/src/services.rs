//! #EDE-172 (P13): the services daemon (`exponential-services serve`) as a
//! helper of the compositor ([`crate::helpers`], `[session] helpers =
//! "compositor"`, a real session's default since eDE's flip, P14). It
//! binds no `services.sock`:
//! its fd 3 is its intake ([`ede_protocol::intake`]), and a connection to
//! it is something a principal asks the compositor for on its own channel
//! ([`ede_protocol::Request::OpenServices`]). The compositor makes the
//! pair, hands the daemon its end with the capabilities the asking grant
//! holds (the daemon's `call` holds exactly those), and the asker the
//! other. The shell's root channel asks with every capability (its buttons
//! and pages); no other principal asks today.
//!
//! With `helpers = "units"` the daemon's unit serves `services.sock` as
//! before eDE's flip: every capability to any process of the person.

use std::os::{fd::OwnedFd, unix::net::UnixStream};

use ede_protocol::{intake::Handed, Capability, ErrorCode, Failure, SessionId};

use crate::state::Ede;

/// The services daemon the compositor runs ([`Ede::services`]).
#[derive(Debug, Default)]
pub struct Services {
    /// The compositor's end of the running daemon's intake. A daemon
    /// started again gets a new one; the old one's connections end with it.
    intake: Option<OwnedFd>,
}

impl Services {
    /// A daemon's intake is open.
    pub fn running(&self) -> bool {
        self.intake.is_some()
    }
}

impl Ede {
    /// A new intake for the services daemon the compositor starts now: the
    /// daemon's end (its fd 3); the compositor keeps the other.
    pub(crate) fn services_intake(&mut self) -> Option<OwnedFd> {
        let (ours, theirs) = ede_protocol::intake::pair().map_err(|e| tracing::warn!("the services' intake: {e}")).ok()?;
        self.services.intake = Some(ours);
        Some(theirs)
    }

    /// `open_services` for a grant of `session_id` that holds
    /// `capabilities` (the arm checked that): the asker's end of a new
    /// connection whose other end went to the daemon with them.
    pub(crate) fn services_open(&mut self, session_id: &SessionId, capabilities: Vec<Capability>) -> Result<OwnedFd, Failure> {
        let Some(intake) = &self.services.intake else {
            return Err(Failure::new(ErrorCode::Unavailable, "the compositor runs no services daemon here (its unit serves services.sock)"));
        };
        let (asker, daemon) = UnixStream::pair().map_err(|e| Failure::new(ErrorCode::Internal, &e.to_string()))?;
        let handed = Handed { session_id: session_id.clone(), capabilities };
        match ede_protocol::intake::send(intake, &handed, &daemon) {
            Ok(()) => {
                tracing::info!(session = session_id.as_str(), capabilities = ?handed.capabilities, "a services connection");
                Ok(asker.into())
            }
            // A daemon that does not read its intake, or one that ended
            // (started again on the next tick, with a new intake).
            Err(e) => {
                tracing::warn!("the services daemon took no connection: {e}");
                Err(Failure::new(ErrorCode::Unavailable, &format!("the services daemon is not answering: {e}")))
            }
        }
    }
}
