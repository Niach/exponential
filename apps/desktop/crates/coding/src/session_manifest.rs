//! #EXP-1868: what a session created OUTSIDE its cgroup scope, deleted when it
//! ends.
//!
//! The scope stop ([`crate::session_teardown`]) takes every process a session
//! started, but not what those processes left on disk or in other daemons: an
//! iOS simulator (~30 GB each; the MacBook went 41 GB → 0 in 5 h on
//! 2026-11-16, one simulator per capture session), an Android AVD, a
//! DerivedData or `/tmp` build dir, a compose project and its volumes. Those
//! are listed by whoever creates them (`scripts/lanes/session-manifest.sh add
//! <kind> <value>`, the Fastfile, the capture scripts) in ONE file per session,
//! `~/.local/state/exponential/session-manifests/<session id>`, one
//! `<kind>\t<value>` line each, and [`clean`] deletes exactly those when the
//! session ends or the daemon's sweep reaps it after a crash. No guessing by
//! name. Same on macOS and Linux.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Overrides [`manifest_dir`] (tests, isolated setups).
pub const MANIFEST_DIR_ENV: &str = "EXP_SESSION_MANIFEST_DIR";

/// One thing a session created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// An iOS simulator UDID: `xcrun simctl delete`.
    Simulator(String),
    /// An Android AVD name: `avdmanager delete avd`, else its files.
    Avd(String),
    /// A directory: `rm -rf`, only under [`dir_allowed`]'s roots.
    Dir(PathBuf),
    /// A compose project: `docker compose -p <name> down --volumes`.
    Compose(String),
}

/// `~/.local/state/exponential/session-manifests` (XDG_STATE_HOME honoured),
/// the same dir `scripts/lanes/session-manifest.sh` writes.
pub fn manifest_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(MANIFEST_DIR_ENV).filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let state = std::env::var_os("XDG_STATE_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))?;
    Some(state.join("exponential/session-manifests"))
}

fn valid_session(session: &str) -> bool {
    !session.is_empty() && session.len() <= 64 && session.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// The sessions that have a manifest in `dir`.
pub fn sessions_with_manifests(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut sessions: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| valid_session(name))
        .collect();
    sessions.sort();
    sessions
}

/// Append a `dir` entry to `session`'s manifest in `dir` (once; best-effort).
/// The Rust twin of `scripts/lanes/session-manifest.sh add dir <path>`.
pub fn record_dir(dir: &Path, session: &str, path: &Path) {
    if !valid_session(session) || !path.is_absolute() {
        return;
    }
    let file = dir.join(session);
    let line = format!("dir\t{}", path.display());
    if std::fs::read_to_string(&file).is_ok_and(|text| text.lines().any(|existing| existing == line)) {
        return;
    }
    let appended = std::fs::create_dir_all(dir).and_then(|()| {
        use std::io::Write as _;
        let mut out = std::fs::OpenOptions::new().create(true).append(true).open(&file)?;
        writeln!(out, "{line}")
    });
    if let Err(err) = appended {
        log::warn!("session manifest [{session}]: could not record {}: {err:#}", path.display());
    }
}

/// A manifest's entries in order, each once. Unknown kinds and malformed lines
/// are skipped (a newer writer's kind is no reason to keep the rest).
pub fn parse(text: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for line in text.lines() {
        let Some((kind, value)) = line.split_once('\t') else { continue };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }
        let entry = match kind {
            "sim" if valid_udid(value) => Entry::Simulator(value.to_string()),
            "avd" if valid_name(value) => Entry::Avd(value.to_string()),
            "dir" if value.starts_with('/') => Entry::Dir(PathBuf::from(value)),
            "compose" if valid_name(value) => Entry::Compose(value.to_string()),
            _ => continue,
        };
        if !entries.contains(&entry) {
            entries.push(entry);
        }
    }
    entries
}

fn valid_udid(value: &str) -> bool {
    value.len() == 36 && value.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

/// An AVD or compose project name: letters, digits, `_`, `-`, `.` (never a path).
fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.starts_with('.')
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
}

