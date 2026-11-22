//! #EDE-127: a process as `/proc` tells it, for the question whether it
//! belongs to an agent space. The compositor answers it for
//! [`crate::Request::SpaceOfProcess`] with what it launched and what
//! connected to a space's socket; eDE's polkit agent also looks at the
//! environments itself, so a request from a space is refused when the
//! compositor cannot be asked. Elsewhere than Linux there is no `/proc`:
//! every answer is "nothing".
//!
//! #EDE-138: what a space launches runs in a systemd scope of the space's
//! own slice, and its cgroup says so first ([`space_by_cgroup`]): a
//! process keeps its cgroup when it starts a session of its own, empties
//! its environment and lets its parent die. It is a sign, not a wall:
//! every cgroup under the user manager belongs to the user, so a process
//! that works at it can move itself to another one.

use std::path::Path;

use crate::SpaceId;

/// How far up from a process its lineage is followed.
pub const MAX_LINEAGE: usize = 64;

/// The two variables a launch into a space gets from the compositor that
/// name the space's own directory.
const SPACE_VARS: [&str; 2] = ["XDG_RUNTIME_DIR", "WAYLAND_DISPLAY"];

/// The slice of all agent spaces of the compositor whose process id is
/// `compositor`, in the person's user manager. The id is in every unit
/// name: two compositors of one user never share one, and what a crashed
/// compositor left is told apart from a running one's. A `-` in a slice's
/// name is a level, so this one is a child of `app.slice` itself (no
/// parent of its own is left behind when it stops).
pub fn spaces_slice(compositor: u32) -> String {
    format!("app-exponential_p{compositor}.slice")
}

/// The slice of that compositor's space number `space`, a child of
/// [`spaces_slice`].
pub fn space_slice(compositor: u32, space: u32) -> String {
    format!("app-exponential_p{compositor}-s{space}.slice")
}

/// The scope of launch number `launch` into that space.
pub fn launch_scope(compositor: u32, space: u32, launch: u32) -> String {
    format!("exponential-p{compositor}-s{space}-{launch}.scope")
}

/// A [`space_slice`] name read back: the compositor's process id and the
/// space's number.
pub fn parse_space_slice(unit: &str) -> Option<(u32, u32)> {
    let (compositor, space) = unit.strip_prefix("app-exponential_p")?.strip_suffix(".slice")?.split_once("-s")?;
    let number = |text: &str| (!text.is_empty() && text.len() <= 10 && text.bytes().all(|b| b.is_ascii_digit())).then(|| text.parse::<u32>().ok()).flatten();
    Some((number(compositor)?, number(space)?))
}

/// The space slice a cgroup path runs through
/// (`…/app-exponential_p41.slice/app-exponential_p41-s3.slice/…`): the
/// compositor's process id and the space, `None` for a path of the
/// person's. `cgroup` is `/proc/<pid>/cgroup` as it is (every hierarchy's
/// line is looked at) or a bare path.
pub fn space_in_cgroup(cgroup: &str) -> Option<(u32, SpaceId)> {
    cgroup.lines().flat_map(|line| line.rsplit(':').next().unwrap_or(line).split('/')).find_map(|unit| {
        let (compositor, space) = parse_space_slice(unit)?;
        Some((compositor, SpaceId::new(&format!("space-{space}")).ok()?))
    })
}

/// `/proc/<pid>/cgroup`; `None` when there is no such process.
pub fn cgroup(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/cgroup")).ok()
}

/// The space whose slice `pid`, or a process it descends from, runs in
/// ([`space_in_cgroup`]): the compositor's process id, the space and the
/// process that says so.
pub fn space_by_cgroup(pid: u32) -> Option<(u32, SpaceId, u32)> {
    space_by_cgroup_with(pid, cgroup)
}

