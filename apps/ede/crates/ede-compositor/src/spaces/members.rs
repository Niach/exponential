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
//!
//! #EDE-183: so the close of a space also ends what these signs tie to it
//! outside its slice and its launches' groups ([`Ede::traced_to_space`]):
//! otherwise such a process outlives its space, and once no space is left
//! its polkit request would take the person's keyboard (#EDE-143).

use std::collections::HashMap;

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

    /// #EDE-183: the processes of this user that space `desk` started, by
    /// what ties them to it while it is open: a launch, a process of a
    /// launch's group or one descending from either, or the space's
    /// directory in its environment or an ancestor's, nearest first as
    /// [`Ede::space_of_process`] reads them. Wherever their cgroup is now:
    /// also what moved itself out of the space's slice. Not what itself
    /// runs in a slice of this compositor (its stop ends that) or in one of the
    /// space's launches' groups (the launch's end reaches that), and not
    /// a client of the space's socket (it connected; the space did not
    /// start it). Read once at the close: one `stat` per process, one
    /// environment at most.
    pub(crate) fn traced_to_space(&self, desk: DeskId) -> Vec<u32> {
        let Some(space) = space_id(desk) else { return Vec::new() };
        let me = std::process::id();
        let launches: HashMap<u32, SpaceId> = self.spaces.launches.iter().filter_map(|launch| Some((launch.pid(), space_id(launch.desk())?))).collect();
        let stats: HashMap<u32, process::Stat> = this_users_processes().into_iter().filter_map(|pid| Some((pid, process::stat(pid)?))).collect();
        let root = self.spaces.dir.clone();
        let tied = tied_to(&stats, &launches, |pid| root.as_deref().and_then(|root| process::space_in_environment(pid, root)));
        let compositor = self.spaces.units.as_ref().map(|units| units.compositor);
        strays(&space, &stats, &launches, &tied, me, compositor, process::cgroup)
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

/// The process ids in `/proc` that run as this user.
fn this_users_processes() -> Vec<u32> {
    use std::os::unix::fs::MetadataExt;
    let uid = smithay::reexports::rustix::process::getuid().as_raw();
    let Ok(entries) = std::fs::read_dir("/proc") else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok().filter(|_| entry.metadata().is_ok_and(|meta| meta.uid() == uid)))
        .collect()
}

/// #EDE-183: the space each of `stats` is tied to, nearest first: the
/// process itself or the nearest process it descends from that is a launch
/// (`launches`: its process id and space), in a launch's group, or names a
/// space in its environment (`environment_of`); `None` = none. A process's
/// own tie is read once, whatever number of processes descend from it.
/// What descends from a process outside `stats` (another user's) stops
/// there.
pub(crate) fn tied_to(stats: &HashMap<u32, process::Stat>, launches: &HashMap<u32, SpaceId>, environment_of: impl Fn(u32) -> Option<SpaceId>) -> HashMap<u32, Option<SpaceId>> {
    let own = |pid: u32, stat: &process::Stat| launches.get(&pid).or_else(|| launches.get(&stat.pgid)).cloned().or_else(|| environment_of(pid));
    let mut tied: HashMap<u32, Option<SpaceId>> = HashMap::with_capacity(stats.len());
    for &pid in stats.keys() {
        let mut chain: Vec<u32> = Vec::new();
        let mut next = pid;
        let found = loop {
            if let Some(known) = tied.get(&next) {
                break known.clone();
            }
            let Some(stat) = stats.get(&next) else { break None };
            if let Some(space) = own(next, stat) {
                tied.insert(next, Some(space.clone()));
                break Some(space);
            }
            chain.push(next);
            if stat.ppid <= 1 || chain.len() >= process::MAX_LINEAGE || chain.contains(&stat.ppid) {
                break None;
            }
            next = stat.ppid;
        };
        for pid in chain {
            tied.insert(pid, found.clone());
        }
    }
    tied
}

