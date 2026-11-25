//! #FEED-308: how long a spawn's command line may be, and where its long
//! texts go instead.
//!
//! Windows' `CreateProcess` takes at most 32,767 characters of command line
//! and answers a longer one with `ERROR_FILENAME_EXCED_RANGE` ("os error
//! 206", "the filename or extension is too long"), which names nothing. A
//! system prompt on argv crossed it for every claude start. So:
//!
//! - a text that can grow (the system prompt, an MCP config) rides a FILE
//!   ([`write_launch_file`]) on every platform, never argv;
//! - every spawn is measured first ([`check_spawn`]) and one over its
//!   platform's limit fails with the argument that is too big.

use std::path::{Path, PathBuf};

/// What a spawn is measured against (`Limits::host` = this machine's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub platform: Platform,
    /// The whole command line (Windows: characters; unix: bytes of argv and
    /// environment, with their pointers).
    pub total: usize,
    /// One argument alone (Linux' `MAX_ARG_STRLEN`).
    pub per_arg: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Unix,
}

/// Under `CreateProcess`' 32,767, with room for the quoting this estimate
/// rounds up.
pub const WINDOWS_COMMAND_LINE_MAX: usize = 30_000;
/// A `.cmd`/`.bat` program (npm's `claude.cmd`) runs through `cmd.exe`,
/// which stops at 8,191.
pub const WINDOWS_BATCH_COMMAND_LINE_MAX: usize = 8_000;
/// Linux refuses one argument or variable of 32 pages or more (`E2BIG`).
pub const LINUX_ARG_MAX_STRLEN: usize = 131_072;
/// Room left under `ARG_MAX` for what the kernel adds (auxv, the path).
const UNIX_HEADROOM: usize = 4096;

impl Limits {
    pub fn windows(program: &str) -> Limits {
        let lower = program.to_ascii_lowercase();
        let batch = lower.ends_with(".cmd") || lower.ends_with(".bat");
        Limits {
            platform: Platform::Windows,
            total: if batch { WINDOWS_BATCH_COMMAND_LINE_MAX } else { WINDOWS_COMMAND_LINE_MAX },
            per_arg: None,
        }
    }

    pub fn unix(arg_max: usize, per_arg: Option<usize>) -> Limits {
        Limits { platform: Platform::Unix, total: arg_max.saturating_sub(UNIX_HEADROOM), per_arg }
    }

    /// This machine's limits for spawning `program`.
    pub fn host(program: &str) -> Limits {
        #[cfg(windows)]
        {
            Limits::windows(program)
        }
        #[cfg(not(windows))]
        {
            let _ = program;
            // SAFETY: `sysconf` only reads a constant of the running system.
            let arg_max = unsafe { libc::sysconf(libc::_SC_ARG_MAX) };
            let arg_max = if arg_max > 0 { arg_max as usize } else { 256 * 1024 };
            Limits::unix(arg_max, cfg!(target_os = "linux").then_some(LINUX_ARG_MAX_STRLEN - 1))
        }
    }
}

/// `argv` (program first) and the child's environment (`NAME=value` sizes in
/// bytes) against `limits`. `Err` = the message for the person: the size,
/// the limit and the biggest argument by its flag.
pub fn check(argv: &[String], env_bytes: usize, limits: &Limits) -> Result<(), String> {
    let cost = |arg: &String| match limits.platform {
        Platform::Windows => windows_cost(arg),
        // The string, its NUL and its pointer.
        Platform::Unix => arg.len() + 1 + std::mem::size_of::<usize>(),
    };
    let biggest = || {
        let (index, arg) = argv.iter().enumerate().max_by_key(|(_, arg)| cost(arg))?;
        Some((argument_name(argv, index), cost(arg)))
    };
    if let Some(per_arg) = limits.per_arg {
        if let Some((index, arg)) = argv.iter().enumerate().find(|(_, arg)| arg.len() > per_arg) {
            return Err(format!(
                "{} is {} bytes, over the limit of {} for one argument",
                argument_name(argv, index),
                group(arg.len()),
                group(per_arg)
            ));
        }
    }
    let total = argv.iter().map(cost).sum::<usize>()
        + match limits.platform {
            Platform::Windows => 0,
            Platform::Unix => env_bytes,
        };
    if total <= limits.total {
        return Ok(());
    }
    let (unit, system) = match limits.platform {
        Platform::Windows => ("characters", "Windows' limit"),
        Platform::Unix => ("bytes with its environment", "the system's limit"),
    };
    let culprit = biggest()
        .map(|(name, size)| format!(": {name} takes {}", group(size)))
        .unwrap_or_default();
    Err(format!(
        "the command line is {} {unit}, over {system} of {}{culprit}",
        group(total),
        group(limits.total)
    ))
}

