//! #EDE-172 (P8): which run a local peer belongs to, by its process
//! ancestry: the peer's parents are walked, start times compared, until one
//! is the agent process of a bound run (admitted as that run) or the walk
//! reaches init or the host itself (refused).
//!
//! The walk reads the process table through [`Procs`], so its rules are
//! tested on a made-up table (a reused pid, a gone ancestor) as well as on
//! the real one ([`Os`]: `/proc/<pid>/stat` on Linux, `proc_pidinfo` on
//! macOS).

use ecore::SessionId;

/// What the walk needs of one process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Stat {
    /// The parent's pid (`0` for init and kernel threads).
    pub parent: u32,
    /// When it started, in a unit fixed per platform (Linux: clock ticks
    /// since boot, `/proc/<pid>/stat` field 22; macOS: microseconds since
    /// the epoch). A pid reused by a later process starts later.
    pub start: u64,
}

/// The process table.
pub(crate) trait Procs {
    /// `pid` now; `None` once it is gone.
    fn stat(&self, pid: u32) -> Option<Stat>;
}

/// A run bound to its agent process ([`super::bind_process`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Bound {
    pub run: SessionId,
    pub pid: u32,
    /// The process's start when it was bound: a later process on the same
    /// pid is not the run's.
    pub start: u64,
}

/// Why a peer is not admitted. The text is what the peer reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refused {
    /// The peer (or an ancestor) exited before the walk read it.
    Gone,
    /// The walk reached init or the host without meeting a bound agent.
    Outside,
    /// An ancestor's pid names a process that started after its child:
    /// the real parent is gone and its pid taken.
    Reused,
    /// More than [`MAX_DEPTH`] ancestors.
    TooDeep,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Refused::Gone => "the process is gone",
            Refused::Outside => "not started by an agent run of this host",
            Refused::Reused => "an ancestor's process id was reused",
            Refused::TooDeep => "the process tree is too deep",
        })
    }
}

/// The most ancestors a walk reads (an agent's tool running a build three
/// shells deep is still far below it).
pub(crate) const MAX_DEPTH: usize = 128;

/// The run `peer` belongs to: the first process from `peer` up (the peer
/// itself included) that is a bound agent, pid AND start time, names it.
/// Reaching `host` (this process: the host's own children are not a run's)
/// or init without one refuses, so does a parent that started after its
/// child.
pub(crate) fn walk(procs: &impl Procs, peer: u32, host: u32, bound: &[Bound]) -> Result<SessionId, Refused> {
    let mut pid = peer;
    let mut stat = procs.stat(pid).ok_or(Refused::Gone)?;
    for _ in 0..MAX_DEPTH {
        if let Some(run) = bound.iter().find(|bound| bound.pid == pid && bound.start == stat.start) {
            return Ok(run.run.clone());
        }
        if pid == host || pid <= 1 || stat.parent <= 1 {
            return Err(Refused::Outside);
        }
        let parent = stat.parent;
        let up = procs.stat(parent).ok_or(Refused::Gone)?;
        if up.start > stat.start {
            return Err(Refused::Reused);
        }
        pid = parent;
        stat = up;
    }
    Err(Refused::TooDeep)
}

/// The real process table.
pub(crate) struct Os;

