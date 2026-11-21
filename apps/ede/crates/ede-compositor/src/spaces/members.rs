//! #EDE-127: which agent space a process belongs to. polkitd finds the
//! person's authentication agent by the login session, and what was
//! launched into a space is in that session; eDE's polkit agent asks here
//! (`space_of_process`) before it shows the person a prompt, and refuses
//! a request from a space.
//!
//! #EDE-138: first of all a process belongs to space D when it, or a
//! process it descends from, runs in a scope of D's own systemd slice.
//! Its cgroup stays with a process that starts a session of its own with
//! an empty environment and lets its parent die, which the four older
//! signs do not survive together. Where no user manager makes scopes,
//! those four are all there is.
//!
//! A process belongs to space D when it, or a process it descends from:
//! is a launch into D (a process the compositor started there), is in
//! the process group of one (a launch leads its own group, and a child
//! keeps it after its parent died), is connected to D's socket, or has
//! D's directory as its runtime directory or socket (what every launch
//! starts with). Like the grants (`launch.rs`), this guards against
//! accidents. The cgroup is a sign too, not a wall: every cgroup under
//! the user manager belongs to the user, and a process that writes its
//! own id into another one's `cgroup.procs` has left its space's slice.

use ede_protocol::{process, Membership, ProcessAnswer, ProcessSpace, SpaceId};

use crate::{desk::DeskId, state::Ede};

/// The id of the space desk `desk` is or was: a closed space's processes
/// may still be ending.
fn space_id(desk: DeskId) -> Option<SpaceId> {
    SpaceId::new(&format!("space-{}", desk.number())).ok()
}

impl Ede {
    /// #EDE-143: what `space_of_process` answers: the space `pid` belongs
    /// to, whether it runs in the compositor's own cgroup (the person's
    /// login session scope, which no process of a space gets back into:
    /// the session scope is root's), whether a space has launches alive
    /// outside systemd scopes (such a process is in the session's cgroup
    /// too), and whether no space exists at all (then every prompt is the
    /// person's, as before #EDE-143).
    pub fn process_answer(&self, pid: u32) -> ProcessAnswer {
        let session = self.spaces.session_cgroup.clone().or_else(|| process::cgroup(std::process::id()));
        ProcessAnswer {
            space: self.space_of_process(pid),
            session: session.is_some_and(|own| process::cgroup(pid).is_some_and(|theirs| theirs == own)),
            unscoped_launches: self.spaces.units.is_none() && !self.spaces.launches.is_empty(),
            no_space: self.spaces.none_alive(),
        }
    }

    /// The space `pid` belongs to; `None` = the person's, or nobody's.
    pub fn space_of_process(&self, pid: u32) -> Option<ProcessSpace> {
        let launched: Vec<(u32, DeskId)> = self.spaces.launches.iter().map(|launch| (launch.pid(), launch.desk())).collect();
        let connected: Vec<(u32, DeskId)> = self
            .spaces
            .iter()
            .flat_map(|space| space.clients.iter().filter_map(|client| client.get_credentials(&self.display_handle).ok()).map(move |c| (c.pid as u32, space.desk.id)))
            .collect();
        let tied = |by: Membership, desk: DeskId, at: u32| space_id(desk).map(|space| ProcessSpace { space, by, pid: at });
        // The slices of this compositor's spaces only: another one's are
        // not this one's to name.
        if let Some((_, space, at)) = process::space_by_cgroup(pid).filter(|(compositor, ..)| self.spaces.units.as_ref().is_some_and(|units| units.compositor == *compositor)) {
            return Some(ProcessSpace { space, by: Membership::Cgroup, pid: at });
        }
        for stat in process::lineage(pid) {
            if let Some(&(_, desk)) = launched.iter().find(|(head, _)| *head == stat.pid) {
                return tied(Membership::Launch, desk, stat.pid);
            }
            if let Some(&(_, desk)) = launched.iter().find(|(head, _)| *head == stat.pgid) {
                return tied(Membership::Group, desk, stat.pid);
            }
            if let Some(&(_, desk)) = connected.iter().find(|(client, _)| *client == stat.pid) {
                return tied(Membership::Client, desk, stat.pid);
            }
            if let Some(space) = self.spaces.dir.as_deref().and_then(|root| process::space_in_environment(stat.pid, root)) {
                return Some(ProcessSpace { space, by: Membership::Environment, pid: stat.pid });
            }
        }
        None
    }
}
