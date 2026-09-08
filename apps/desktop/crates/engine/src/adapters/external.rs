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

use agent_client_protocol::{Agent, AcpAgent, AcpAgentConfig, Client, ConnectTo};

use super::AdapterSpec;
use crate::session::EngineError;

pub struct ExternalAgent {
    /// The label the session header shows (the spec's own, never a builtin's).
    label: String,
    agent: AcpAgent,
}

impl ExternalAgent {
    /// EXP-766: `spec.exit` is deliberately unused here, so the child's pid
    /// never reaches `coding::reaper::reap_recorded`. The SDK owns the
    /// subprocess: `AcpAgent::connect_to` spawns it internally and hands back
    /// nothing but the connection future — `spawn_process` is public, but a
    /// caller that uses it also has to reimplement the SDK's whole transport
    /// (stderr drain, stdin sink, EOF signalling, the shutdown grace and the
    /// process-group `ChildGuard`), which is 150 lines of copied internals to
    /// re-verify on every SDK bump.
    ///
    /// What that costs: nothing while the engine exits normally or is
    /// cancelled, because the SDK's own guard kills the whole process group on
    /// drop. The gap is only a HARD engine death (SIGKILL, a panic that skips
    /// unwinding), after which the reaper cannot find this child. Revisit when
    /// the SDK exposes the child or a stream-pair transport.
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
        ConnectTo::<Client>::connect_to(self.agent, client)
    }
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

    #[test]
    fn a_builtin_agent_never_takes_the_external_adapter() {
        let spec = AdapterSpec {
            kind: super::super::AdapterKind::External,
            agent: coding::AgentKind::Builtin(coding::CodingAgent::Claude),
            spawn: terminal::pty::SpawnSpec::new("claude"),
            options: coding::LaunchOptions::defaults(&coding::Settings::default()),
            mcp: coding::AgentMcp::ClaudeFile,
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