/// #EDE-183: of `stats`, what is tied to `space` (`tied`, [`tied_to`])
/// and its close does not reach otherwise: not the compositor (`me`), not
/// a process of one of the space's launches' groups (`launches`: the
/// launch's end reaches it), not one running in a slice of compositor
/// `compositor`'s spaces (`None` = no user manager, no slices: the slice's
/// stop ends it). That is the process's OWN cgroup (`cgroup_of`), never its
/// lineage's as [`process::space_by_cgroup`] reads it: a process that moved
/// itself out of the slice still descends from one in it, and the slice's
/// stop does not reach it.
pub(crate) fn strays(space: &SpaceId, stats: &HashMap<u32, process::Stat>, launches: &HashMap<u32, SpaceId>, tied: &HashMap<u32, Option<SpaceId>>, me: u32, compositor: Option<u32>, cgroup_of: impl Fn(u32) -> Option<String>) -> Vec<u32> {
    let in_a_slice = |pid: u32| compositor.is_some_and(|compositor| cgroup_of(pid).and_then(|cgroup| process::space_in_cgroup(&cgroup)).is_some_and(|(owner, _)| owner == compositor));
    stats
        .iter()
        .filter(|(pid, stat)| **pid > 1 && **pid != me && launches.get(&stat.pgid) != Some(space))
        .filter(|(pid, _)| tied.get(*pid).is_some_and(|found| found.as_ref() == Some(space)))
        .filter(|(pid, _)| !in_a_slice(**pid))
        .map(|(pid, _)| *pid)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Nearest first, as `space_of_process` reads a lineage: a launch, a
    /// process of a launch's group, a space in the environment; what
    /// descends from one of them is that space's unless something nearer
    /// names another.
    #[test]
    fn a_process_is_tied_to_the_space_nearest_in_its_lineage() {
        let space = |n: u32| SpaceId::new(&format!("space-{n}")).unwrap();
        let stat = |pid: u32, ppid: u32, pgid: u32| (pid, process::Stat { pid, ppid, pgid });
        let stats: HashMap<u32, process::Stat> = [
            stat(10, 1, 10),   // the person's session
            stat(100, 1, 100), // a launch into space 1
            stat(101, 100, 100),
            stat(102, 101, 102), // left the group: its parent ties it
            stat(103, 1, 103),   // an orphan that names space 1
            stat(104, 103, 104), // its child, with no environment
            stat(105, 100, 105), // names space 2 nearer than its launch
            stat(200, 1, 200),   // a launch into space 2
            stat(11, 10, 10),    // the person's
            stat(12, 99, 12),    // its parent is another user's
        ]
        .into_iter()
        .collect();
        let launches: HashMap<u32, SpaceId> = [(100, space(1)), (200, space(2))].into_iter().collect();
        let tied = tied_to(&stats, &launches, |pid| match pid {
            103 => Some(space(1)),
            105 => Some(space(2)),
            _ => None,
        });
        let of = |pid: u32| tied.get(&pid).cloned().flatten();
        for pid in [100, 101, 102, 103, 104] {
            assert_eq!(of(pid), Some(space(1)), "process {pid}");
        }
        assert_eq!(of(105), Some(space(2)), "the nearest tie");
        assert_eq!(of(200), Some(space(2)));
        for pid in [10, 11, 12] {
            assert_eq!(of(pid), None, "process {pid}");
        }
    }

    /// What the close ends itself: a process tied to the space that is
    /// neither in a launch's group nor itself in a slice of this
    /// compositor. A launch's child that moved itself into a scope of the
    /// person's still descends from the launch in the slice: its own
    /// cgroup, not its parent's, says the slice's stop misses it (the
    /// `ede-scope` failure of #3237).
    #[test]
    fn a_stray_is_what_its_own_cgroup_keeps_out_of_this_compositors_slices() {
        let space = |n: u32| SpaceId::new(&format!("space-{n}")).unwrap();
        let stat = |pid: u32, ppid: u32, pgid: u32| (pid, process::Stat { pid, ppid, pgid });
        let manager = "0::/user.slice/user-1000.slice/user@1000.service/app.slice";
        let scope = |compositor: u32, n: u32| format!("{manager}/app-exponential_p{compositor}.slice/app-exponential_p{compositor}-s{n}.slice/exponential-p{compositor}-s{n}-1.scope\n");
        let stats: HashMap<u32, process::Stat> = [
            stat(7, 1, 7),       // the compositor
            stat(100, 7, 100),   // a launch into space 1, in its scope
            stat(101, 100, 100), // in its group, in the scope
            stat(102, 100, 102), // left the group and moved into the person's scope
            stat(103, 100, 103), // left the group, still in the scope
            stat(104, 1, 104),   // an orphan that names space 1, in the person's scope
            stat(105, 100, 105), // left the group for another compositor's slice
            stat(200, 7, 200),   // a launch into space 2
            stat(201, 200, 201), // left its group and moved into the person's scope
            stat(10, 1, 10),     // the person's
        ]
        .into_iter()
        .collect();
        let launches: HashMap<u32, SpaceId> = [(100, space(1)), (200, space(2))].into_iter().collect();
        let tied = tied_to(&stats, &launches, |pid| (pid == 104).then(|| space(1)));
        let cgroup_of = |pid: u32| match pid {
            100 | 101 | 103 => Some(scope(7, 1)),
            105 => Some(scope(8, 1)),
            200 => Some(scope(7, 2)),
            7 | 10 | 102 | 104 | 201 => Some(format!("{manager}/exp-person.scope\n")),
            _ => None,
        };
        let sorted = |mut pids: Vec<u32>| {
            pids.sort_unstable();
            pids
        };
        assert_eq!(sorted(strays(&space(1), &stats, &launches, &tied, 7, Some(7), cgroup_of)), [102, 104, 105]);
        assert_eq!(sorted(strays(&space(2), &stats, &launches, &tied, 7, Some(7), cgroup_of)), [201]);
        // Without a user manager there are no slices: only the groups and
        // the compositor are left out.
        assert_eq!(sorted(strays(&space(1), &stats, &launches, &tied, 7, None, cgroup_of)), [102, 103, 104, 105]);
    }
}