#[cfg(target_os = "linux")]
impl Procs for Os {
    fn stat(&self, pid: u32) -> Option<Stat> {
        parse_stat(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
    }
}

/// `/proc/<pid>/stat`: the fields after the command's closing parenthesis
/// (the command may hold spaces and parentheses of its own), field 4 the
/// parent, field 22 the start.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn parse_stat(text: &str) -> Option<Stat> {
    let rest = &text[text.rfind(')')? + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // fields[0] is field 3 (the state).
    Some(Stat { parent: fields.get(1)?.parse().ok()?, start: fields.get(19)?.parse().ok()? })
}

#[cfg(target_os = "macos")]
impl Procs for Os {
    fn stat(&self, pid: u32) -> Option<Stat> {
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
        // SAFETY: `proc_pidinfo` fills at most `size` bytes of `info`.
        let read = unsafe {
            libc::proc_pidinfo(pid as libc::c_int, libc::PROC_PIDTBSDINFO, 0, &mut info as *mut _ as *mut libc::c_void, size)
        };
        (read == size).then(|| Stat {
            parent: info.pbi_ppid,
            start: info.pbi_start_tvsec.saturating_mul(1_000_000).saturating_add(info.pbi_start_tvusec),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A made-up process table: pid → (parent, start).
    struct Table(HashMap<u32, Stat>);

    impl Table {
        fn new(rows: &[(u32, u32, u64)]) -> Table {
            Table(rows.iter().map(|&(pid, parent, start)| (pid, Stat { parent, start })).collect())
        }
    }

    impl Procs for Table {
        fn stat(&self, pid: u32) -> Option<Stat> {
            self.0.get(&pid).copied()
        }
    }

    const HOST: u32 = 10;

    fn run(id: &str) -> SessionId {
        SessionId::new(id).unwrap()
    }

    fn bound(id: &str, pid: u32, start: u64) -> Bound {
        Bound { run: run(id), pid, start }
    }

    /// init 1 → the person's shell 5 → the host 10 → the agent 100 (bound)
    /// → its CLI 200 → the relay 300; a terminal 400 the host opened; the
    /// other run's agent 500 → its tool 600.
    fn table() -> Table {
        Table::new(&[
            (1, 0, 1),
            (5, 1, 2),
            (HOST, 5, 3),
            (100, HOST, 50),
            (200, 100, 60),
            (300, 200, 70),
            (400, HOST, 80),
            (500, HOST, 90),
            (600, 500, 95),
        ])
    }

    #[test]
    fn a_descendant_of_a_bound_agent_is_its_run() {
        let bound = [bound("run-a", 100, 50), bound("run-b", 500, 90)];
        assert_eq!(walk(&table(), 300, HOST, &bound), Ok(run("run-a")));
        assert_eq!(walk(&table(), 200, HOST, &bound), Ok(run("run-a")));
        // The agent itself.
        assert_eq!(walk(&table(), 100, HOST, &bound), Ok(run("run-a")));
        // The other run's tool is the other run, never the first.
        assert_eq!(walk(&table(), 600, HOST, &bound), Ok(run("run-b")));
    }

    #[test]
    fn the_hosts_own_children_and_the_persons_processes_are_no_run() {
        let bound = [bound("run-a", 100, 50)];
        // A terminal the host opened: its parent is the host.
        assert_eq!(walk(&table(), 400, HOST, &bound), Err(Refused::Outside));
        // The host itself, the person's shell, init.
        assert_eq!(walk(&table(), HOST, HOST, &bound), Err(Refused::Outside));
        assert_eq!(walk(&table(), 5, HOST, &bound), Err(Refused::Outside));
        assert_eq!(walk(&table(), 1, HOST, &bound), Err(Refused::Outside));
    }

    #[test]
    fn an_unbound_run_admits_nobody() {
        assert_eq!(walk(&table(), 300, HOST, &[]), Err(Refused::Outside));
        assert_eq!(walk(&table(), 600, HOST, &[bound("run-a", 100, 50)]), Err(Refused::Outside));
    }

    /// The agent 100 exited and a later process took its pid: the binding
    /// names the old process (start 50), so neither the new one nor a child
    /// of it is the run.
    #[test]
    fn a_reused_pid_with_a_new_start_is_not_the_run() {
        let reused = Table::new(&[(1, 0, 1), (5, 1, 2), (HOST, 5, 3), (100, 5, 900), (901, 100, 910)]);
        let bound = [bound("run-a", 100, 50)];
        assert_eq!(walk(&reused, 100, HOST, &bound), Err(Refused::Outside));
        assert_eq!(walk(&reused, 901, HOST, &bound), Err(Refused::Outside));
    }

    /// The relay's real parent died and its pid went to a process started
    /// later: that process is not the relay's parent, whatever it says.
    #[test]
    fn a_parent_that_started_after_its_child_refuses() {
        let reused = Table::new(&[(1, 0, 1), (HOST, 1, 3), (100, HOST, 950), (300, 100, 70)]);
        assert_eq!(walk(&reused, 300, HOST, &[bound("run-a", 100, 950)]), Err(Refused::Reused));
    }

    #[test]
    fn a_gone_peer_or_ancestor_refuses() {
        let bound = [bound("run-a", 100, 50)];
        assert_eq!(walk(&table(), 777, HOST, &bound), Err(Refused::Gone));
        let broken = Table::new(&[(300, 200, 70)]);
        assert_eq!(walk(&broken, 300, HOST, &bound), Err(Refused::Gone));
    }

    #[test]
    fn a_tree_deeper_than_the_cap_refuses() {
        let mut rows = vec![(1, 0, 1), (HOST, 1, 2), (100, HOST, 3)];
        let depth = MAX_DEPTH as u32 + 5;
        for n in 0..depth {
            rows.push((1000 + n + 1, 1000 + n, 10));
        }
        rows.push((1000, 100, 4));
        let deep = Table::new(&rows);
        assert_eq!(walk(&deep, 1000 + depth, HOST, &[bound("run-a", 100, 3)]), Err(Refused::TooDeep));
        assert_eq!(walk(&deep, 1010, HOST, &[bound("run-a", 100, 3)]), Ok(run("run-a")));
    }

    #[test]
    fn proc_stat_parses_past_a_command_with_spaces_and_parentheses() {
        let line = "4242 (my (odd) cmd) S 4100 4242 4242 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 987654 1000 10 18446744073709551615";
        assert_eq!(parse_stat(line), Some(Stat { parent: 4100, start: 987654 }));
        assert_eq!(parse_stat("1 (init) S"), None);
        assert_eq!(parse_stat("garbage"), None);
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn the_real_table_knows_this_process_and_its_parent() {
        let own = Os.stat(std::process::id()).expect("this process");
        assert_eq!(own.parent, std::os::unix::process::parent_id());
        let parent = Os.stat(own.parent).expect("its parent");
        assert!(parent.start <= own.start);
    }
}
