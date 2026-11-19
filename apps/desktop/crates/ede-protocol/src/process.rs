//! #EDE-127: a process as `/proc` tells it, for the question whether it
//! belongs to an agent space. The compositor answers it for
//! [`crate::Request::SpaceOfProcess`] with what it launched and what
//! connected to a space's socket; eDE's polkit agent also looks at the
//! environments itself, so a request from a space is refused when the
//! compositor cannot be asked. Elsewhere than Linux there is no `/proc`:
//! every answer is "nothing".

use std::path::Path;

use crate::SpaceId;

/// How far up from a process its lineage is followed.
pub const MAX_LINEAGE: usize = 64;

/// The two variables a launch into a space gets from the compositor that
/// name the space's own directory.
const SPACE_VARS: [&str; 2] = ["XDG_RUNTIME_DIR", "WAYLAND_DISPLAY"];

/// A process's place among the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stat {
    pub pid: u32,
    pub ppid: u32,
    /// Its process group.
    pub pgid: u32,
}

/// `/proc/<pid>/stat`; `None` when there is no such process.
pub fn stat(pid: u32) -> Option<Stat> {
    parse_stat(pid, &std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// The fields after the command name, which may hold spaces and parentheses
/// of its own: `pid (comm) state ppid pgrp …`.
pub fn parse_stat(pid: u32, text: &str) -> Option<Stat> {
    let mut fields = text.rsplit_once(')')?.1.split_whitespace();
    let _state = fields.next()?;
    let ppid = fields.next()?.parse().ok()?;
    let pgid = fields.next()?.parse().ok()?;
    Some(Stat { pid, ppid, pgid })
}

/// `pid` and the processes it descends from, nearest first, up to pid 1
/// (not included) and at most [`MAX_LINEAGE`]; empty when it is gone.
pub fn lineage(pid: u32) -> Vec<Stat> {
    let mut line: Vec<Stat> = Vec::new();
    let mut next = pid;
    while next > 1 && line.len() < MAX_LINEAGE && !line.iter().any(|s| s.pid == next) {
        let Some(stat) = stat(next) else { break };
        next = stat.ppid;
        line.push(stat);
    }
    line
}

/// The variables `names` of `pid`'s environment as it started
/// (`/proc/<pid>/environ`); empty when it cannot be read (another user's
/// process, or one that is not dumpable).
pub fn environment(pid: u32, names: &[&str]) -> Vec<(String, String)> {
    let Ok(bytes) = std::fs::read(format!("/proc/{pid}/environ")) else { return Vec::new() };
    bytes
        .split(|b| *b == 0)
        .filter_map(|entry| std::str::from_utf8(entry).ok()?.split_once('='))
        .filter(|(name, _)| names.contains(name))
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

/// The space whose directory `path` is in: `<spaces>/space-n/…`, where
/// `spaces` is a compositor's directory of spaces ([`crate::spaces_path`]).
pub fn space_in(path: &Path, spaces: &Path) -> Option<SpaceId> {
    let first = path.strip_prefix(spaces).ok()?.components().next()?;
    let std::path::Component::Normal(name) = first else { return None };
    let name = name.to_str()?;
    name.strip_prefix("space-").filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))?;
    SpaceId::new(name).ok()
}

/// The space `pid`'s own environment points into: its runtime directory
/// or its Wayland socket in the space's directory under `spaces`.
pub fn space_in_environment(pid: u32, spaces: &Path) -> Option<SpaceId> {
    environment(pid, &SPACE_VARS).into_iter().find_map(|(_, value)| space_in(Path::new(&value), spaces))
}

/// The space `pid`'s environment, or the environment of a process it
/// descends from, points into ([`space_in_environment`]), with the process
/// that says so: what a client can tell without the compositor.
pub fn space_by_environment(pid: u32, spaces: &Path) -> Option<(SpaceId, u32)> {
    lineage(pid).into_iter().find_map(|stat| space_in_environment(stat.pid, spaces).map(|space| (space, stat.pid)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn a_stat_line_whatever_the_command_name_holds() {
        assert_eq!(parse_stat(42, "42 (sleep) S 7 42 7 0 -1"), Some(Stat { pid: 42, ppid: 7, pgid: 42 }));
        assert_eq!(parse_stat(42, "42 (a) b (c) S 7 9 7 0 -1"), Some(Stat { pid: 42, ppid: 7, pgid: 9 }), "parentheses and spaces in the name");
        assert_eq!(parse_stat(42, "42 (x)"), None);
        assert_eq!(parse_stat(42, ""), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_lineage_runs_from_the_process_up_to_init() {
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        let line = lineage(child.id());
        assert_eq!(line[0].pid, child.id());
        assert_eq!(line[1].pid, std::process::id(), "its parent next");
        assert!(line.iter().all(|s| s.pid > 1) && line.len() <= MAX_LINEAGE);
        let _ = child.kill();
        let _ = child.wait();
        assert!(lineage(u32::MAX).is_empty(), "no such process");
        assert!(lineage(1).is_empty() && lineage(0).is_empty());
    }

    #[test]
    fn a_path_in_a_spaces_directory_names_its_space() {
        let spaces = Path::new("/run/user/1000/exponential-desktop/wayland-1.spaces");
        let space = |p: &str| space_in(Path::new(p), spaces).map(|s| s.as_str().to_string());
        assert_eq!(space("/run/user/1000/exponential-desktop/wayland-1.spaces/space-3"), Some("space-3".into()));
        assert_eq!(space("/run/user/1000/exponential-desktop/wayland-1.spaces/space-12/wayland-0"), Some("space-12".into()));
        assert_eq!(space("/run/user/1000/exponential-desktop/wayland-1.spaces/space-2/home/config"), Some("space-2".into()));
        assert_eq!(space("/run/user/1000"), None, "the person's runtime directory");
        assert_eq!(space("wayland-1"), None, "a relative socket name");
        assert_eq!(space("/run/user/1000/exponential-desktop/wayland-1.spaces"), None);
        assert_eq!(space("/run/user/1000/exponential-desktop/wayland-1.spaces/../wayland-1.sock"), None);
        assert_eq!(space("/run/user/1000/exponential-desktop/wayland-1.spaces/space-"), None);
        assert_eq!(space("/run/user/1000/exponential-desktop/wayland-1.spaces/spacey"), None);
        assert_eq!(space("/run/user/1000/exponential-desktop/wayland-2.spaces/space-3"), None, "another compositor's");
    }

    /// A process started with a space's runtime directory is the space's, and
    /// so is what it starts with an environment of its own; the person's
    /// environment is nobody's space.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_environment_of_a_process_or_its_ancestors_names_its_space() {
        let spaces = std::env::temp_dir().join(format!("ede-protocol-{}.spaces", std::process::id()));
        let dir = spaces.join("space-4");
        let mut shell = Command::new("sh")
            .args(["-c", "env -i sleep 30 & echo $!; wait"])
            .env("XDG_RUNTIME_DIR", &dir)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(shell.stdout.as_mut().unwrap()), &mut line).unwrap();
        let cleared: u32 = line.trim().parse().unwrap();
        // `$!` comes before the fork has run `env`, and `env` before `sleep`.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::fs::read_to_string(format!("/proc/{cleared}/comm")).map_or(true, |comm| comm.trim() != "sleep") {
            assert!(std::time::Instant::now() < deadline, "the cleared child never ran sleep");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(space_in_environment(shell.id(), &spaces).map(|s| s.as_str().to_string()), Some("space-4".into()));
        assert_eq!(space_in_environment(cleared, &spaces), None, "its own environment is empty");
        assert_eq!(space_by_environment(cleared, &spaces).map(|(s, pid)| (s.as_str().to_string(), pid)), Some(("space-4".into(), shell.id())), "its parent's names the space");
        let mut person = Command::new("sleep").arg("30").env("XDG_RUNTIME_DIR", "/run/user/1000").env("WAYLAND_DISPLAY", "wayland-1").spawn().unwrap();
        assert_eq!(space_by_environment(person.id(), &spaces), None);
        for child in [&mut shell, &mut person] {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = Command::new("kill").arg(cleared.to_string()).status();
    }
}
