//! #EXP-1868: an ENDED session takes everything it started with it.
//!
//! ## The bug
//!
//! On 2026-11-15 mint's `expbg.slice` (every agent run's scope) sat at 17.4 GB
//! against its 16 GB `memory.high` for hours: every process in it was
//! throttled (`docker info`, curl, ssh hung; prod looked down). What filled
//! it were leftovers of sessions whose work was done: vite dev servers (2-3.6
//! GB each), a 2.3 GB gradle JVM and their docker compose stacks.
//!
//! A session's agent runs in its own systemd scope (`eagent::run_scope`), and
//! its end killed the agent's PROCESS GROUP (`ChildGuard`). A background
//! `bun dev`, a gradle or kotlin daemon `setsid`s into a group of its own, so
//! it survived; the scope (`--collect`) stayed alive around it, named
//! `run-<random>`, so nothing could find it again. Compose containers are not
//! in the scope at all: they run under dockerd.
//!
//! ## The fix
//!
//! - Every scope is named after its session (`run-exp-<session>-<n>.scope`)
//!   and the end stops exactly those scopes ([`end_session_processes`]): by
//!   cgroup, never by process name.
//! - The compose stacks the session started from ITS OWN worktree (a
//!   container's `com.docker.compose.project.working_dir` or one of its
//!   `config_files` inside it) go `docker compose -p <project> down`
//!   WITH their volumes (a session stack's data is its own). A worktree another live run still works in keeps its
//!   stacks; the main checkout never matches (it is not inside a session
//!   worktree).
//! - An idle session (suspended, released) keeps everything: only an END
//!   tears down. Whatever the session created OUTSIDE its scope and listed in
//!   its manifest (simulators, AVDs, build dirs, compose projects) goes too
//!   ([`crate::session_manifest`]). Each host's sweep ([`sweep_ended`]: the
//!   daemon and the desktop app) catches a session that ended while no run
//!   of it was live here (it idled first, or the host restarted), and every
//!   end the engine's own end sequence leaves to the row (#EXP-1909: a lost
//!   run, a failed start, a root chat whose CLI exited stay alive on the
//!   server).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use domain::rows::SessionRow;

/// `docker ps` under a throttled slice can hang: never longer than this.
const DOCKER_LIST_TIMEOUT: Duration = Duration::from_secs(30);
/// One `docker compose down` (stops, removes containers and networks).
const COMPOSE_DOWN_TIMEOUT: Duration = Duration::from_secs(180);

/// The `docker ps` format [`compose_projects_in`] parses: project, working
/// dir, config files (comma-separated), tab-separated.
const COMPOSE_FORMAT: &str = "{{.Label \"com.docker.compose.project\"}}\t{{.Label \"com.docker.compose.project.working_dir\"}}\t{{.Label \"com.docker.compose.project.config_files\"}}";

/// Stops everything `session_id` started on this host: its scopes (the agent,
/// vite, gradle/kotlin daemons, emulators) and the compose stacks started
/// from `worktree`, unless another live run works in that worktree (a shared
/// issue worktree). Blocking (systemctl, docker): call it off any UI thread.
/// Returns the compose projects it brought down.
pub fn end_session_processes(data_dir: &Path, session_id: &str, worktree: Option<&Path>) -> Vec<String> {
    teardown(data_dir, session_id, worktree, Some(session_id))
}

/// [`end_session_processes`]. `own` = the session whose run record does not
/// count as a live holder of `worktree`: the ending run's own (its host
/// clears its pid only at the end); `None` on the sweep, where a live run of
/// the session itself means it was revived.
fn teardown(data_dir: &Path, session_id: &str, worktree: Option<&Path>, own: Option<&str>) -> Vec<String> {
    eagent::run_scope::stop_session_scopes(session_id);
    session_wide_teardown(data_dir, session_id, worktree, own)
}

