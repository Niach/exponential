//! git's remote helper for `exp://<team>/<repo>` (#EXP-1616, eCode E3):
//! `git clone/fetch/push exp://…` on eCode's own git, where the team bucket
//! IS the remote ([`ecode::exp_git`]). Plain git clients reach eCode only
//! through this helper.
//!
//! The protocol (gitremote-helpers(7)): `capabilities`, `option`, `list`
//! (`for-push`), batches of `fetch <sha> <name>` and of `push
//! [+]<src>:<dst>`, each batch ended by a blank line. git sets `GIT_DIR`.
//!
//! Auth: a short-lived repo token. Hosts resolve it their own way
//! ([`main_with`]): the CLI and the desktop app from their signed-in
//! account (`repositories.credentials {remote}`), the standalone binary from
//! the environment ([`auth_from_env`]).

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ecode::exp_git::{ExpGit, RefPush};
use ecode::{CredentialSource, GitCommand};

/// Where the helper talks to, and as whom.
#[derive(Clone, Debug)]
pub struct Auth {
    /// The instance (`https://app.exponential.at`).
    pub server: String,
    /// A repo token (`expg_…`).
    pub token: String,
}

/// Mints a repo token for `url` with a session `bearer` on `server`.
pub fn auth_from_session(server: &str, bearer: &str, url: &str) -> Result<Auth, String> {
    let source = ecode::exponential::ExponentialCredentials::for_remote(server, bearer, url).map_err(|e| e.to_string())?;
    let credentials = source.credentials().map_err(|e| e.to_string())?;
    Ok(Auth { server: server.to_string(), token: credentials.token().to_string() })
}

/// The environment's credential (the standalone binary, device jobs, CI).
pub fn auth_from_env(url: &str) -> Result<Auth, String> {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    if let (Some(server), Some(token)) = (var("EXP_GIT_SERVER"), var("EXP_GIT_TOKEN")) {
        return Ok(Auth { server, token });
    }
    if let (Some(server), Some(bearer)) = (var("EXP_SERVER"), var("EXP_SESSION_TOKEN")) {
        return auth_from_session(&server, &bearer, url);
    }
    Err("no credential for exp:// remotes: sign in with `exponential login`, or set EXP_GIT_SERVER and EXP_GIT_TOKEN".into())
}

/// The helper's entry: argv `<remote> <url>`, stdin/stdout git's pipes.
pub fn main_with(resolve: impl Fn(&str) -> Result<Auth, String>) -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(url) = args.get(2).or(args.get(1)).cloned() else {
        eprintln!("usage: git-remote-exp <remote> <url>");
        return ExitCode::from(2);
    };
    let git_dir = std::env::var_os("GIT_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(".git"));
    let auth = match resolve(&url) {
        Ok(auth) => auth,
        Err(why) => {
            eprintln!("fatal: {why}");
            return ExitCode::from(128);
        }
    };
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match run(&git_dir, &auth, stdin.lock(), stdout.lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("fatal: {}", ecode::core::redact(&why, &[&auth.token]));
            ExitCode::from(128)
        }
    }
}

fn resolve_local(git_dir: &Path, rev: &str) -> Result<String, String> {
    GitCommand::new(None)
        .arg(format!("--git-dir={}", git_dir.display()))
        .args(["rev-parse", "--verify", &format!("{rev}^{{commit}}")])
        .run()
        .map(|sha| sha.trim().to_string())
        .map_err(|e| e.to_string())
}