/// [`check`] for a spawn on this machine: `program` with `args`, the
/// inherited environment plus the `env` overlay.
pub fn check_spawn(program: &str, args: &[String], env: &[(String, String)]) -> std::io::Result<()> {
    let mut argv = Vec::with_capacity(args.len() + 1);
    argv.push(program.to_string());
    argv.extend(args.iter().cloned());
    let variable = |name: usize, value: usize| name + value + 2 + std::mem::size_of::<usize>();
    let env_bytes = std::env::vars_os().map(|(name, value)| variable(name.len(), value.len())).sum::<usize>()
        + env.iter().map(|(name, value)| variable(name.len(), value.len())).sum::<usize>();
    check(&argv, env_bytes, &Limits::host(program))
        .map_err(|message| std::io::Error::new(std::io::ErrorKind::InvalidInput, message))
}

/// An upper bound of what `arg` adds to a Windows command line: its UTF-16
/// units, the separating space, the quotes, and one escape per quote and
/// backslash.
fn windows_cost(arg: &String) -> usize {
    arg.encode_utf16().count() + 3 + arg.chars().filter(|ch| matches!(ch, '"' | '\\')).count()
}

/// How a person finds argument `index`: `--flag` for `--flag=value` and for
/// the value after a flag, else its position.
fn argument_name(argv: &[String], index: usize) -> String {
    let arg = &argv[index];
    if arg.starts_with('-') {
        return arg.split('=').next().unwrap_or(arg).chars().take(64).collect();
    }
    match index.checked_sub(1).and_then(|before| argv.get(before)) {
        Some(flag) if index > 1 && flag.starts_with('-') => format!("the value of {flag}"),
        _ if index == 0 => "the program path".to_string(),
        _ => format!("argument {index}"),
    }
}

/// `12345` as `12,345`.
fn group(number: usize) -> String {
    let digits = number.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A launch's long text as a file under `dir`, named by its content
/// (`<prefix>-<hash>.<ext>`, 16 hex digits: short for Windows' `MAX_PATH`,
/// and the launches that carry the same text share one file). Written once,
/// in one rename, so a CLI that is reading it never sees half of it.
/// [`prune_launch_files`] drops the ones no launch named for a while.
pub fn write_launch_file(dir: &Path, prefix: &str, ext: &str, content: &str) -> std::io::Result<PathBuf> {
    let path = dir.join(format!("{prefix}-{:016x}.{ext}", fnv1a(content.as_bytes())));
    if std::fs::read(&path).is_ok_and(|existing| existing == content.as_bytes()) {
        // Used again: its age starts over, so a prune by age never takes a
        // file a launch is about to name.
        let _ = std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_modified(std::time::SystemTime::now()));
        return Ok(path);
    }
    crate::atomic_file::write_atomic(&path, content).map_err(|err| {
        std::io::Error::new(err.kind(), format!("could not write the launch file {}: {err}", path.display()))
    })?;
    Ok(path)
}