/// #EXP-1917: the teardown of ONE run's end, on its own thread: the scope
/// units `units` that run owned when it ended ([`eagent::run_scope::session_scope_units`],
/// read before the thread), by name, never the session's pattern. What
/// belongs to the SESSION (its manifest, its worktree's compose stacks) goes
/// only while no run of it is live here: a revive that started meanwhile
/// keeps them, and its own scope.
fn teardown_run(data_dir: &Path, session_id: &str, worktree: Option<&Path>, units: &[String]) -> Vec<String> {
    if !units.is_empty() {
        eagent::run_scope::stop_scope_units(units);
    }
    if live_on_host(data_dir, session_id) {
        log::info!("session teardown [{session_id}]: revived meanwhile, the new run keeps its manifest and stacks");
        return Vec::new();
    }
    session_wide_teardown(data_dir, session_id, worktree, Some(session_id))
}

/// The session's manifest and its worktree's compose stacks ([`teardown`]).
fn session_wide_teardown(data_dir: &Path, session_id: &str, worktree: Option<&Path>, own: Option<&str>) -> Vec<String> {
    // What it created outside its scope (simulators, AVDs, build dirs,
    // compose projects it listed): its manifest, same on macOS and Linux.
    if let Some(manifests) = crate::session_manifest::manifest_dir() {
        crate::session_manifest::clean(&manifests, session_id, worktree);
    }
    let Some(worktree) = worktree.filter(|worktree| is_session_worktree(worktree)) else {
        return Vec::new();
    };
    if held_by_live_run(data_dir, own, worktree) {
        log::info!(
            "session teardown [{session_id}]: {} is shared with a live run, its compose stacks stay",
            worktree.display()
        );
        return Vec::new();
    }
    down_compose_stacks(session_id, worktree)
}

/// [`end_session_processes`] for the run ending NOW, on a side thread (the
/// end never waits on docker). #EXP-1917: run-scoped: the run's scope units
/// are read here, before the thread, and only they are stopped; a revive of
/// the session that starts before the thread runs keeps its new scope.
pub fn end_session_processes_soon(data_dir: PathBuf, session_id: String, worktree: Option<PathBuf>) {
    let units = eagent::run_scope::session_scope_units(&session_id);
    let spawned = std::thread::Builder::new().name("exp-session-teardown".to_string()).spawn(move || {
        let downed = teardown_run(&data_dir, &session_id, worktree.as_deref(), &units);
        if !downed.is_empty() {
            log::info!("session teardown [{session_id}]: compose stacks down: {}", downed.join(", "));
        }
    });
    if let Err(err) = spawned {
        log::warn!("session teardown: thread failed to spawn: {err}");
    }
}

/// A directory a session's run owns: inside a `<clone>.worktrees/` dir (the
/// launcher's `git_worktree::worktrees_dir`). The main checkout, a home dir
/// or `/` never are: their compose stacks are nobody's leftovers.
pub fn is_session_worktree(dir: &Path) -> bool {
    dir.parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".worktrees"))
        && dir.file_name().is_some()
}

/// Does a run a live host process drives work in `worktree`? `own`'s record
/// does not count ([`teardown`]).
fn held_by_live_run(data_dir: &Path, own: Option<&str>, worktree: &Path) -> bool {
    crate::run_registry::live_host_runs(data_dir)
        .into_iter()
        .any(|(session, cwd)| own != Some(session.as_str()) && same_dir(&cwd, worktree))
}

/// Does a live host process on this machine (this one, the desktop app, the
/// daemon) drive a run of `session_id`? The run registry's word.
fn live_on_host(data_dir: &Path, session_id: &str) -> bool {
    crate::run_registry::live_host_runs(data_dir).into_iter().any(|(session, _)| session == session_id)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    let canon = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canon(a) == canon(b)
}

/// The compose projects in a [`COMPOSE_FORMAT`] listing that were started
/// from inside `worktree` (working dir or any config file under it, whole
/// path components), each once, in listing order.
pub fn compose_projects_in(listing: &str, worktree: &Path) -> Vec<String> {
    let mut projects: Vec<String> = Vec::new();
    for line in listing.lines() {
        let mut fields = line.split('\t');
        let project = fields.next().unwrap_or("").trim();
        if project.is_empty() || projects.iter().any(|known| known == project) {
            continue;
        }
        let working_dir = fields.next().unwrap_or("").trim();
        let config_files = fields.next().unwrap_or("");
        let inside = |path: &str| !path.is_empty() && Path::new(path).starts_with(worktree);
        if inside(working_dir) || config_files.split(',').map(str::trim).any(inside) {
            projects.push(project.to_string());
        }
    }
    projects
}

