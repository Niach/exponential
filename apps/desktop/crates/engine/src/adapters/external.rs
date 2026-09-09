//! EXP-746 (D13) — ExternalAgent: a user-declared binary that already speaks
//! ACP over stdio. Owned by lane E4.
//!
//! Opt-in and settings-driven (`coding::Settings::external_agents`), offered
//! only when the command resolves on `terminal::pty::login_path()`, local
//! starts only — `coding_sessions.start` carries `agent: None` for one and it
//! is never remotely startable.
//!
//! `agent_client_protocol::AcpAgent` already IS a `ConnectTo<Client>`
//! subprocess transport, so there is no wire to write — but it must be
//! constructed from an explicit [`agent_client_protocol::AcpAgentConfig`].
//! NEVER `AcpAgent::claude_agent()` / `AcpAgent::codex()`: those shell out to
//! `npx -y @agentclientprotocol/...@latest`, i.e. network at runtime and an
//! unpinned supply chain.

use std::path::{Path, PathBuf};
use std::time::Duration;

use agent_client_protocol::{Agent, AcpAgent, AcpAgentConfig, Client, ConnectTo};

use super::AdapterSpec;
use crate::host::ChildExitLink;
use crate::session::EngineError;

/// EXP-784: how long [`record_child_pid`] keeps looking for the SDK's child
/// after `connect_to` starts, and how often. The spawn is the first thing
/// the SDK does, so one or two polls normally find it; a slow login-shell
/// wrapper gets the whole window.
const PID_SCAN_WINDOW: Duration = Duration::from_secs(5);
const PID_SCAN_STEP: Duration = Duration::from_millis(100);

pub struct ExternalAgent {
    /// The label the session header shows (the spec's own, never a builtin's).
    label: String,
    agent: AcpAgent,
    /// EXP-784: where the child's pid is recorded once the scan finds it.
    exit: ChildExitLink,
}

impl ExternalAgent {
    /// The SDK owns the subprocess: `AcpAgent::connect_to` spawns it
    /// internally and hands back nothing but the connection future —
    /// `spawn_process` is public, but a caller that uses it also has to
    /// reimplement the SDK's whole transport (stderr drain, stdin sink, EOF
    /// signalling, the shutdown grace and the process-group `ChildGuard`),
    /// which is 150 lines of copied internals to re-verify on every SDK bump.
    ///
    /// EXP-784: the pid is recovered AFTER the fact instead — the child is a
    /// direct child of this process, so [`record_child_pid`] scans our own
    /// children for the spec's program while the connection comes up and
    /// records the match on `spec.exit`, which is what
    /// `coding::reaper::reap_recorded` reads after a HARD engine death
    /// (SIGKILL, a panic that skips unwinding). A normal exit never needed
    /// it: the SDK's own guard kills the process group on drop. Windows has
    /// no scan and records nothing (the EXP-766 gap stands there); the exit
    /// CODE is never recorded on any platform, so the run ends as `ended`.
    pub fn new(spec: AdapterSpec) -> Result<ExternalAgent, EngineError> {
        let coding::AgentKind::External(external) = &spec.agent else {
            return Err(EngineError::Unsupported(
                "this launch names a builtin agent, not an external one",
            ));
        };
        let agent = external_adapter(external, &spec.cwd, &spec.spawn.env)?;
        Ok(ExternalAgent {
            label: spec.agent.label().to_string(),
            agent,
            exit: spec.exit,
        })
    }

    /// The picker/header label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The launch configuration, for tests and log lines.
    pub fn config(&self) -> &AcpAgentConfig {
        self.agent.config()
    }
}

impl ConnectTo<Client> for ExternalAgent {
    fn connect_to(
        self,
        client: impl ConnectTo<Agent>,
    ) -> impl std::future::Future<Output = Result<(), agent_client_protocol::Error>> + Send {
        // Nothing of ours sits in this path: the binary speaks ACP itself, so
        // the SDK's subprocess transport (stdio framing + a process-group
        // ChildGuard) is the whole adapter. `AcpAgent` is generic over the
        // counterpart role (Client or Conductor), so name ours.
        let program = self.agent.config().command().to_path_buf();
        let link = self.exit;
        let connection = ConnectTo::<Client>::connect_to(self.agent, client);
        async move {
            // EXP-784: beside the connection, never in its way — a scan that
            // finds nothing (Windows, a wrapper that re-execs) costs a few
            // polls and records nothing.
            let scan = tokio::runtime::Handle::try_current()
                .ok()
                .map(|handle| handle.spawn(record_child_pid(program, link)));
            let result = connection.await;
            if let Some(scan) = scan {
                scan.abort();
            }
            result
        }
    }
}