/// Remove the files directly under `dir` that no launch wrote or reused for
/// `ttl`. Best effort.
pub fn prune_launch_files(dir: &Path, ttl: std::time::Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| modified.elapsed().is_ok_and(|age| age > ttl));
        if stale && entry.path().is_file() {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// FNV-1a, 64 bit: a stable name for a text, never a security boundary.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    #[test]
    fn a_short_command_line_passes_everywhere() {
        let line = argv(&["claude", "-p", "--model", "opus"]);
        assert_eq!(check(&line, 4096, &Limits::windows("claude.exe")), Ok(()));
        assert_eq!(check(&line, 4096, &Limits::unix(1024 * 1024, Some(LINUX_ARG_MAX_STRLEN - 1))), Ok(()));
    }

    /// #FEED-308: what Windows answered with "os error 206" now says which
    /// argument is too big.
    #[test]
    fn a_prompt_on_argv_is_refused_on_windows_by_its_flag() {
        let prompt = "x".repeat(40_000);
        let line = argv(&["claude.exe", "-p", "--append-system-prompt", &prompt, "--model", "opus"]);
        let message = check(&line, 0, &Limits::windows("claude.exe")).unwrap_err();
        assert!(message.contains("over Windows' limit of 30,000"), "{message}");
        assert!(message.contains("the value of --append-system-prompt takes 40,003"), "{message}");
        // The same line is fine where the limit is a megabyte.
        assert_eq!(check(&line, 0, &Limits::unix(1024 * 1024, None)), Ok(()));
    }

    #[test]
    fn a_batch_program_gets_cmd_exes_limit() {
        assert_eq!(Limits::windows(r"C:\npm\claude.CMD").total, WINDOWS_BATCH_COMMAND_LINE_MAX);
        assert_eq!(Limits::windows(r"C:\bin\claude.exe").total, WINDOWS_COMMAND_LINE_MAX);
        let line = argv(&["claude.cmd", &format!("--mcp-config={}", "y".repeat(9_000))]);
        let message = check(&line, 0, &Limits::windows("claude.cmd")).unwrap_err();
        assert!(message.contains("--mcp-config takes"), "{message}");
        assert!(!message.contains("yyyy"), "the value itself stays out: {message}");
    }

    #[test]
    fn windows_counts_utf16_units_and_escapes() {
        // One astral char = two units; a quote and a backslash one escape each.
        assert_eq!(windows_cost(&"a\u{1F600}".to_string()), 3 + 3);
        assert_eq!(windows_cost(&r#"a"\"#.to_string()), 3 + 3 + 2);
    }

    #[test]
    fn unix_counts_the_environment_and_one_argument() {
        let line = argv(&["codex", "-c", &"z".repeat(200_000)]);
        let linux = Limits::unix(2 * 1024 * 1024, Some(LINUX_ARG_MAX_STRLEN - 1));
        let message = check(&line, 0, &linux).unwrap_err();
        assert!(message.contains("the value of -c is 200,000 bytes"), "{message}");
        let small = argv(&["codex", "app-server"]);
        let message = check(&small, 300_000, &Limits::unix(256 * 1024, None)).unwrap_err();
        assert!(message.contains("bytes with its environment"), "{message}");
    }

    #[test]
    fn this_machines_limits_take_an_ordinary_spawn() {
        check_spawn("claude", &argv(&["-p", "--model", "opus"]), &[("EXP_MCP_TOKEN".to_string(), "k".repeat(64))])
            .expect("an ordinary spawn fits");
        let err = check_spawn("claude", &argv(&["--append-system-prompt", &"p".repeat(8 * 1024 * 1024)]), &[])
            .expect_err("8 MB of argv fits nowhere");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(err.to_string().contains("--append-system-prompt"), "{err}");
    }

    #[test]
    fn a_launch_file_is_named_by_its_content_and_stays_short() {
        let dir = std::env::temp_dir().join(format!("exp-launch-file-{}-{}", std::process::id(), uuid::Uuid::new_v4()));
        let text = "p".repeat(100_000);
        let first = write_launch_file(&dir, "prompt", "md", &text).unwrap();
        assert_eq!(std::fs::read_to_string(&first).unwrap(), text);
        assert_eq!(write_launch_file(&dir, "prompt", "md", &text).unwrap(), first, "the same text, one file");
        let other = write_launch_file(&dir, "prompt", "md", "another").unwrap();
        assert_ne!(other, first);
        // `prompt-<16 hex>.md`: far under MAX_PATH below any data dir.
        assert_eq!(first.file_name().unwrap().len(), "prompt-".len() + 16 + ".md".len());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2, "no temp file is left");
        // A file a launch reused is young again; one nobody named goes.
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        for file in [&first, &other] {
            std::fs::OpenOptions::new().write(true).open(file).unwrap().set_modified(old).unwrap();
        }
        write_launch_file(&dir, "prompt", "md", &text).unwrap();
        prune_launch_files(&dir, std::time::Duration::from_secs(60));
        assert!(first.exists() && !other.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