/// `docker compose -p <project> down --volumes` for every stack started from
/// `worktree`. No docker, or a docker that does not answer: nothing.
fn down_compose_stacks(session_id: &str, worktree: &Path) -> Vec<String> {
    let listing = match run_bounded(
        "docker",
        &["ps", "-a", "--filter", "label=com.docker.compose.project", "--format", COMPOSE_FORMAT],
        DOCKER_LIST_TIMEOUT,
    ) {
        Some((true, stdout)) => stdout,
        _ => return Vec::new(),
    };
    let mut downed = Vec::new();
    for project in compose_projects_in(&listing, worktree) {
        match run_bounded(
            "docker",
            &["compose", "-p", &project, "down", "--volumes", "--remove-orphans", "--timeout", "10"],
            COMPOSE_DOWN_TIMEOUT,
        ) {
            Some((true, _)) => downed.push(project),
            _ => log::warn!("session teardown [{session_id}]: docker compose -p {project} down failed"),
        }
    }
    downed
}

/// Runs `program args` with stdin closed, killed at `timeout`: `(success,
/// stdout)`, `None` when it never started or timed out. On the login
/// shell's PATH, like the manifest's commands (launchd's has no docker).
fn run_bounded(program: &str, args: &[&str], timeout: Duration) -> Option<(bool, String)> {
    use std::io::Read as _;
    let mut child = terminal::process::background_command(program)
        .args(args)
        .env("PATH", terminal::pty::login_path())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = stdout.read_to_string(&mut out);
        out
    });
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    Some((status.success(), reader.join().unwrap_or_default()))
}

/// The hosts' sweep (the daemon's and the desktop app's, #EXP-1909): tears
/// down every session with a scope or a manifest on this host whose synced
/// row (`rows`) is ended and that no run in `live` drives (it idled before
/// its end, or its host restarted). `still_ended` reads the session's row
/// and the live runs AGAIN right before its teardown, and a session another
/// host process here drives is never touched ([`sweep_each`]). Blocking.
/// Returns the sessions it tore down.
pub fn sweep_ended(
    data_dir: &Path,
    rows: &[SessionRow],
    live: &HashSet<String>,
    still_ended: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    // A session leaves a scope (Linux) or a manifest (any OS), or both.
    let mut scoped = eagent::run_scope::sessions_with_scopes();
    if let Some(manifests) = crate::session_manifest::manifest_dir() {
        scoped.extend(crate::session_manifest::sessions_with_manifests(&manifests));
    }
    scoped.sort();
    scoped.dedup();
    let ended = ended_with_scopes(rows, &scoped, live);
    sweep_each(data_dir, &ended, still_ended, |session_id| {
        let worktree = crate::run_registry::get(data_dir, session_id).map(|record| record.cwd);
        log::info!("session teardown [{session_id}]: ended with leftovers on this host");
        teardown(data_dir, session_id, worktree.as_deref(), None);
    })
}

/// The sweep's loop: one `teardown` per session of `ended`, in order, each
/// only if `still_ended` says so right before it (#EXP-1909: `ended` is a
/// snapshot, and one teardown can block for minutes on docker). Returns the
/// sessions it tore down.
///
/// #EXP-1911: a session with a live run in the HOST-WIDE run registry is never swept,
/// whatever the caller's rows say: the desktop app and the daemon share a
/// machine and a data dir, and one's stale rows must not stop a run the
/// other drives.
fn sweep_each(
    data_dir: &Path,
    ended: &[String],
    still_ended: &dyn Fn(&str) -> bool,
    mut teardown: impl FnMut(&str),
) -> Vec<String> {
    let mut swept = Vec::new();
    for session_id in ended {
        if live_on_host(data_dir, session_id) {
            log::info!("session teardown [{session_id}]: a host process here drives a run of it, it keeps everything");
            continue;
        }
        if !still_ended(session_id) {
            log::info!("session teardown [{session_id}]: not ended any more (or live here again), it keeps everything");
            continue;
        }
        teardown(session_id);
        swept.push(session_id.clone());
    }
    swept
}

