//! Background child-process helpers shared by every crate that shells out
//! (`coding`, `ui`, `steer`) — EXP-419.
//!
//! The GUI app is `windows_subsystem = "windows"`, so it owns no console;
//! every console-subsystem child (`git`, `claude`, `reg.exe`, …) spawned with
//! a plain `std::process::Command` allocates a **visible** conhost window for
//! the few milliseconds it runs. [`background_command`] is the one Command
//! constructor for captured/background spawns; interactive children go
//! through the ConPTY path in [`crate::pty`] instead.
//!
//! [`refresh_windows_path`] solves the sibling problem: a running Windows
//! process never sees PATH edits an installer wrote to the registry, so
//! "install git → Check tools" stayed red until an app restart. Re-reading
//! the registry and updating the process env lets the doctor — and every
//! later spawn, including clones and agent launches — pick the tool up live.

use std::ffi::OsStr;

/// A `std::process::Command` that never flashes a console window on Windows.
/// Use for every spawn whose output is captured or ignored; no-op difference
/// on unix.
pub fn background_command(program: impl AsRef<OsStr>) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Re-read the machine+user `Path` from the registry and update this
/// process's `PATH` so children inherit a fresh view (Windows only; no-op
/// elsewhere). Installers edit the registry and broadcast a change message,
/// but an already-running process's environment block is never updated —
/// without this, a freshly installed `git`/`claude` is invisible until
/// restart.
///
/// Entries the process already carries but the registry doesn't (launcher or
/// app-injected dirs) are preserved, appended after the registry entries.
pub fn refresh_windows_path() {
    #[cfg(windows)]
    {
        let system = query_registry_path(
            r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        );
        let user = query_registry_path(r"HKCU\Environment");
        if system.is_none() && user.is_none() {
            return; // reg.exe unavailable/failed — keep the current env
        }
        let process = std::env::var("PATH").unwrap_or_default();
        let merged = merge_windows_path(
            system.as_deref().unwrap_or(""),
            user.as_deref().unwrap_or(""),
            &process,
        );
        if !merged.is_empty() {
            // Safe pre-2024-edition; the Windows env APIs are thread-safe.
            std::env::set_var("PATH", merged);
        }
    }
}

/// `reg.exe query <key> /v Path` → the expanded value. Spawned hidden; any
/// failure (missing value, weird output) is `None` — the caller treats that
/// as "leave the env alone".
#[cfg(windows)]
fn query_registry_path(key: &str) -> Option<String> {
    let output = background_command("reg.exe")
        .args(["query", key, "/v", "Path"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw = parse_reg_query_value(&stdout)?;
    Some(expand_env_placeholders(&raw, |name| std::env::var(name).ok()))
}

/// Pull the data out of a `reg.exe query … /v Path` output line:
///
/// ```text
/// HKEY_CURRENT_USER\Environment
///     Path    REG_EXPAND_SZ    %USERPROFILE%\.local\bin;C:\tools
/// ```
///
/// The data may itself contain spaces, so split on the `REG_*` type token
/// rather than whitespace columns.
#[cfg_attr(not(windows), allow(dead_code))]
fn parse_reg_query_value(stdout: &str) -> Option<String> {
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        // Value-name match is case-insensitive ("Path" vs "PATH").
        let Some(rest) = trimmed
            .strip_prefix("Path")
            .or_else(|| trimmed.strip_prefix("PATH"))
            .or_else(|| trimmed.strip_prefix("path"))
        else {
            continue;
        };
        let rest = rest.trim_start();
        for reg_type in ["REG_EXPAND_SZ", "REG_SZ"] {
            if let Some(value) = rest.strip_prefix(reg_type) {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
        return None;
    }
    None
}

/// `%NAME%` → `lookup(NAME)`, unknown names left verbatim (matching
/// `ExpandEnvironmentStrings`). Lookup is case-insensitive on real Windows;
/// the injected `lookup` decides (tests pass exact names).
#[cfg_attr(not(windows), allow(dead_code))]
fn expand_env_placeholders(value: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) => {
                let name = &after[..end];
                match lookup(name) {
                    Some(replacement) if !name.is_empty() => out.push_str(&replacement),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                // Unmatched trailing % — keep verbatim.
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Windows PATH semantics: system entries, then user entries, then whatever
/// extra entries the process already had — `;`-joined, deduped
/// case-insensitively (first occurrence wins), empties dropped.
#[cfg_attr(not(windows), allow(dead_code))]
fn merge_windows_path(system: &str, user: &str, process: &str) -> String {
    let mut seen: Vec<String> = Vec::new();
    let mut parts: Vec<&str> = Vec::new();
    for part in system.split(';').chain(user.split(';')).chain(process.split(';')) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let key = part.to_ascii_lowercase();
        if !seen.contains(&key) {
            seen.push(key);
            parts.push(part);
        }
    }
    parts.join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reg_query_output() {
        let out = "\r\nHKEY_CURRENT_USER\\Environment\r\n    Path    REG_EXPAND_SZ    %USERPROFILE%\\.local\\bin;C:\\Program Files\\Git\\cmd\r\n\r\n";
        assert_eq!(
            parse_reg_query_value(out).as_deref(),
            Some("%USERPROFILE%\\.local\\bin;C:\\Program Files\\Git\\cmd")
        );
    }

    #[test]
    fn parses_reg_sz_and_case_insensitive_name() {
        let out = "HKEY_LOCAL_MACHINE\\...\\Environment\n    PATH    REG_SZ    C:\\Windows;C:\\Windows\\system32\n";
        assert_eq!(
            parse_reg_query_value(out).as_deref(),
            Some("C:\\Windows;C:\\Windows\\system32")
        );
    }

    #[test]
    fn reg_parse_misses_are_none() {
        assert_eq!(parse_reg_query_value(""), None);
        assert_eq!(parse_reg_query_value("ERROR: The system was unable to find the specified registry key or value."), None);
    }

    #[test]
    fn expands_known_placeholders_keeps_unknown() {
        let expanded = expand_env_placeholders(
            "%USERPROFILE%\\.local\\bin;%NOPE%\\x;50%",
            |name| (name == "USERPROFILE").then(|| "C:\\Users\\d".to_string()),
        );
        assert_eq!(expanded, "C:\\Users\\d\\.local\\bin;%NOPE%\\x;50%");
    }

    #[test]
    fn merges_system_user_process_dedup_case_insensitive() {
        let merged = merge_windows_path(
            "C:\\Windows;C:\\Windows\\system32",
            "C:\\Users\\d\\.local\\bin;c:\\windows",
            "C:\\Windows\\system32;C:\\app-injected",
        );
        assert_eq!(
            merged,
            "C:\\Windows;C:\\Windows\\system32;C:\\Users\\d\\.local\\bin;C:\\app-injected"
        );
    }

    #[test]
    fn merge_all_empty_is_empty() {
        assert_eq!(merge_windows_path("", "", ""), "");
    }
}