/// May a manifest's `dir` be deleted? Only strictly INSIDE one of `roots` (a
/// temp dir, Xcode's DerivedData, fastlane's caches, the session's worktree), after `..` and
/// symlinks are resolved: never a root itself, a home, a repo or `/`.
pub fn dir_allowed(dir: &Path, roots: &[PathBuf]) -> bool {
    let Ok(dir) = std::fs::canonicalize(dir) else { return false };
    roots.iter().filter_map(|root| std::fs::canonicalize(root).ok()).any(|root| dir != root && dir.starts_with(&root))
}

/// The roots a session's `dir` entries may live under on this machine.
pub fn dir_roots(worktree: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = vec![std::env::temp_dir(), PathBuf::from("/tmp"), PathBuf::from("/var/tmp")];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        roots.push(home.join("Library/Developer/Xcode/DerivedData"));
        // The Fastfile's per-tag snapshot caches (`exp-<tag>-sg`).
        roots.push(home.join("Library/Caches/tools.fastlane"));
    }
    if let Some(worktree) = worktree.filter(|worktree| crate::session_teardown::is_session_worktree(worktree)) {
        roots.push(worktree.to_path_buf());
    }
    roots
}

/// What one entry's cleanup runs (pure; `None` = refused or nothing to run).
/// `Dir` and an `Avd` without `avdmanager` are file removals, not commands.
pub fn command_for(entry: &Entry) -> Option<Vec<String>> {
    let argv = |args: &[&str]| Some(args.iter().map(|arg| arg.to_string()).collect());
    match entry {
        Entry::Simulator(udid) => argv(&["xcrun", "simctl", "delete", udid]),
        Entry::Avd(name) => argv(&["avdmanager", "delete", "avd", "-n", name]),
        Entry::Compose(project) => {
            argv(&["docker", "compose", "-p", project, "down", "--volumes", "--remove-orphans", "--timeout", "10"])
        }
        Entry::Dir(_) => None,
    }
}

/// How one entry went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Removed,
    /// Already gone (nothing to do).
    Gone,
    /// Refused (a `dir` outside the allowed roots) or failed: kept on record.
    Kept(String),
}

/// Deletes every entry of `session`'s manifest under `dir`, then the manifest
/// itself. Entries that failed are logged, never retried for ever: the
/// manifest goes either way (a simulator that cannot be deleted now will not
/// be deletable by a later sweep either). Blocking.
pub fn clean(dir: &Path, session: &str, worktree: Option<&Path>) -> Vec<(Entry, Outcome)> {
    if !valid_session(session) {
        return Vec::new();
    }
    let path = dir.join(session);
    let Ok(text) = std::fs::read_to_string(&path) else { return Vec::new() };
    let roots = dir_roots(worktree);
    let results: Vec<(Entry, Outcome)> =
        parse(&text).into_iter().map(|entry| {
            let outcome = clean_entry(&entry, &roots);
            log::info!("session manifest [{session}]: {entry:?} → {outcome:?}");
            (entry, outcome)
        }).collect();
    let _ = std::fs::remove_file(&path);
    results
}

fn clean_entry(entry: &Entry, roots: &[PathBuf]) -> Outcome {
    match entry {
        Entry::Dir(dir) => {
            if !dir.exists() {
                return Outcome::Gone;
            }
            if !dir_allowed(dir, roots) {
                return Outcome::Kept(format!("{} is outside the temp, DerivedData and worktree roots", dir.display()));
            }
            match std::fs::remove_dir_all(dir) {
                Ok(()) => Outcome::Removed,
                Err(err) => Outcome::Kept(err.to_string()),
            }
        }
        Entry::Avd(name) => {
            let ran = command_for(entry).and_then(|argv| run(&argv));
            if ran == Some(true) {
                return Outcome::Removed;
            }
            remove_avd_files(name)
        }
        Entry::Simulator(_) if !cfg!(target_os = "macos") => Outcome::Kept("no simctl here".to_string()),
        _ => match command_for(entry).and_then(|argv| run(&argv)) {
            Some(true) => Outcome::Removed,
            Some(false) => Outcome::Kept("the command failed".to_string()),
            None => Outcome::Kept("the command did not run".to_string()),
        },
    }
}