/// The sweep (pure): the sessions with a scope on this host whose
/// synced row is ENDED and that no run here drives right now. A row this
/// host cannot see (another team, not synced yet) is never swept.
pub fn ended_with_scopes(rows: &[SessionRow], scoped: &[String], live: &HashSet<String>) -> Vec<String> {
    scoped
        .iter()
        .filter(|session| !live.contains(session.as_str()))
        .filter(|session| rows.iter().any(|row| row.id == **session && row.is_ended()))
        .cloned()
        .collect()
}

// ---- #EXP-1514: the agent spaces this host keeps for its sessions ----
//
// A space lives with its SESSION (D 5), kept by the host that launched its
// space run: the run's end leaves it (row 13), and only these close it: an
// attached engine's end for a session that ended, was deleted or left this
// device (`engine::lifecycle`), this host's sweep beat for one that did so
// while no run of it was live here ([`sweep_spaces`]), the person signing out
// ([`close_account_spaces`]), the switch going off (`epilot::shutdown`) and
// the list's Close. A person's chat never ends by itself, so these are all.
// At its start a host takes its own sessions' spaces over ([`start_spaces`],
// [`settle_spaces`], row 15); never another host's (the desktop app and the
// daemon are two devices on one machine and share the run records).

/// #EXP-1514: the kept sessions whose row this process has seen (a launch
/// here, the adoption's settle, a sweep): gone after that = deleted.
static SPACE_ROWS_SEEN: std::sync::Mutex<Option<HashSet<String>>> = std::sync::Mutex::new(None);

fn spaces_seen<T>(with: impl FnOnce(&mut HashSet<String>) -> T) -> T {
    let mut seen = SPACE_ROWS_SEEN.lock().unwrap_or_else(|p| p.into_inner());
    with(seen.get_or_insert_with(HashSet::new))
}

/// #EXP-1514: a launch here made `session_id` a space run: its row exists.
pub fn saw_space_row(session_id: &str) {
    spaces_seen(|seen| seen.insert(session_id.to_string()));
}

/// #EXP-1514 (row 15): where a session is, as this host's synced row says:
/// ended, or on another device = `Ended`; no row = `Unknown`, never ended.
pub fn space_session_here(row: Option<&SessionRow>, device_id: &str) -> crate::computer::SessionHere {
    match row {
        None => crate::computer::SessionHere::Unknown,
        Some(row) if row.is_ended() || !row.on_device(device_id) => crate::computer::SessionHere::Ended,
        Some(_) => crate::computer::SessionHere::Live,
    }
}

/// #EXP-1514 (row 15), step 1, at the host's start, before any launch: its
/// run record notes each session's first space from now on (`had_space`),
/// the offer's cache starts to fill (the first launch then reads a real
/// answer), and the spaces of the sessions whose records name THIS host
/// (`device_id`) are held, or destroyed when the switch is off. Settle the
/// answer once the rows are in sync ([`settle_spaces`]).
pub fn start_spaces(data_dir: &Path, device_id: &str, switch_on: bool) -> crate::computer::Adoption {
    let records = data_dir.to_path_buf();
    crate::computer::set_space_made_observer(move |session_id| crate::run_registry::note_space_made(&records, session_id));
    let _ = crate::computer::space_offer();
    let ours = crate::run_registry::space_sessions(data_dir, device_id);
    if !ours.is_empty() {
        log::info!("computer use: adopting the agent spaces of {} session(s) of this host (switch {})", ours.len(), if switch_on { "on" } else { "off" });
    }
    crate::computer::adopt_spaces(&ours, switch_on)
}

/// #EXP-1514 (row 15), step 2: the held spaces as this host's synced rows
/// say: live here = kept; ended or elsewhere = destroyed unless a host
/// process here drives a run of it (the run registry, read right before);
/// no row = the hold goes and its own grants decide. Blocking (the
/// compositor): off any UI thread.
pub fn settle_spaces(adoption: crate::computer::Adoption, data_dir: &Path, rows: &[SessionRow], device_id: &str) {
    adoption.settle(
        |session_id| {
            let row = rows.iter().find(|row| row.id == session_id);
            if row.is_some() {
                saw_space_row(session_id);
            }
            space_session_here(row, device_id)
        },
        |session_id| live_on_host(data_dir, session_id),
    );
}