/// EXP-784: poll our direct children for `program` until one shows up (or
/// [`PID_SCAN_WINDOW`] is out) and record its pid on `link`.
async fn record_child_pid(program: PathBuf, link: ChildExitLink) {
    let Some(name) = program.file_name().map(|name| name.to_string_lossy().into_owned()) else {
        return;
    };
    let deadline = std::time::Instant::now() + PID_SCAN_WINDOW;
    while std::time::Instant::now() < deadline {
        if let Some(pid) = find_direct_child(std::process::id(), &name) {
            log::debug!("engine: external agent {name} is pid {pid}");
            link.record_pid(pid);
            return;
        }
        tokio::time::sleep(PID_SCAN_STEP).await;
    }
    log::debug!("engine: external agent {name}: no child pid found within the scan window");
}

/// The pid of a DIRECT child of `parent` whose executable (or process name)
/// is `name`. Linux reads `/proc/<pid>/stat` for the ppid and matches the
/// `comm`/`cmdline` basename; macOS asks `proc_listchildpids` and matches
/// `proc_pidpath`/`proc_name`. `None` elsewhere, and whenever nothing
/// matches. Best effort: a wrapper script that `exec`s the real binary
/// still matches on the script's name (Linux comm) or the interpreter's
/// (macOS path) — the reaper only needs SOME handle on the tree's root.
pub fn find_direct_child(parent: u32, name: &str) -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        find_direct_child_linux(parent, name)
    }
    #[cfg(target_os = "macos")]
    {
        find_direct_child_macos(parent, name)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (parent, name);
        None
    }
}

/// Whether `candidate` names `name`: equal, or (Linux `comm`, 15 bytes)
/// a truncated prefix of it.
fn process_name_matches(candidate: &str, name: &str) -> bool {
    if candidate.is_empty() {
        return false;
    }
    candidate == name || (candidate.len() == 15 && name.starts_with(candidate))
}

