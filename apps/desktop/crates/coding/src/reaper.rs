//! Reap agent processes that escaped the PTY (EXP-300).
//!
//! ## The bug
//!
//! Quitting the app left it "running in the background" and made the next
//! launch do nothing. The main process was NOT hung — it exited cleanly. What
//! survived was its **Launch Services registration**:
//!
//! ```text
//! "Exponential" ASN:0x0-0x6c06c  pid = 3129 ... (exited-with-subordinates)
//! coalition: 1720  { 3921 3937 3938 3956 3995 }
//! ```
//!
//! macOS keeps an app's ASN registered while its **coalition** still has live
//! members. Those five were all `claude` processes spawned by a coding
//! session. Because the ASN stays registered, the next launch is delivered to
//! the dead instance as a re-open instead of starting a new process — the
//! reported "you have to launch it twice".
//!
//! Killing the survivors releases the registration immediately (verified: the
//! entry vanished the moment the session leader died, and the very next launch
//! opened normally).
//!
//! ## Why the obvious fixes do not work
//!
//! `Terminal::shutdown` already kills the PTY child. That is not enough:
//! `claude` spawns a background daemon that `setsid`s into its own session, so
//! it is neither in the child's process group (no `killpg`) nor in our process
//! tree by the time we look (`ppid == 1` — already reparented to launchd, so
//! walking DOWN from our child finds nothing).
//!
//! ## Selection rule
//!
//! Anchored on `<data_dir>/claude-hooks/`, a directory only this app ever
//! writes. A process carrying that path in its argv is definitionally ours,
//! so a user's own `claude` session — even one running inside the same repo —
//! can never match. From those seeds:
//!
//! - **down**: every descendant (the daemon's own children carry no marker of
//!   their own — `claude bg-pty-host`, `claude bg-spare`).
//! - **up**: ancestors that are ALREADY ORPHANED (`ppid == 1`). This is the
//!   case that matters and the one a tree-walk misses: the marker-carrying
//!   processes were *children* of the detached daemon, and that daemon is what
//!   pinned the coalition. Orphaned-ness is what makes walking up safe — a
//!   live parent (a shell, the terminal app, us) never has `ppid == 1`, so the
//!   walk cannot climb out into processes we do not own.
//!
//! Never selects our own pid.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

/// One row of the process table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proc {
    pub pid: i32,
    pub ppid: i32,
    pub command: String,
}

/// The marker directory: `<data_dir>/claude-hooks`. Written only by
/// [`crate::launcher`], which is what makes it a safe anchor.
pub fn hook_marker(data_dir: &Path) -> String {
    data_dir.join("claude-hooks").to_string_lossy().into_owned()
}

/// Pick the processes to kill. Pure — [`reap`] supplies the process table and
/// does the signalling, so the whole selection rule is testable.
pub fn select(procs: &[Proc], marker: &str, self_pid: i32) -> Vec<i32> {
    let by_pid: HashMap<i32, &Proc> = procs.iter().map(|p| (p.pid, p)).collect();
    let mut children: HashMap<i32, Vec<i32>> = HashMap::new();
    for p in procs {
        children.entry(p.ppid).or_default().push(p.pid);
    }

    let mut selected: BTreeSet<i32> = BTreeSet::new();

    // Seeds: processes carrying the app-owned marker path.
    let seeds: Vec<i32> = procs
        .iter()
        .filter(|p| p.pid != self_pid && p.command.contains(marker))
        .map(|p| p.pid)
        .collect();

    // Up: orphaned ancestors (the detached daemon). Bounded by the `ppid == 1`
    // requirement — the climb stops the moment a parent is still attached.
    for &seed in &seeds {
        let mut cur = seed;
        while let Some(proc) = by_pid.get(&cur) {
            let parent = proc.ppid;
            if parent <= 1 || parent == self_pid {
                break;
            }
            match by_pid.get(&parent) {
                Some(p) if p.ppid == 1 => {
                    selected.insert(parent);
                    cur = parent;
                }
                _ => break,
            }
        }
    }

    // Down: every descendant of everything selected so far (seeds included).
    let mut stack: Vec<i32> = seeds.iter().copied().chain(selected.iter().copied()).collect();
    selected.extend(seeds.iter().copied());
    while let Some(pid) = stack.pop() {
        for &child in children.get(&pid).map(Vec::as_slice).unwrap_or(&[]) {
            if child != self_pid && selected.insert(child) {
                stack.push(child);
            }
        }
    }

    selected.remove(&self_pid);
    selected.into_iter().collect()
}

/// Read the process table via `ps`. `None` when it cannot be read — a failed
/// reap must never block a quit.
#[cfg(unix)]
fn process_table() -> Option<Vec<Proc>> {
    let out = std::process::Command::new("ps")
        .args(["-ax", "-o", "pid=,ppid=,command="])
        .output()
        .ok()?;
    Some(parse_ps(&String::from_utf8_lossy(&out.stdout)))
}

/// Parse `ps -ax -o pid=,ppid=,command=`.
pub fn parse_ps(text: &str) -> Vec<Proc> {
    text.lines()
        .filter_map(|line| {
            // Columns are right-aligned and padded, so split on the FIRST
            // whitespace run twice and keep the remainder verbatim — the
            // command contains spaces and must survive intact.
            let (pid, rest) = line.trim_start().split_once(char::is_whitespace)?;
            let (ppid, command) = rest.trim_start().split_once(char::is_whitespace)?;
            Some(Proc {
                pid: pid.parse().ok()?,
                ppid: ppid.parse().ok()?,
                command: command.trim_start().to_string(),
            })
        })
        .collect()
}