/// #EXP-1514 (row 14), pure: which of this host's kept spaces (`kept`)
/// close on its sweep beat: those whose session is ended, on another
/// device, or gone after this host saw its row (`seen`: deleted). Judged
/// only by the rows of the account they belong to (`account_of`, their run
/// record's: an account switch is no deletion), never while a host process
/// here drives a run of them (`live`: its engine's end decides). A row this
/// host never saw says nothing: kept. Notes the rows it sees in `seen`.
pub fn spaces_to_close(
    kept: &[String],
    rows: &[SessionRow],
    account_id: &str,
    device_id: &str,
    seen: &mut HashSet<String>,
    account_of: &dyn Fn(&str) -> Option<String>,
    live: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    seen.retain(|session| kept.contains(session));
    let mut close = Vec::new();
    for session in kept {
        if account_of(session).is_some_and(|account| account != account_id) {
            continue;
        }
        let row = rows.iter().find(|row| row.id == *session);
        if row.is_some() {
            seen.insert(session.clone());
        }
        if live(session) {
            continue;
        }
        let gone = match row {
            Some(row) => row.is_ended() || !row.on_device(device_id),
            None => seen.contains(session),
        };
        if gone {
            close.push(session.clone());
        }
    }
    close
}

/// #EXP-1514 (row 14): the host's sweep beat (the daemon's and the desktop
/// app's, every [`crate::computer::kept_spaces`] against `rows`, which must
/// be in sync and of `account_id`): a kept session that ended, was deleted
/// or left this device while no run of it was live here has its space
/// closed now. Blocking (the compositor). Returns the sessions closed.
pub fn sweep_spaces(data_dir: &Path, rows: &[SessionRow], account_id: &str, device_id: &str) -> Vec<String> {
    let kept = crate::computer::kept_spaces();
    if kept.is_empty() {
        return Vec::new();
    }
    let close = spaces_seen(|seen| {
        spaces_to_close(
            &kept,
            rows,
            account_id,
            device_id,
            seen,
            &|session| crate::run_registry::get(data_dir, session).map(|record| record.account_id),
            &|session| live_on_host(data_dir, session),
        )
    });
    for session in &close {
        log::info!("session teardown [{session}]: ended, deleted or moved away: its agent space closes");
        crate::computer::close_space(session);
    }
    close
}

/// #EXP-1514 (row 14), pure: the kept spaces a sign-out of `account_id`
/// closes: its sessions' (by their run records), and any whose record is
/// gone.
pub fn spaces_of_account(kept: &[String], account_id: &str, account_of: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
    kept.iter().filter(|session| account_of(session).is_none_or(|account| account == account_id)).cloned().collect()
}

/// #EXP-1514 (row 14): the person signs out of `account_id` here: every
/// space this host keeps for that account's sessions closes. Blocking (the
/// compositor): off any UI thread.
pub fn close_account_spaces(data_dir: &Path, account_id: &str) -> Vec<String> {
    let kept = crate::computer::kept_spaces();
    let close = spaces_of_account(&kept, account_id, &|session| crate::run_registry::get(data_dir, session).map(|record| record.account_id));
    for session in &close {
        log::info!("session teardown [{session}]: signed out: its agent space closes");
        crate::computer::close_space(session);
    }
    close
}

