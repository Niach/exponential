//! #EDE-127: which agent space a process belongs to. polkitd finds the
//! person's authentication agent by the login session, and what was
//! launched into a space is in that session; eDE's polkit agent asks here
//! (`space_of_process`) before it shows the person a prompt, and refuses
//! a request from a space.
//!
//! A process belongs to space D when it, or a process it descends from:
//! is a launch into D (a process the compositor started there), is in
//! the process group of one (a launch leads its own group, and a child
//! keeps it after its parent died), is connected to D's socket, or has
//! D's directory as its runtime directory or socket (what every launch
//! starts with). Like the grants (`launch.rs`), this guards against
//! accidents: a process that leaves its group, clears its environment and
//! lets its parent die is out of reach until F3 (#EDE-138) gives a space's
//! processes a cgroup of their own.

use ede_protocol::{process, Membership, ProcessSpace, SpaceId};

use crate::{desk::DeskId, state::Ede};

/// The id of the space desk `desk` is or was: a closed space's processes
/// may still be ending.
fn space_id(desk: DeskId) -> Option<SpaceId> {
    SpaceId::new(&format!("space-{}", desk.number())).ok()
}

impl Ede {
    /// The space `pid` belongs to; `None` = the person's, or nobody's.
    pub fn space_of_process(&self, pid: u32) -> Option<ProcessSpace> {
        let launched: Vec<(u32, DeskId)> = self.spaces.launches.iter().map(|launch| (launch.pid(), launch.desk())).collect();
        let connected: Vec<(u32, DeskId)> = self
            .spaces
            .iter()
            .flat_map(|space| space.clients.iter().filter_map(|client| client.get_credentials(&self.display_handle).ok()).map(move |c| (c.pid as u32, space.desk.id)))
            .collect();
        let tied = |by: Membership, desk: DeskId, at: u32| space_id(desk).map(|space| ProcessSpace { space, by, pid: at });
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
