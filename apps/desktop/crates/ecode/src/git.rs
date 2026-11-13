//! git via argv: the ops gitoxide does not cover yet (push, fetch, squash
//! merge, `rebase --onto`, bundles) run as `git` with explicit arguments,
//! never through a shell, never with a secret in the argv. Every captured
//! output is scrubbed by the caller's redactor before it becomes an error.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::{Error, Result};

/// Whether a `git` binary runs.
pub fn git_available() -> bool {
    Command::new("git").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// One git invocation being built.
pub struct GitCommand {
    cwd: Option<PathBuf>,
    args: Vec<OsString>,
    env: Vec<(OsString, OsString)>,
    scrub: Vec<String>,
}

/// A finished invocation.
pub(crate) struct GitOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl GitCommand {
    pub fn new(cwd: Option<&Path>) -> Self {
        Self { cwd: cwd.map(Path::to_path_buf), args: vec![], env: vec![], scrub: vec![] }
    }

    pub fn arg(mut self, arg: impl Into<OsString>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args<I: IntoIterator<Item = S>, S: Into<OsString>>(mut self, args: I) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, key: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Texts to redact from any output (tokens).
    pub fn scrub(mut self, secret: impl Into<String>) -> Self {
        let secret = secret.into();
        if !secret.is_empty() {
            self.scrub.push(secret);
        }
        self
    }

    fn command(&self) -> Command {
        let mut command = Command::new("git");
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        // Never prompt (a missing credential fails instead of hanging) and
        // never read the user's hooks into a library operation.
        command
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_ASKPASS", "")
            .env("SSH_ASKPASS", "")
            .env("LC_ALL", "C")
            .args(["-c", "core.hooksPath=/dev/null"])
            .args(&self.args)
            .stdin(Stdio::null());
        for (key, value) in &self.env {
            command.env(key, value);
        }
        command
    }

    /// Runs it and captures the (scrubbed) output.
    pub(crate) fn output(&self) -> Result<GitOutput> {
        let op = self.describe();
        let out = self.command().output().map_err(|e| Error::git(&op, format!("spawn git: {e}")))?;
        let clean = |bytes: &[u8]| ecode_core::redact(&String::from_utf8_lossy(bytes), &self.scrub);
        Ok(GitOutput { success: out.status.success(), stdout: clean(&out.stdout), stderr: clean(&out.stderr) })
    }

    /// Runs it; `Err` on a non-zero exit, with the scrubbed stderr.
    pub fn run(&self) -> Result<String> {
        let out = self.output()?;
        if out.success {
            Ok(out.stdout)
        } else {
            let detail = if out.stderr.trim().is_empty() { out.stdout } else { out.stderr };
            Err(Error::git(self.describe(), detail.trim()))
        }
    }

    /// `git <first arg>`: the op's name in errors (never the full argv,
    /// which may hold a URL).
    fn describe(&self) -> String {
        let verb = self
            .args
            .iter()
            .map(|a| a.to_string_lossy())
            .find(|a| !a.starts_with('-') && !a.contains('='))
            .unwrap_or_default();
        format!("git {verb}")
    }
}