/// #EXP-1514 (row 14): a sign-out in a process that keeps nothing (the
/// CLI's `logout`, beside the daemon): the spaces of `account_id`'s
/// sessions whose records name the host `device_id` close at the desktop.
pub fn close_recorded_spaces(data_dir: &Path, device_id: &str, account_id: &str) -> Vec<String> {
    let close: Vec<String> = crate::run_registry::space_sessions(data_dir, device_id)
        .into_iter()
        .filter(|session| crate::run_registry::get(data_dir, session).is_some_and(|record| record.account_id == account_id))
        .collect();
    for session in &close {
        crate::computer::close_space(session);
    }
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stack_started_from_the_worktree_is_its_sessions() {
        let worktree = Path::new("/repos/acme.worktrees/exp-session-c026c74a");
        let listing = "\
c026\t/repos/acme.worktrees/exp-session-c026c74a\t/repos/acme.worktrees/exp-session-c026c74a/docker-compose.yml
c026\t/repos/acme.worktrees/exp-session-c026c74a\t/repos/acme.worktrees/exp-session-c026c74a/docker-compose.yml
cap9122\t/tmp/cap\t/tmp/cap/a.yml,/repos/acme.worktrees/exp-session-c026c74a/apps/web/compose.yml
exponential\t/repos/acme\t/repos/acme/docker-compose.yml
sibling\t/repos/acme.worktrees/exp-session-c026c74a2\t/repos/acme.worktrees/exp-session-c026c74a2/compose.yml
\t/repos/acme.worktrees/exp-session-c026c74a\t
";
        assert_eq!(compose_projects_in(listing, worktree), vec!["c026".to_string(), "cap9122".to_string()]);
    }

    #[test]
    fn only_a_session_worktree_owns_stacks() {
        assert!(is_session_worktree(Path::new("/repos/acme.worktrees/exp-session-c026c74a")));
        assert!(is_session_worktree(Path::new("/repos/acme.worktrees/exp-EXP-42")));
        assert!(!is_session_worktree(Path::new("/repos/acme")), "the main checkout");
        assert!(!is_session_worktree(Path::new("/repos/acme.worktrees")));
        assert!(!is_session_worktree(Path::new("/")));
        assert!(!is_session_worktree(Path::new("/home/u/.local/share/exponential/scratch/abc")));
    }

    #[test]
    fn a_session_that_is_not_the_worktrees_owner_keeps_its_stacks() {
        let dir = std::env::temp_dir().join(format!("exp-teardown-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // The main checkout: no docker is ever asked, nothing comes down
        // (the scope stop for an unknown session is a no-op).
        let session = "00000000-0000-4000-8000-00000000e868";
        assert!(end_session_processes(&dir, session, Some(&dir)).is_empty());
        assert!(end_session_processes(&dir, session, None).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #EXP-1917 (the proof the issue asks for): a run is killed, the
    /// session is revived before the killed run's teardown thread ran: the
    /// teardown stops the old run's scope and the new run's survives. On a
    /// host without run scopes there is nothing to stop.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_killed_runs_teardown_spares_the_revived_runs_scope() {
        let mode = eagent::run_scope::ScopeMode::current();
        if *mode == eagent::run_scope::ScopeMode::Off {
            return;
        }
        let session = format!("00000000-0000-4000-8000-{:012}", 1917_000_000u64 + u64::from(std::process::id()));
        let spawn = || {
            let argv = mode.wrap_for("sleep", &["300".to_string()], Some(&session));
            std::process::Command::new(&argv[0]).args(&argv[1..]).spawn().expect("a scoped sleep")
        };
        let listed = |count: usize| {
            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            loop {
                let units = eagent::run_scope::session_scope_units(&session);
                if units.len() >= count || std::time::Instant::now() > deadline {
                    return units;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        };
        let alive = |child: &mut std::process::Child| matches!(child.try_wait(), Ok(None));
        let mut old = spawn();
        // The killed run's end reads its units ...
        let units = listed(1);
        assert_eq!(units.len(), 1, "{units:?}");
        // ... the revive starts a new run (and its record) before the
        // teardown thread got to run.
        let mut revived = spawn();
        assert_eq!(listed(2).len(), 2);
        let dir = std::env::temp_dir().join(format!("exp-teardown-revive-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut record = crate::run_registry::sample_record(&session);
        record.host_pid = Some(std::process::id());
        crate::run_registry::record(&dir, record);
        teardown_run(&dir, &session, None, &units);
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while alive(&mut old) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        let old_gone = !alive(&mut old);
        let revived_alive = alive(&mut revived);
        eagent::run_scope::stop_session_scopes(&session);
        let _ = old.kill();
        let _ = revived.kill();
        let _ = old.wait();
        let _ = revived.wait();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(old_gone, "the killed run's scope is stopped");
        assert!(revived_alive, "the revived run's scope survives the old run's teardown");
    }

    /// #EXP-1909: one teardown can take minutes (docker); a session revived
    /// meanwhile is asked about again right before its turn and stays.
    #[test]
    fn the_sweep_asks_again_before_each_teardown() {
        let dir = std::env::temp_dir().join(format!("exp-teardown-ask-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ended = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let revived = std::cell::Cell::new(false);
        let mut torn = Vec::new();
        let swept = sweep_each(
            &dir,
            &ended,
            &|session| !(session == "b" && revived.get()),
            |session| {
                // While `a` comes down, a person messages `b`.
                revived.set(true);
                torn.push(session.to_string());
            },
        );
        assert_eq!(torn, vec!["a".to_string(), "c".to_string()]);
        assert_eq!(swept, torn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #EXP-1911: the app and the daemon share a machine. A session that
    /// reads ended in THIS host's rows (stale: its sync is behind) while the
    /// other host process drives a run of it keeps everything.
    #[test]
    fn a_session_live_in_the_host_registry_is_never_swept() {
        let dir = std::env::temp_dir().join(format!("exp-teardown-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let live = "00000000-0000-4000-8000-0000000a1911".to_string();
        let mut record = crate::run_registry::sample_record(&live);
        record.host_pid = Some(std::process::id());
        crate::run_registry::record(&dir, record);
        let ended = vec!["00000000-0000-4000-8000-00000000e911".to_string(), live.clone()];
        let mut torn = Vec::new();
        let swept = sweep_each(&dir, &ended, &|_| true, |session| torn.push(session.to_string()));
        assert_eq!(torn, vec![ended[0].clone()], "ended in the rows, live on the host: untouched");
        assert_eq!(swept, torn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #EXP-1909: the ending run's own record never holds its worktree; on
    /// the sweep a live run of the session itself does (it was revived).
    #[test]
    fn on_the_sweep_the_sessions_own_live_run_holds_its_worktree() {
        let dir = std::env::temp_dir().join(format!("exp-teardown-held-{}", std::process::id()));
        let worktree = dir.join("acme.worktrees").join("exp-session-0000e909");
        std::fs::create_dir_all(&worktree).unwrap();
        let session = "00000000-0000-4000-8000-00000000e909";
        let mut record = crate::run_registry::sample_record(session);
        record.cwd = worktree.clone();
        record.host_pid = Some(std::process::id());
        crate::run_registry::record(&dir, record);
        assert!(!held_by_live_run(&dir, Some(session), &worktree), "the end: its own record");
        assert!(held_by_live_run(&dir, None, &worktree), "the sweep: a run of it is live again");
        assert!(held_by_live_run(&dir, Some("another"), &worktree));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn row(id: &str, status: &str) -> SessionRow {
        serde_json::from_value(serde_json::json!({ "id": id, "status": status })).unwrap()
    }

    fn on_device(id: &str, status: &str, device: &str) -> SessionRow {
        serde_json::from_value(serde_json::json!({ "id": id, "status": status, "device_id": device })).unwrap()
    }

    /// #EXP-1514 (row 14): a chat that ended and one deleted after its idle
    /// release (no engine of either here) are closed within a beat; a chat
    /// that idles (released, or this host restarted) keeps its space.
    #[test]
    fn an_ended_and_a_deleted_chat_close_within_a_beat_an_idle_one_keeps_its_space() {
        let kept: Vec<String> = ["ended", "deleted", "idle"].iter().map(|s| s.to_string()).collect();
        let mut seen = HashSet::new();
        let account = |_: &str| Some("acct".to_string());
        let nobody = |_: &str| false;
        // The beat while all three rows are there: the ended one goes.
        let rows = vec![on_device("ended", "ended", "dev"), on_device("deleted", "idle", "dev"), on_device("idle", "idle", "dev")];
        assert_eq!(spaces_to_close(&kept, &rows, "acct", "dev", &mut seen, &account, &nobody), ["ended"]);
        // The person deletes the released chat: its row is gone by the next beat.
        let kept: Vec<String> = ["deleted", "idle"].iter().map(|s| s.to_string()).collect();
        let rows = vec![on_device("idle", "idle", "dev")];
        assert_eq!(spaces_to_close(&kept, &rows, "acct", "dev", &mut seen, &account, &nobody), ["deleted"]);
        // Beats go by: the idle chat keeps its space.
        let kept = vec!["idle".to_string()];
        for _ in 0..3 {
            assert!(spaces_to_close(&kept, &rows, "acct", "dev", &mut seen, &account, &nobody).is_empty());
        }
    }

    /// #EXP-1514: a missing row is unknown: a session whose row this host
    /// never saw keeps its space (a cold store, another team), and the
    /// adoption asks it as `Unknown`, never `Ended`.
    #[test]
    fn a_missing_row_is_unknown() {
        let kept = vec!["never-seen".to_string()];
        let mut seen = HashSet::new();
        assert!(spaces_to_close(&kept, &[], "acct", "dev", &mut seen, &|_| None, &|_| false).is_empty());
        assert!(seen.is_empty());
        assert_eq!(space_session_here(None, "dev"), crate::computer::SessionHere::Unknown);
        assert_eq!(space_session_here(Some(&on_device("s", "idle", "dev")), "dev"), crate::computer::SessionHere::Live);
        assert_eq!(space_session_here(Some(&on_device("s", "running", "dev")), "dev"), crate::computer::SessionHere::Live);
        assert_eq!(space_session_here(Some(&on_device("s", "ended", "dev")), "dev"), crate::computer::SessionHere::Ended);
        assert_eq!(space_session_here(Some(&on_device("s", "idle", "other")), "dev"), crate::computer::SessionHere::Ended);
    }

    /// #EXP-1514: the desktop app and the daemon are two devices on one
    /// machine sharing the run records: each adopts only the spaces its
    /// own launches made, a session that moved to the other is closed only
    /// while no run of it is live there, and another account's sessions are
    /// never judged by this account's rows.
    #[test]
    fn the_app_never_closes_a_daemon_sessions_space_and_the_other_way_round() {
        let dir = std::env::temp_dir().join(format!("exp-spaces-hosts-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for (session, host) in [("s-app", Some("dev-app")), ("s-cli", Some("dev-cli")), ("s-none", None)] {
            let mut record = crate::run_registry::sample_record(session);
            record.set_space_host(host);
            crate::run_registry::record(&dir, record);
        }
        assert_eq!(crate::run_registry::space_sessions(&dir, "dev-app"), ["s-app"]);
        assert_eq!(crate::run_registry::space_sessions(&dir, "dev-cli"), ["s-cli"]);
        // The app keeps s-app; its row moved to the daemon, which drives it now.
        let kept = vec!["s-app".to_string()];
        let rows = vec![on_device("s-app", "running", "dev-cli")];
        let mut seen = HashSet::new();
        let account = |_: &str| Some("acct".to_string());
        assert!(spaces_to_close(&kept, &rows, "acct", "dev-app", &mut seen, &account, &|_| true).is_empty(), "a live run there");
        assert_eq!(spaces_to_close(&kept, &rows, "acct", "dev-app", &mut seen, &account, &|_| false), ["s-app"], "it left: no run of it is live");
        // Another account's session is not judged by these rows.
        let other = |_: &str| Some("acct-2".to_string());
        assert!(spaces_to_close(&kept, &[], "acct", "dev-app", &mut seen, &other, &|_| false).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// #EXP-1514 (row 14): a sign-out closes every space this host keeps
    /// for the account's sessions (and any whose record is gone), never
    /// another signed-in account's.
    #[test]
    fn a_sign_out_closes_every_kept_space_of_the_account() {
        let kept: Vec<String> = ["mine-1", "mine-2", "theirs", "no-record"].iter().map(|s| s.to_string()).collect();
        let account_of = |session: &str| match session {
            "theirs" => Some("acct-2".to_string()),
            "no-record" => None,
            _ => Some("acct".to_string()),
        };
        assert_eq!(spaces_of_account(&kept, "acct", &account_of), ["mine-1", "mine-2", "no-record"]);
    }

    #[test]
    fn the_sweep_takes_only_ended_sessions_no_run_here_drives() {
        let rows = vec![row("ended", "ended"), row("idle", "idle"), row("live", "ended"), row("working", "working")];
        let scoped: Vec<String> =
            ["ended", "idle", "live", "working", "unknown"].iter().map(|s| s.to_string()).collect();
        let live: HashSet<String> = ["live".to_string()].into();
        assert_eq!(ended_with_scopes(&rows, &scoped, &live), vec!["ended".to_string()]);
    }
}