/// Speaks the helper protocol until git closes the pipe.
pub fn run(git_dir: &Path, auth: &Auth, input: impl BufRead, mut output: impl Write) -> Result<(), String> {
    let git = ExpGit::new(&auth.server, auth.token.clone()).map_err(|e| e.to_string())?;
    let mut lines = input.lines();
    let io = |e: std::io::Error| e.to_string();
    while let Some(line) = lines.next() {
        let line = line.map_err(io)?;
        let command = line.trim_end();
        if command.is_empty() {
            break;
        }
        if command == "capabilities" {
            write!(output, "fetch\npush\noption\n\n").map_err(io)?;
        } else if let Some(option) = command.strip_prefix("option ") {
            let known = option.starts_with("verbosity ") || option.starts_with("progress ");
            writeln!(output, "{}", if known { "ok" } else { "unsupported" }).map_err(io)?;
        } else if command == "list" || command == "list for-push" {
            let remote = git.refs().map_err(|e| e.to_string())?;
            for (name, sha) in &remote.refs {
                writeln!(output, "{sha} {name}").map_err(io)?;
            }
            if let Some(head) = remote.head.filter(|head| remote.refs.contains_key(head)) {
                writeln!(output, "@{head} HEAD").map_err(io)?;
            }
            writeln!(output).map_err(io)?;
        } else if command.starts_with("fetch ") {
            // A batch: every `fetch` line, then a blank one.
            let mut wanted = vec![command.split(' ').nth(1).unwrap_or_default().to_string()];
            for next in lines.by_ref() {
                let next = next.map_err(io)?;
                if next.trim().is_empty() {
                    break;
                }
                wanted.extend(next.split(' ').nth(1).map(str::to_string));
            }
            let refs: Vec<&str> = wanted.iter().map(String::as_str).collect();
            git.fetch_wanted(git_dir, &refs).map_err(|e| e.to_string())?;
            writeln!(output).map_err(io)?;
        } else if command.starts_with("push ") {
            let mut specs = vec![command["push ".len()..].to_string()];
            for next in lines.by_ref() {
                let next = next.map_err(io)?;
                if next.trim().is_empty() {
                    break;
                }
                specs.extend(next.strip_prefix("push ").map(str::to_string));
            }
            let mut updates = Vec::new();
            let mut refused = Vec::new();
            for spec in specs {
                let (force, spec) = match spec.strip_prefix('+') {
                    Some(rest) => (true, rest.to_string()),
                    None => (false, spec),
                };
                let (src, dst) = spec.split_once(':').unwrap_or((spec.as_str(), spec.as_str()));
                if src.is_empty() {
                    updates.push(RefPush { src: None, dst: dst.to_string(), force });
                    continue;
                }
                match resolve_local(git_dir, src) {
                    Ok(sha) => updates.push(RefPush { src: Some(sha), dst: dst.to_string(), force }),
                    Err(why) => refused.push((dst.to_string(), why)),
                }
            }
            for (dst, result) in git.push(git_dir, &updates).map_err(|e| e.to_string())? {
                match result {
                    Ok(()) => writeln!(output, "ok {dst}").map_err(io)?,
                    Err(why) => writeln!(output, "error {dst} {why}").map_err(io)?,
                }
            }
            for (dst, why) in refused {
                writeln!(output, "error {dst} {}", why.lines().next().unwrap_or("unknown source")).map_err(io)?;
            }
            writeln!(output).map_err(io)?;
        } else {
            return Err(format!("unknown command `{command}`"));
        }
        output.flush().map_err(io)?;
    }
    Ok(())
}

/// Installs `git-remote-exp` as a link to `exe` in `dir` (so git finds it on
/// `PATH`); best effort, idempotent. The CLI and the desktop app call it at
/// start with their own executable.
pub fn install_link(exe: &Path, dir: &Path) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let link = dir.join(if cfg!(windows) { "git-remote-exp.exe" } else { "git-remote-exp" });
    if std::fs::read_link(&link).ok().as_deref() == Some(exe) {
        return Ok(link);
    }
    let _ = std::fs::remove_file(&link);
    #[cfg(unix)]
    std::os::unix::fs::symlink(exe, &link)?;
    #[cfg(not(unix))]
    std::fs::copy(exe, &link).map(|_| ())?;
    Ok(link)
}

/// Whether this process was started AS the helper (argv[0] is
/// `git-remote-exp`): the CLI and the desktop app check this first.
pub fn invoked_as_helper() -> bool {
    std::env::args_os()
        .next()
        .and_then(|arg0| Path::new(&arg0).file_stem().map(|s| s.to_string_lossy().into_owned()))
        .is_some_and(|stem| stem == "git-remote-exp")
}