#[cfg(target_os = "linux")]
fn find_direct_child_linux(parent: u32, name: &str) -> Option<u32> {
    let entries = std::fs::read_dir("/proc").ok()?;
    for entry in entries.filter_map(Result::ok) {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else { continue };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else { continue };
        // `pid (comm) state ppid ...` — comm may hold spaces and parens, so
        // split at the LAST `)`.
        let Some(open) = stat.find('(') else { continue };
        let Some(close) = stat.rfind(')') else { continue };
        let comm = &stat[open + 1..close];
        let mut rest = stat[close + 1..].split_whitespace();
        let _state = rest.next();
        let Some(ppid) = rest.next().and_then(|ppid| ppid.parse::<u32>().ok()) else { continue };
        if ppid != parent {
            continue;
        }
        let argv0 = std::fs::read(entry.path().join("cmdline"))
            .ok()
            .and_then(|bytes| {
                let first = bytes.split(|byte| *byte == 0).next()?;
                let path = String::from_utf8_lossy(first).into_owned();
                Path::new(&path).file_name().map(|base| base.to_string_lossy().into_owned())
            })
            .unwrap_or_default();
        if process_name_matches(comm, name) || process_name_matches(&argv0, name) {
            return Some(pid);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn find_direct_child_macos(parent: u32, name: &str) -> Option<u32> {
    // `proc_listchildpids` returns COUNTS, not byte lengths (measured on
    // macOS 26: a null-buffer call answers with the system's process count,
    // a real one with the number of pids written). The buffer is sized by
    // asking first, plus headroom for children spawned in between.
    let needed = unsafe { libc::proc_listchildpids(parent as libc::pid_t, std::ptr::null_mut(), 0) };
    if needed <= 0 {
        return None;
    }
    let mut pids: Vec<libc::pid_t> = vec![0; needed as usize + 16];
    let bytes = (pids.len() * std::mem::size_of::<libc::pid_t>()) as libc::c_int;
    let written = unsafe {
        libc::proc_listchildpids(parent as libc::pid_t, pids.as_mut_ptr() as *mut libc::c_void, bytes)
    };
    if written <= 0 {
        return None;
    }
    let count = (written as usize).min(pids.len());
    for &pid in pids.iter().take(count) {
        if pid <= 0 {
            continue;
        }
        let mut path = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let len = unsafe {
            libc::proc_pidpath(pid, path.as_mut_ptr() as *mut libc::c_void, path.len() as u32)
        };
        let base = (len > 0)
            .then(|| String::from_utf8_lossy(&path[..len as usize]).into_owned())
            .and_then(|path| Path::new(&path).file_name().map(|base| base.to_string_lossy().into_owned()))
            .unwrap_or_default();
        let mut comm = vec![0u8; 256];
        let len = unsafe {
            libc::proc_name(pid, comm.as_mut_ptr() as *mut libc::c_void, comm.len() as u32)
        };
        let comm = (len > 0)
            .then(|| String::from_utf8_lossy(&comm[..len as usize]).into_owned())
            .unwrap_or_default();
        if process_name_matches(&base, name) || process_name_matches(&comm, name) {
            return Some(pid as u32);
        }
    }
    None
}

/// Build the ACP subprocess transport for one declared external agent.
///
/// `cwd`: `AcpAgentConfig` carries only command/args/env — there is no
/// working-directory setting and the child would inherit the engine
/// process's cwd (on the desktop, whatever the `.app` was launched from).
/// The worktree therefore reaches the binary the way ACP itself specifies
/// it: the host's `session/new { cwd }` names it, and every ACP agent is
/// required to honour that over its own process cwd. It also rides the env
/// as `PWD` so a wrapper script (`sh -c`, a venv launcher) that never sees
/// the ACP handshake can still find the tree, and a spec may override
/// anything it likes through its own `env`.
///
/// `launch_env`: the launcher's own entries off `PreparedLaunch::spawn.env`
/// (EXP-758: the external MCP posture rides there as `EXP_MCP_URL` /
/// `EXP_MCP_TOKEN` / `EXP_MCP_SESSION_ID`, nothing is written into the
/// worktree for an agent whose config format we do not know). They sit
/// between PATH/PWD and the spec's env, so a declared env still wins.
pub fn external_adapter(
    spec: &coding::ExternalAgentSpec,
    cwd: &Path,
    launch_env: &[(String, String)],
) -> Result<AcpAgent, EngineError> {
    let path_env = terminal::pty::login_path();
    let Some(program) = resolve_on_path(&spec.command, &path_env) else {
        return Err(EngineError::Spawn(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{} is not on PATH", spec.command),
        )));
    };
    // PATH first, then PWD, then the launcher's entries, then the spec's
    // own: a declared env is the user's escape hatch and must win over all.
    let config = AcpAgentConfig::new(program)
        .args(spec.args.clone())
        .env("PATH", path_env)
        .env("PWD", cwd.to_string_lossy().into_owned())
        .envs(launch_env.iter().cloned())
        .envs(spec.env.clone());
    Ok(AcpAgent::new(config))
}

/// Resolve `command` the way a login shell would: an explicit path is taken
/// as given (and must exist), a bare name is looked up in `path_env`.
///
/// The lookup is what gates the picker (D13: a spec is only OFFERED when its
/// command resolves), and it runs against `login_path()` rather than the
/// process PATH for the EXP-206 reason: a `.app`/`.desktop` launch carries a
/// minimal PATH without Homebrew or npm-global, so probing with the process
/// PATH hides binaries every terminal launch finds.
pub fn resolve_on_path(command: &str, path_env: &str) -> Option<PathBuf> {
    if command.is_empty() {
        return None;
    }
    let named_path = Path::new(command);
    if named_path.components().count() > 1 || named_path.is_absolute() {
        return is_program(named_path).then(|| named_path.to_path_buf());
    }
    for dir in std::env::split_paths(path_env) {
        let candidate = dir.join(command);
        if is_program(&candidate) {
            return Some(candidate);
        }
        // Windows resolves a bare name through PATHEXT; the three extensions
        // below are the ones an agent CLI actually ships as.
        #[cfg(windows)]
        for extension in ["exe", "cmd", "bat"] {
            let candidate = dir.join(format!("{command}.{extension}"));
            if is_program(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn is_program(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn spec(command: &str) -> coding::ExternalAgentSpec {
        coding::ExternalAgentSpec {
            id: "acme".to_string(),
            label: "Acme".to_string(),
            command: command.to_string(),
            args: vec!["--acp".to_string(), "--quiet".to_string()],
            env: BTreeMap::from([("ACME_TOKEN".to_string(), "t".to_string())]),
        }
    }

    #[test]
    fn an_external_spec_becomes_an_explicit_launch_vector() {
        let dir = std::env::temp_dir().join(format!("exp746-external-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("the scratch dir is writable");
        let program = dir.join("acme-acp");
        std::fs::write(&program, "#!/bin/sh\n").expect("the fake binary writes");
        let path_env = dir.display().to_string();
        let cwd = dir.join("worktree");

        let resolved = resolve_on_path("acme-acp", &path_env).expect("the fake binary resolves");
        assert_eq!(resolved, program);

        // The real builder runs against login_path(), so drive it with the
        // absolute path the resolver would have produced.
        let launch_env = vec![
            ("EXP_MCP_URL".to_string(), "http://127.0.0.1/api/mcp".to_string()),
            // The spec's own entry must win over the launcher's.
            ("ACME_TOKEN".to_string(), "from-launcher".to_string()),
            // EXP-792: the team server list, references verbatim.
            (
                coding::argv::MCP_SERVERS_ENV.to_string(),
                r#"[{"name":"linear","kind":"http","url":"https://mcp.linear.app/mcp","headers":{"Authorization":"Bearer ${EXP_MCP_TOKEN_1}"}}]"#.to_string(),
            ),
        ];
        let agent = external_adapter(&spec(&program.display().to_string()), &cwd, &launch_env)
            .expect("an absolute command needs no PATH lookup");
        let config = agent.config();
        assert_eq!(config.command(), program.as_path());
        assert_eq!(config.arguments(), ["--acp", "--quiet"]);
        assert_eq!(
            config.environment().get("ACME_TOKEN").map(String::as_str),
            Some("t")
        );
        // EXP-758: the launcher's MCP wiring reaches the child.
        assert_eq!(
            config.environment().get("EXP_MCP_URL").map(String::as_str),
            Some("http://127.0.0.1/api/mcp")
        );
        // EXP-792: so does the team server list, `${VAR}` untouched — the
        // agent resolves it from this same environment.
        assert!(config
            .environment()
            .get(coding::argv::MCP_SERVERS_ENV)
            .is_some_and(|json| json.contains("${EXP_MCP_TOKEN_1}")));
        // The worktree is never silently dropped: `session/new { cwd }` names
        // it and PWD carries it to wrapper scripts.
        assert_eq!(
            config.environment().get("PWD").map(String::as_str),
            Some(cwd.display().to_string().as_str())
        );
        assert!(config.environment().contains_key("PATH"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_command_that_does_not_resolve_is_never_offered() {
        assert_eq!(resolve_on_path("", "/usr/bin"), None);
        assert_eq!(
            resolve_on_path("exp746-no-such-binary", "/usr/bin:/bin"),
            None
        );
        // An explicit path that does not exist fails the same way — never a
        // spawn attempt that dies later with a worse message.
        let error = external_adapter(&spec("/no/such/agent"), Path::new("/tmp"), &[])
            .expect_err("a missing binary is a start-time error");
        assert!(
            matches!(&error, EngineError::Spawn(err) if err.kind() == std::io::ErrorKind::NotFound),
            "unexpected error: {error:?}"
        );
    }

    /// EXP-784: the pid scan finds a direct child by its program name and
    /// nothing else — not a grandchild, not an unrelated name.
    #[cfg(unix)]
    #[test]
    fn the_child_scan_finds_a_direct_child_by_name() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .spawn()
            .expect("sleep spawns");
        let me = std::process::id();
        let found = find_direct_child(me, "sleep");
        // Other tests in this binary may spawn their own `sleep`s; what
        // matters is that OURS is findable and a foreign name is not.
        assert!(found.is_some(), "a direct `sleep` child is found");
        assert_eq!(find_direct_child(me, "exp784-no-such-program"), None);
        assert!(process_name_matches("sleep", "sleep"));
        assert!(process_name_matches("acme-acp-server", "acme-acp-server-linux"));
        assert!(!process_name_matches("", "sleep"));
        let own = child.id();
        let _ = child.kill();
        let _ = child.wait();
        // Reaped: never returned again (a sibling test's own `sleep` may be).
        assert_ne!(find_direct_child(me, "sleep"), Some(own), "the reaped pid is not returned");
    }

    #[test]
    fn a_builtin_agent_never_takes_the_external_adapter() {
        let spec = AdapterSpec {
            kind: super::super::AdapterKind::External,
            agent: coding::AgentKind::Builtin(coding::CodingAgent::Claude),
            spawn: terminal::pty::SpawnSpec::new("claude"),
            options: coding::LaunchOptions::defaults(&coding::Settings::default()),
            mcp: coding::AgentMcp::ClaudeFile,
            servers: Vec::new(),
            cwd: PathBuf::from("/tmp"),
            session_id: "sess-1".to_string(),
            prompt: None,
            resume: None,
            replay: false,
            personal_key: None,
            reaper_settings_path: None,
            exit: crate::ChildExitLink::new(),
        };
        assert!(matches!(
            ExternalAgent::new(spec),
            Err(EngineError::Unsupported(_))
        ));
    }
}