/// [`space_by_cgroup`] with the cgroups read through `cgroup_of` (a test's).
pub fn space_by_cgroup_with(pid: u32, cgroup_of: impl Fn(u32) -> Option<String>) -> Option<(u32, SpaceId, u32)> {
    lineage(pid).into_iter().find_map(|stat| space_in_cgroup(&cgroup_of(stat.pid)?).map(|(compositor, space)| (compositor, space, stat.pid)))
}

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
    #[cfg(target_os = "linux")]
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

    /// #EDE-138: the unit names and what reads them back are one pair.
    #[test]
    fn a_cgroup_under_a_spaces_slice_names_its_space() {
        assert_eq!(spaces_slice(41), "app-exponential_p41.slice");
        assert_eq!(space_slice(41, 3), "app-exponential_p41-s3.slice");
        assert_eq!(launch_scope(41, 3, 7), "exponential-p41-s3-7.scope");
        assert_eq!(parse_space_slice(&space_slice(41, 3)), Some((41, 3)));
        for not_a_space in ["app-exponential_p41.slice", "app-exponential.slice", "app-exponential_p-s3.slice", "app-exponential_p41-s.slice", "app-exponential_p41-s3.scope", "app-exponential_p41-sx.slice", "app-exponential_p4x-s3.slice", "xapp-exponential_p41-s3.slice", "app-exponential_p41-s3-1.slice", "app-exponential_p99999999999-s1.slice", "app-exponential-p41-s3.slice"] {
            assert_eq!(parse_space_slice(not_a_space), None, "{not_a_space}");
        }
        let space = |cgroup: &str| space_in_cgroup(cgroup).map(|(compositor, space)| (compositor, space.as_str().to_string()));
        let user = "/user.slice/user-1000.slice/user@1000.service";
        let slices = "app.slice/app-exponential_p41.slice/app-exponential_p41-s3.slice";
        assert_eq!(space(&format!("0::{user}/{slices}/exponential-p41-s3-7.scope\n")), Some((41, "space-3".into())), "a launch");
        assert_eq!(space(&format!("0::{user}/{slices}/exponential-p41-s3-session.scope/app.slice/snap.firefox.firefox-1f.scope\n")), Some((41, "space-3".into())), "what the space's own manager started");
        assert_eq!(space(&format!("{user}/{slices}")), Some((41, "space-3".into())), "a bare path");
        assert_eq!(space(&format!("12:pids:/x\n1:name=systemd:{user}/{slices}/a.scope\n0::/\n")), Some((41, "space-3".into())), "a v1 hierarchy's line");
        assert_eq!(space("0::/user.slice/user-1000.slice/session-3.scope\n"), None, "the person's login session");
        assert_eq!(space(&format!("0::{user}/app.slice/app-exponential_p41.slice/x.scope\n")), None, "the slice of all spaces is no space");
        assert_eq!(space(&format!("0::{user}/app.slice/app-flatpak-org.x.Y-1234.scope\n")), None);
        assert_eq!(space(""), None);
    }

    /// A process and what it starts are in one cgroup: this test's own is
    /// nobody's space, and neither is a child's.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_process_of_the_person_is_in_no_spaces_slice() {
        assert!(cgroup(std::process::id()).is_some());
        assert_eq!(cgroup(u32::MAX), None, "no such process");
        let mut child = Command::new("sleep").arg("30").spawn().unwrap();
        assert_eq!(space_by_cgroup(child.id()), None);
        // The nearest process that is in a space's slice answers: here the
        // child's parent, this test.
        let own = std::process::id();
        let in_space = |pid: u32| (pid == own).then(|| "0::/user.slice/a/app-exponential_p9.slice/app-exponential_p9-s2.slice/x.scope\n".to_string()).or_else(|| cgroup(pid));
        assert_eq!(space_by_cgroup_with(child.id(), in_space).map(|(compositor, space, pid)| (compositor, space.as_str().to_string(), pid)), Some((9, "space-2".into(), own)));
        let _ = child.kill();
        let _ = child.wait();
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
        while !std::fs::read_to_string(format!("/proc/{cleared}/comm")).is_ok_and(|comm| comm.trim() == "sleep") {
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
