//! #NEW-136 — the issue search's MODEL FALLBACK on the desktop: ONE short
//! print-mode call of the signed-in claude CLI, on the login a launch would
//! run on (the user's own account; never a hosted agent, never our server).
//!
//! The pure half (when the row shows, the prompt, the sanitizer that never
//! trusts the answer) is [`domain::issue_search_translation`]; the spec is
//! `packages/domain-contract/fixtures/issue-search-translation.json`.
//!
//! The call is as small as the CLI allows, like the usage probe
//! ([`crate::claude_token::PROBE_ARGS`]): the cheapest model, no tools, no
//! MCP servers, no settings, no transcript, no session row. The prompt goes
//! in on stdin (never argv, where `ps` would show what the user typed). It
//! is killed at the deadline and when the caller cancels (the search closed
//! or its text changed).

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::agent::CodingAgent;
use crate::doctor::DoctorReport;
use crate::settings::Settings;

/// The call's working directory under the data dir: empty, so the CLI finds
/// no project memory and no project settings.
const SCRATCH_DIR: &str = "search-translate";

/// How much of the answer is read (the sanitizer takes one line of it).
const MAX_OUTPUT_BYTES: u64 = 16 * 1024;

/// How often the wait looks at the deadline and the cancel flag.
const POLL: Duration = Duration::from_millis(25);

/// The hard deadline (`limits.timeoutSeconds`).
pub fn timeout() -> Duration {
    Duration::from_secs(domain::issue_search_translation::limits().timeout_seconds)
}