/// `avdmanager` missing or failing: the AVD's `<name>.avd` dir and `<name>.ini`
/// under `ANDROID_AVD_HOME` (else `~/.android/avd`).
fn remove_avd_files(name: &str) -> Outcome {
    let home = std::env::var_os("ANDROID_AVD_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".android/avd")));
    let Some(home) = home else { return Outcome::Kept("no AVD home".to_string()) };
    let avd = home.join(format!("{name}.avd"));
    let ini = home.join(format!("{name}.ini"));
    if !avd.exists() && !ini.exists() {
        return Outcome::Gone;
    }
    let dir = std::fs::remove_dir_all(&avd);
    let _ = std::fs::remove_file(&ini);
    match dir {
        Ok(()) => Outcome::Removed,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Outcome::Removed,
        Err(err) => Outcome::Kept(err.to_string()),
    }
}

/// Runs `argv` quietly, bounded: `Some(success)`, `None` = never started.
fn run(argv: &[String]) -> Option<bool> {
    let (program, args) = argv.split_first()?;
    let mut child = terminal::process::background_command(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(180);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.success()),
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Some(false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("exp-manifest-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_manifest_parses_each_entry_once_and_skips_what_it_cannot_trust() {
        let text = "sim\tA1B2C3D4-0000-4000-8000-00000000E868\n\
                    sim\tA1B2C3D4-0000-4000-8000-00000000E868\n\
                    sim\tnot-a-udid\n\
                    avd\texp_s339_pixel\n\
                    avd\t../../.ssh\n\
                    dir\t/tmp/s339-dd\n\
                    dir\trelative/dir\n\
                    compose\tcap9122\n\
                    compose\tbad name\n\
                    volume\tsomething-newer\n\
                    no tab here\n";
        assert_eq!(
            parse(text),
            vec![
                Entry::Simulator("A1B2C3D4-0000-4000-8000-00000000E868".into()),
                Entry::Avd("exp_s339_pixel".into()),
                Entry::Dir("/tmp/s339-dd".into()),
                Entry::Compose("cap9122".into()),
            ]
        );
    }

    #[test]
    fn a_recorded_dir_is_written_once_and_cleaned_with_the_session() {
        let manifests = temp("record");
        let xdg = std::env::temp_dir().join(format!("exp-manifest-record-xdg-{}", std::process::id()));
        std::fs::create_dir_all(xdg.join("data")).unwrap();
        record_dir(&manifests, "s1875", &xdg);
        record_dir(&manifests, "s1875", &xdg);
        record_dir(&manifests, "../bad", &xdg);
        assert_eq!(parse(&std::fs::read_to_string(manifests.join("s1875")).unwrap()), vec![Entry::Dir(xdg.clone())]);
        assert_eq!(std::fs::read_dir(&manifests).unwrap().count(), 1);
        clean(&manifests, "s1875", None);
        assert!(!xdg.exists());
        let _ = std::fs::remove_dir_all(&manifests);
    }

    #[test]
    fn each_kind_runs_its_own_delete() {
        let argv = |entry: Entry| command_for(&entry).unwrap().join(" ");
        assert_eq!(argv(Entry::Simulator("U".into())), "xcrun simctl delete U");
        assert_eq!(argv(Entry::Avd("a".into())), "avdmanager delete avd -n a");
        assert_eq!(argv(Entry::Compose("c".into())), "docker compose -p c down --volumes --remove-orphans --timeout 10");
        assert_eq!(command_for(&Entry::Dir("/tmp/x".into())), None);
    }

    #[test]
    fn a_dir_goes_only_from_inside_an_allowed_root() {
        let root = temp("roots");
        let inside = root.join("s339-dd");
        std::fs::create_dir_all(inside.join("Build")).unwrap();
        let roots = vec![root.clone()];
        assert!(dir_allowed(&inside, &roots));
        assert!(!dir_allowed(&root, &roots), "never a root itself");
        assert!(!dir_allowed(&inside.join("../.."), &roots), "`..` resolves out of it");
        assert!(!dir_allowed(Path::new("/"), &roots));
        #[cfg(unix)]
        {
            let link = root.join("link-out");
            std::os::unix::fs::symlink("/", &link).unwrap();
            assert!(!dir_allowed(&link, &roots), "a symlink out of the root is not inside it");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The whole path on disk: a session's manifest lists a build dir inside a
    /// temp root, a dir outside every root and an AVD; the end deletes the
    /// first and the AVD's files, refuses the second, and drops the manifest.
    #[test]
    fn clean_deletes_what_the_manifest_lists_and_nothing_else() {
        let base = temp("clean");
        let manifests = base.join("manifests");
        std::fs::create_dir_all(&manifests).unwrap();
        let build = std::env::temp_dir().join(format!("exp-manifest-build-{}", std::process::id()));
        std::fs::create_dir_all(build.join("DerivedData")).unwrap();
        let outside = base.join("not-a-temp-root");
        std::fs::create_dir_all(&outside).unwrap();
        let session = "00000000-0000-4000-8000-00000000e868";
        std::fs::write(
            manifests.join(session),
            format!("dir\t{}\ndir\t/definitely/not/there\n", build.display()),
        )
        .unwrap();
        let results = clean(&manifests, session, None);
        assert_eq!(results[0], (Entry::Dir(build.clone()), Outcome::Removed));
        assert!(!build.exists(), "the listed build dir is gone");
        assert_eq!(results[1].1, Outcome::Gone);
        assert!(!manifests.join(session).exists(), "the manifest goes with the session");
        assert!(outside.exists(), "an unlisted dir is never touched");
        // A dir outside every root is refused even when listed (roots of
        // their own: a CI checkout can itself live under the temp dir).
        let root = base.join("root");
        std::fs::create_dir_all(root.join("build")).unwrap();
        let roots = vec![root.clone()];
        assert!(dir_allowed(&root.join("build"), &roots));
        assert!(!dir_allowed(&outside, &roots), "a dir beside the root is not inside it");
        assert_eq!(clean_entry(&Entry::Dir(outside.clone()), &roots), Outcome::Kept(format!(
            "{} is outside the temp, DerivedData and worktree roots",
            outside.display()
        )));
        assert!(outside.exists(), "a refused dir stays");
        assert!(clean(&manifests, session, None).is_empty(), "nothing left to clean");
        assert_eq!(sessions_with_manifests(&manifests), Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn manifests_are_listed_by_session() {
        let dir = temp("list");
        std::fs::write(dir.join("aaaa-1"), "").unwrap();
        std::fs::write(dir.join("bbbb"), "").unwrap();
        std::fs::write(dir.join(".hidden"), "").unwrap();
        assert_eq!(sessions_with_manifests(&dir), vec!["aaaa-1".to_string(), "bbbb".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_avd_without_avdmanager_loses_its_files() {
        let home = temp("avd");
        std::fs::create_dir_all(home.join("exp_s1.avd")).unwrap();
        std::fs::write(home.join("exp_s1.ini"), "").unwrap();
        std::fs::write(home.join("other.ini"), "").unwrap();
        // The only test reading ANDROID_AVD_HOME.
        std::env::set_var("ANDROID_AVD_HOME", &home);
        assert_eq!(remove_avd_files("exp_s1"), Outcome::Removed);
        assert!(!home.join("exp_s1.avd").exists() && !home.join("exp_s1.ini").exists());
        assert!(home.join("other.ini").exists(), "another AVD stays");
        assert_eq!(remove_avd_files("exp_s1"), Outcome::Gone);
        std::env::remove_var("ANDROID_AVD_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }
}
