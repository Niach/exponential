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
//!   tears down. The daemon's sweep ([`ended_with_scopes`]) catches a session
//!   that ended while no run of it was live here (it idled first, or the host
//!   restarted).

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
    eagent::run_scope::stop_session_scopes(session_id);
    let Some(worktree) = worktree.filter(|worktree| is_session_worktree(worktree)) else {
        return Vec::new();
    };
    let held = crate::run_cleanup::LiveHolders::default().with_registry(data_dir, session_id);
    if held.worktrees.iter().any(|other| same_dir(other, worktree)) {
        log::info!(
            "session teardown [{session_id}]: {} is shared with a live run, its compose stacks stay",
            worktree.display()
        );
        return Vec::new();
    }
    down_compose_stacks(session_id, worktree)
}

/// [`end_session_processes`] on a side thread (the end never waits on docker).
pub fn end_session_processes_soon(data_dir: PathBuf, session_id: String, worktree: Option<PathBuf>) {
    let spawned = std::thread::Builder::new().name("exp-session-teardown".to_string()).spawn(move || {
        let downed = end_session_processes(&data_dir, &session_id, worktree.as_deref());
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
/// stdout)`, `None` when it never started or timed out.
fn run_bounded(program: &str, args: &[&str], timeout: Duration) -> Option<(bool, String)> {
    use std::io::Read as _;
    let mut child = terminal::process::background_command(program)
        .args(args)
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

/// The daemon's sweep: tears down every session with a scope on this host
/// whose synced row (`rows`) is ended and that no run in `live` drives (it
/// idled before its end, or its host restarted). Blocking. Returns the
/// sessions it tore down.
pub fn sweep_ended(data_dir: &Path, rows: &[SessionRow], live: &HashSet<String>) -> Vec<String> {
    let scoped = eagent::run_scope::sessions_with_scopes();
    let ended = ended_with_scopes(rows, &scoped, live);
    for session_id in &ended {
        let worktree = crate::run_registry::get(data_dir, session_id).map(|record| record.cwd);
        log::info!("session teardown [{session_id}]: ended with leftovers on this host");
        end_session_processes(data_dir, session_id, worktree.as_deref());
    }
    ended
}

/// The daemon's sweep (pure): the sessions with a scope on this host whose
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

    fn row(id: &str, status: &str) -> SessionRow {
        serde_json::from_value(serde_json::json!({ "id": id, "status": status })).unwrap()
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