/// The arguments after the program name: print mode, the prompt on stdin.
pub fn args(system: &str, model: &str) -> Vec<String> {
    [
        "-p",
        "--model",
        model,
        "--max-turns",
        "1",
        "--tools",
        "",
        "--system-prompt",
        system,
        "--strict-mcp-config",
        "--setting-sources",
        "",
        "--no-session-persistence",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

/// Where and as whom the call runs. `None` from [`Translator::resolve`] =
/// no signed-in claude login on this device = the search offers no row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Translator {
    pub(crate) program: String,
    pub(crate) path_env: String,
    /// The login's config dir (`CLAUDE_CONFIG_DIR`).
    pub(crate) config_dir: PathBuf,
    /// The login's long-lived token pair, when it holds one.
    pub(crate) token: Option<(String, String)>,
    pub(crate) scratch: PathBuf,
}

impl Translator {
    /// The login a fresh launch would run on, as the doctor found it: the
    /// first signed-in claude profile, the last used first. No probe of its
    /// own (the report is the app's cached one).
    pub fn resolve(report: &DoctorReport, settings: &Settings, data_dir: &Path) -> Option<Translator> {
        let check = &report.claude;
        if !check.ok {
            return None;
        }
        let profile = check.signed_in_profile.as_deref()?;
        let config_dir = crate::agent_profiles::profile_dir(data_dir, CodingAgent::Claude, profile)?;
        Some(Translator {
            program: settings.resolved_path_for(CodingAgent::Claude),
            path_env: terminal::pty::login_path(),
            token: crate::claude_token::env(&config_dir),
            config_dir,
            scratch: data_dir.join(SCRATCH_DIR),
        })
    }
}

/// The caller's stop switch: [`Cancel::cancel`] ends a running call.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// A [`Cancel`] that fires when dropped: the search holds one per call, so
/// closing the sheet or replacing it for a new text ends the call.
#[derive(Debug)]
pub struct CancelOnDrop(pub Cancel);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

/// Run the ONE call: `prompt` on stdin, the answer's text back. `None` on a
/// spawn error, a non-zero exit, the deadline or a cancel. Blocking: call it
/// off the UI thread.
pub fn run(translator: &Translator, prompt: &str, cancel: &Cancel) -> Option<String> {
    let limits = domain::issue_search_translation::limits();
    run_until(
        translator,
        &args(domain::issue_search_translation::system(), &limits.model),
        prompt,
        cancel,
        timeout(),
    )
}

fn run_until(
    translator: &Translator,
    args: &[String],
    prompt: &str,
    cancel: &Cancel,
    timeout: Duration,
) -> Option<String> {
    std::fs::create_dir_all(&translator.scratch).ok()?;
    let mut cmd = eagent::process::background_command(&translator.program);
    for key in crate::claude_token::OUTRANKING_ENV {
        cmd.env_remove(key);
    }
    cmd.env("PATH", &translator.path_env)
        .env("CLAUDE_CONFIG_DIR", &translator.config_dir)
        .current_dir(&translator.scratch)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some((key, value)) = &translator.token {
        cmd.env(key, value);
    }
    // Its own process group: the CLI is a wrapper around its real binary,
    // and a kill has to reach both.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        cmd.process_group(0);
    }
    let mut child = cmd.spawn().ok()?;
    // Feed stdin from its own thread and close it by dropping it: the CLI
    // reads to EOF, and a child that never reads must not block us.
    if let Some(mut pipe) = child.stdin.take() {
        let prompt = prompt.to_string();
        std::thread::spawn(move || {
            let _ = pipe.write_all(prompt.as_bytes());
        });
    }
    let (sent, received) = std::sync::mpsc::channel();
    if let Some(mut pipe) = child.stdout.take() {
        std::thread::spawn(move || {
            let mut text = Vec::new();
            let _ = pipe.by_ref().take(MAX_OUTPUT_BYTES).read_to_end(&mut text);
            // The rest is read and dropped, so a long answer never blocks
            // the CLI on a full pipe.
            let _ = std::io::copy(&mut pipe, &mut std::io::sink());
            let _ = sent.send(text);
        });
    }
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if cancel.is_cancelled() || Instant::now() >= deadline => break None,
            Ok(None) => std::thread::sleep(POLL),
            Err(_) => break None,
        }
    };
    let Some(status) = status else {
        #[cfg(unix)]
        // SAFETY: the child leads its own process group (set above).
        unsafe {
            libc::killpg(child.id() as i32, libc::SIGKILL);
        }
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    if !status.success() {
        return None;
    }
    // A survivor holding the pipe open cannot keep us: the answer is in by
    // the time the CLI exits.
    let text = received.recv_timeout(Duration::from_secs(1)).ok()?;
    Some(String::from_utf8_lossy(&text).into_owned())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::test_support::temp_dir;

    /// A stand-in `claude`: an executable shell script.
    fn stub(dir: &Path, body: &str) -> Translator {
        use std::os::unix::fs::PermissionsExt as _;
        let program = dir.join("claude");
        std::fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        Translator {
            program: program.to_string_lossy().into_owned(),
            path_env: std::env::var("PATH").unwrap_or_default(),
            config_dir: dir.join("profile"),
            token: None,
            scratch: dir.join("scratch"),
        }
    }

    #[test]
    fn the_call_is_the_minimal_print_mode_one() {
        let args = args("SYSTEM", "haiku");
        assert_eq!(
            args,
            [
                "-p", "--model", "haiku", "--max-turns", "1", "--tools", "", "--system-prompt", "SYSTEM",
                "--strict-mcp-config", "--setting-sources", "", "--no-session-persistence",
            ]
        );
        // No resume, no session id, no MCP config, and never the prompt.
        assert!(!args.iter().any(|arg| arg.contains("mcp-config") && arg != "--strict-mcp-config"));
        assert_eq!(timeout(), Duration::from_secs(15));
    }

    #[test]
    fn the_prompt_goes_in_on_stdin_and_the_answer_comes_back() {
        let dir = temp_dir("search-translate-echo");
        // The stub prints its env, its argv and its stdin.
        let translator = stub(&dir.0, r#"echo "$CLAUDE_CONFIG_DIR|$PWD|$*"; cat"#);
        let out = run_until(&translator, &["-p".into(), "--model".into(), "haiku".into()], "the prompt", &Cancel::new(), Duration::from_secs(10))
            .expect("the stub answers");
        let (head, body) = out.split_once('\n').unwrap();
        let fields: Vec<&str> = head.split('|').collect();
        assert_eq!(fields[0], dir.0.join("profile").to_string_lossy());
        assert!(fields[1].ends_with("scratch"), "{head}");
        assert_eq!(fields[2], "-p --model haiku");
        assert_eq!(body, "the prompt");
    }

    #[test]
    fn a_login_token_and_no_outranking_key_reach_the_cli() {
        let dir = temp_dir("search-translate-env");
        let mut translator = stub(&dir.0, r#"echo "$CLAUDE_CODE_OAUTH_TOKEN|${ANTHROPIC_API_KEY:-none}""#);
        translator.token = Some(("CLAUDE_CODE_OAUTH_TOKEN".into(), "tok".into()));
        let out = run_until(&translator, &[], "", &Cancel::new(), Duration::from_secs(10)).unwrap();
        assert!(out.starts_with("tok|"), "{out}");
    }

    #[test]
    fn a_failed_cli_is_no_answer() {
        let dir = temp_dir("search-translate-fail");
        let translator = stub(&dir.0, "echo nope; exit 1");
        assert_eq!(run_until(&translator, &[], "x", &Cancel::new(), Duration::from_secs(10)), None);
        let missing = Translator { program: dir.0.join("absent").to_string_lossy().into_owned(), ..translator };
        assert_eq!(run_until(&missing, &[], "x", &Cancel::new(), Duration::from_secs(10)), None);
    }

    #[test]
    fn a_hanging_cli_is_killed_at_the_deadline() {
        let dir = temp_dir("search-translate-hang");
        let translator = stub(&dir.0, "sleep 30");
        let started = Instant::now();
        assert_eq!(run_until(&translator, &[], "x", &Cancel::new(), Duration::from_millis(300)), None);
        assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
    }

    #[test]
    fn a_cancel_ends_the_call_at_once() {
        let dir = temp_dir("search-translate-cancel");
        let translator = stub(&dir.0, "sleep 30");
        let cancel = Cancel::new();
        let guard = CancelOnDrop(cancel.clone());
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            drop(guard);
        });
        let started = Instant::now();
        assert_eq!(run_until(&translator, &[], "x", &cancel, Duration::from_secs(20)), None);
        assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
        stopper.join().unwrap();
    }

    #[test]
    fn the_answer_is_capped() {
        let dir = temp_dir("search-translate-cap");
        let translator = stub(&dir.0, "yes abcdefghij | head -c 200000");
        let out = run_until(&translator, &[], "x", &Cancel::new(), Duration::from_secs(10)).unwrap();
        assert_eq!(out.len() as u64, MAX_OUTPUT_BYTES);
    }
}
