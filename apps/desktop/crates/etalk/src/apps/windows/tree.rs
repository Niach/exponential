//! Which audio session belongs to which app (pure: the process table is
//! passed in, so the rules are unit-tested without Windows' APIs).

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::super::Selection;
use super::pcm;

/// One row of the process table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Proc {
    pub ppid: u32,
    /// The exe's file stem, lowercased (the app id).
    pub stem: String,
}

/// `C:\Program Files\Mozilla Firefox\Firefox.EXE` → `firefox`.
pub(super) fn stem_of(exe: &str) -> String {
    let base = exe.rsplit(['\\', '/']).next().unwrap_or(exe);
    let base = base.trim_end_matches('\0').trim();
    let stem = match base.rfind('.') {
        Some(dot) if dot > 0 => &base[..dot],
        _ => base,
    };
    stem.to_lowercase()
}

/// The app's root process: the topmost ancestor running the same exe (a
/// browser's audio service → the browser), so the process-tree loopback
/// of the root covers every process of the app.
pub(super) fn root_of(pid: u32, procs: &HashMap<u32, Proc>) -> u32 {
    let Some(me) = procs.get(&pid) else { return pid };
    let mut root = pid;
    let mut seen = BTreeSet::from([pid]);
    while let Some(parent) = procs.get(&root).map(|p| p.ppid) {
        match procs.get(&parent) {
            Some(up) if up.stem == me.stem && parent != 0 && seen.insert(parent) => root = parent,
            _ => break,
        }
    }
    root
}

/// The processes to loop back, by app: the roots of every active session's
/// process the selection wants, never this executable (the IDE and its
/// speech worker) nor the system sounds (pid 0).
pub(super) fn plan(
    active: &[u32],
    procs: &HashMap<u32, Proc>,
    own_pid: u32,
    own_stem: Option<&str>,
    selection: &Selection,
) -> BTreeMap<String, BTreeSet<u32>> {
    let mut out: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    for &pid in active {
        if pid == 0 || pid == own_pid {
            continue;
        }
        let Some(proc_) = procs.get(&pid) else { continue };
        if proc_.stem.is_empty() || own_stem.is_some_and(|own| pcm::same_binary(own, &proc_.stem)) {
            continue;
        }
        if !selection.wants(&proc_.stem) {
            continue;
        }
        out.entry(proc_.stem.clone()).or_default().insert(root_of(pid, procs));
    }
    out
}

/// The name an app is shown by: its file description, else its stem.
pub(super) fn display_name(description: Option<&str>, stem: &str) -> String {
    match description.map(str::trim).filter(|d| !d.is_empty()) {
        Some(description) => description.to_string(),
        None => stem.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(rows: &[(u32, u32, &str)]) -> HashMap<u32, Proc> {
        rows.iter().map(|(pid, ppid, stem)| (*pid, Proc { ppid: *ppid, stem: stem.to_string() })).collect()
    }

    #[test]
    fn stems_are_lowercased_file_stems() {
        assert_eq!(stem_of("C:\\Program Files\\Mozilla Firefox\\Firefox.EXE"), "firefox");
        assert_eq!(stem_of("Zoom.exe"), "zoom");
        assert_eq!(stem_of("spotify"), "spotify");
        assert_eq!(stem_of("C:\\x\\.hidden"), ".hidden");
    }

    #[test]
    fn root_climbs_while_the_exe_matches() {
        let procs = table(&[(1, 0, "explorer"), (10, 1, "chrome"), (11, 10, "chrome"), (12, 11, "chrome"), (20, 1, "zoom")]);
        assert_eq!(root_of(12, &procs), 10);
        assert_eq!(root_of(20, &procs), 20);
        assert_eq!(root_of(99, &procs), 99, "unknown pid = itself");
    }

    #[test]
    fn root_survives_a_parent_cycle() {
        let procs = table(&[(5, 6, "app"), (6, 5, "app")]);
        let root = root_of(5, &procs);
        assert!(root == 5 || root == 6);
    }

    #[test]
    fn plan_groups_roots_by_app_and_follows_the_selection() {
        let procs = table(&[
            (1, 0, "explorer"),
            (10, 1, "chrome"),
            (12, 10, "chrome"),
            (13, 10, "chrome"),
            (20, 1, "zoom"),
            (30, 1, "exponential"),
            (31, 30, "exponential"),
        ]);
        let active = [12, 13, 20, 0, 31, 30];
        let all = plan(&active, &procs, 30, Some("exponential"), &Selection::All);
        assert_eq!(all.keys().collect::<Vec<_>>(), vec!["chrome", "zoom"]);
        assert_eq!(all["chrome"], BTreeSet::from([10]));
        assert_eq!(all["zoom"], BTreeSet::from([20]));
        let only = plan(&active, &procs, 30, Some("exponential"), &Selection::Include(vec!["zoom".into()]));
        assert_eq!(only.keys().collect::<Vec<_>>(), vec!["zoom"]);
        let but = plan(&active, &procs, 30, Some("exponential"), &Selection::Exclude(vec!["zoom".into()]));
        assert_eq!(but.keys().collect::<Vec<_>>(), vec!["chrome"]);
    }

    #[test]
    fn names_prefer_the_file_description() {
        assert_eq!(display_name(Some("Google Chrome"), "chrome"), "Google Chrome");
        assert_eq!(display_name(Some("  "), "chrome"), "chrome");
        assert_eq!(display_name(None, "zoom"), "zoom");
    }
}