/// Terminate every agent process this app spawned that outlived its PTY.
///
/// SIGTERM, a short grace, then SIGKILL the holdouts — `claude bg-pty-host`
/// and the ClaudeCode helper were both observed to ignore SIGTERM. Bounded
/// and best-effort: this runs on the quit path, so it must never be the thing
/// that hangs a quit.
#[cfg(unix)]
pub fn reap(data_dir: &Path) -> usize {
    let marker = hook_marker(data_dir);
    let Some(procs) = process_table() else {
        return 0;
    };
    let self_pid = std::process::id() as i32;
    let targets = select(&procs, &marker, self_pid);
    if targets.is_empty() {
        return 0;
    }
    log::info!(
        "reaping {} escaped agent process(es) holding the app coalition open: {targets:?}",
        targets.len()
    );
    for &pid in &targets {
        // SAFETY: `kill` on a pid we selected; an invalid pid is an errno, not UB.
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
    std::thread::sleep(std::time::Duration::from_millis(300));
    for &pid in &targets {
        // SAFETY: as above. ESRCH (already gone) is the expected common case.
        unsafe { libc::kill(pid, libc::SIGKILL) };
    }
    targets.len()
}

#[cfg(not(unix))]
pub fn reap(_data_dir: &Path) -> usize {
    // The coalition/ASN mechanic is macOS-only, and Windows has no equivalent
    // orphan-holds-the-registration behaviour.
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact topology captured from the reproduced bug. 3938/3995 carry
    /// the marker; 3921 is their ALREADY-ORPHANED parent and is what actually
    /// pinned the coalition; 3937/3956 are its unmarked children.
    fn observed() -> Vec<Proc> {
        let marker = "/Users/u/Library/Application Support/at.exponential/claude-hooks";
        vec![
            Proc { pid: 1, ppid: 0, command: "/sbin/launchd".into() },
            Proc { pid: 3921, ppid: 1, command: "claude daemon run --origin transient".into() },
            Proc { pid: 3937, ppid: 3921, command: "claude bg-pty-host".into() },
            Proc {
                pid: 3938,
                ppid: 3921,
                command: format!("claude --bg-pty-host --settings {marker}/b98.settings.json"),
            },
            Proc { pid: 3956, ppid: 3937, command: "claude bg-spare".into() },
            Proc {
                pid: 3995,
                ppid: 3938,
                command: format!("claude 2.1.220 --settings {marker}/b98.settings.json"),
            },
        ]
    }

    const MARKER: &str = "/Users/u/Library/Application Support/at.exponential/claude-hooks";

    #[test]
    fn selects_the_whole_escaped_tree_including_the_orphaned_daemon() {
        let got = select(&observed(), MARKER, 999);
        assert_eq!(got, vec![3921, 3937, 3938, 3956, 3995]);
    }

    /// The regression that matters: a tree-walk that only goes DOWN from the
    /// marked processes misses 3921 — the one whose death released the ASN.
    #[test]
    fn includes_the_orphaned_parent_not_just_descendants() {
        assert!(select(&observed(), MARKER, 999).contains(&3921));
    }

    /// A user's own `claude`, even in the same repo, carries no app-written
    /// path and must survive.
    #[test]
    fn never_touches_a_users_own_session() {
        let mut procs = observed();
        procs.push(Proc {
            pid: 7000,
            ppid: 6000,
            command: "claude --resume /Users/u/Exponential/repos/Niach/exponential".into(),
        });
        procs.push(Proc { pid: 6000, ppid: 500, command: "-fish".into() });
        let got = select(&procs, MARKER, 999);
        assert!(!got.contains(&7000), "killed a user session: {got:?}");
        assert!(!got.contains(&6000), "killed a user shell: {got:?}");
    }

    /// The upward walk must stop at an ATTACHED parent, or it would climb out
    /// of the processes we own and into the terminal that launched us.
    #[test]
    fn does_not_climb_through_a_live_parent() {
        let procs = vec![
            Proc { pid: 1, ppid: 0, command: "/sbin/launchd".into() },
            Proc { pid: 400, ppid: 1, command: "/Applications/Ghostty.app".into() },
            Proc { pid: 500, ppid: 400, command: "-fish".into() },
            Proc {
                pid: 600,
                ppid: 500,
                command: format!("claude --settings {MARKER}/x.settings.json"),
            },
        ];
        let got = select(&procs, MARKER, 999);
        assert_eq!(got, vec![600], "climbed into the shell/terminal: {got:?}");
    }

    #[test]
    fn never_selects_self() {
        let procs = vec![Proc {
            pid: 42,
            ppid: 1,
            command: format!("exp-desktop --settings {MARKER}/x.settings.json"),
        }];
        assert!(select(&procs, MARKER, 42).is_empty());
    }

    #[test]
    fn parses_ps_output() {
        let got = parse_ps("  3921     1 /usr/bin/claude daemon run --origin transient\n 1 0 /sbin/launchd\n");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].pid, 3921);
        assert_eq!(got[0].ppid, 1);
        assert_eq!(got[0].command, "/usr/bin/claude daemon run --origin transient");
        assert_eq!(got[1].pid, 1);
    }
}
