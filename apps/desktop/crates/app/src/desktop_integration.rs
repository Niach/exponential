//! Linux/BSD desktop integration: register THIS build as the handler for the
//! `exp://` OAuth-callback / invite deep-link scheme (§5.7).
//!
//! A packaged `.deb`/tarball ships a `.desktop` with `MimeType=x-scheme-
//! handler/exp;`, but an AppImage (or a bare `cargo`/dev binary) registers
//! nothing — so the browser's `exp://oauth-return#token=…` callback has no
//! handler and never reaches the app. We install the `.desktop` ourselves,
//! idempotently, on every primary launch:
//!
//! - `Exec=` points at the AppImage (`$APPIMAGE`, set inside AppImages) when
//!   packaged, else at `current_exe()` (installed binary / dev build).
//! - We only rewrite the file when its contents change, then refresh the
//!   desktop DB, and we assert ourselves as the `x-scheme-handler/exp` default
//!   in `mimeapps.list` (so the channel you're running wins if both a prod and
//!   a staging build are installed).
//!
//! Everything here is best-effort: a failure just means deep links won't route
//! until a package installs the handler — it must never block startup.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::channel::{APP_ID, APP_NAME};

const SCHEME_ENTRY: &str = "x-scheme-handler/exp";

/// The executable the browser should launch for an `exp://` callback: the
/// AppImage path when packaged, otherwise this binary.
fn launch_target() -> Option<PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE") {
        return Some(PathBuf::from(appimage));
    }
    std::env::current_exe().ok()
}

fn applications_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join("applications"))
}

fn desktop_file_name() -> String {
    format!("{APP_ID}.desktop")
}

/// Install the `.desktop` handler and make it the default `exp://` handler.
pub fn ensure_scheme_registered() {
    let Some(exec) = launch_target() else { return };
    let Some(apps_dir) = applications_dir() else {
        return;
    };
    let desktop_path = apps_dir.join(desktop_file_name());

    let contents = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={APP_NAME}\n\
         Comment=Exponential desktop IDE\n\
         Exec={exec} %U\n\
         Terminal=false\n\
         NoDisplay=true\n\
         Categories=Development;\n\
         MimeType={SCHEME_ENTRY};\n\
         StartupWMClass=exp-desktop\n",
        exec = exec.display(),
    );

    // Only rewrite (and re-index) when something actually changed.
    let changed = std::fs::read_to_string(&desktop_path)
        .map(|existing| existing != contents)
        .unwrap_or(true);
    if changed {
        if let Err(err) = std::fs::create_dir_all(&apps_dir)
            .and_then(|()| std::fs::write(&desktop_path, &contents))
        {
            eprintln!("[exp-desktop] scheme registration: write failed: {err:#}");
            return;
        }
        refresh_desktop_database(&apps_dir);
    }

    if let Err(err) = ensure_default_handler(&desktop_file_name()) {
        eprintln!("[exp-desktop] scheme registration: set-default failed: {err:#}");
    }
}

/// Best-effort `update-desktop-database` so the new `.desktop`'s MimeType is
/// indexed into `mimeinfo.cache` (no-op if the tool isn't installed).
fn refresh_desktop_database(apps_dir: &Path) {
    let _ = Command::new("update-desktop-database").arg(apps_dir).status();
}

/// Ensure `mimeapps.list` maps `x-scheme-handler/exp` to our `.desktop` under
/// `[Default Applications]` (what `xdg-mime default` writes) — done in-process
/// so it works without the `xdg-utils` binaries. Rewrites only on change.
fn ensure_default_handler(desktop_name: &str) -> std::io::Result<()> {
    let Some(config_dir) = dirs::config_dir() else {
        return Ok(());
    };
    let path = config_dir.join("mimeapps.list");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let desired_line = format!("{SCHEME_ENTRY}={desktop_name}");

    let updated = upsert_default_application(&existing, SCHEME_ENTRY, &desired_line);
    if updated == existing {
        return Ok(());
    }

    std::fs::create_dir_all(&config_dir)?;
    let mut file = std::fs::File::create(&path)?;
    file.write_all(updated.as_bytes())
}

/// Pure helper: return `contents` with `key=` set to `desired_line` inside the
/// `[Default Applications]` section, adding the section/line if absent.
fn upsert_default_application(contents: &str, key: &str, desired_line: &str) -> String {
    let key_prefix = format!("{key}=");
    let mut lines: Vec<String> = contents.lines().map(str::to_string).collect();

    // Find the [Default Applications] section span.
    let section_start = lines
        .iter()
        .position(|line| line.trim() == "[Default Applications]");

    if let Some(start) = section_start {
        // Section end = next section header, or EOF.
        let end = lines[start + 1..]
            .iter()
            .position(|line| line.trim_start().starts_with('['))
            .map(|offset| start + 1 + offset)
            .unwrap_or(lines.len());

        if let Some(existing) = lines[start + 1..end]
            .iter_mut()
            .find(|line| line.trim_start().starts_with(&key_prefix))
        {
            *existing = desired_line.to_string();
        } else {
            lines.insert(end, desired_line.to_string());
        }
    } else {
        if !lines.is_empty() && !lines.last().map(|l| l.is_empty()).unwrap_or(true) {
            lines.push(String::new());
        }
        lines.push("[Default Applications]".to_string());
        lines.push(desired_line.to_string());
    }

    let mut out = lines.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::upsert_default_application;

    const KEY: &str = "x-scheme-handler/exp";
    const LINE: &str = "x-scheme-handler/exp=at.exponential.desktop";

    #[test]
    fn adds_section_to_empty_file() {
        let out = upsert_default_application("", KEY, LINE);
        assert!(out.contains("[Default Applications]"));
        assert!(out.contains(LINE));
    }

    #[test]
    fn adds_line_to_existing_section() {
        let input = "[Default Applications]\ntext/html=brave.desktop\n";
        let out = upsert_default_application(input, KEY, LINE);
        assert!(out.contains("text/html=brave.desktop"));
        assert!(out.contains(LINE));
        assert_eq!(out.matches("[Default Applications]").count(), 1);
    }

    #[test]
    fn replaces_existing_handler() {
        let input =
            "[Default Applications]\nx-scheme-handler/exp=old.desktop\nx-scheme-handler/claude=c.desktop\n";
        let out = upsert_default_application(input, KEY, LINE);
        assert!(out.contains(LINE));
        assert!(!out.contains("old.desktop"));
        assert!(out.contains("x-scheme-handler/claude=c.desktop"));
    }

    #[test]
    fn no_change_when_already_correct() {
        let input = format!("[Default Applications]\n{LINE}\n");
        let out = upsert_default_application(&input, KEY, LINE);
        assert_eq!(out, input);
    }

    #[test]
    fn inserts_before_next_section() {
        let input = "[Default Applications]\ntext/html=brave.desktop\n\n[Added Associations]\nx=y.desktop;\n";
        let out = upsert_default_application(input, KEY, LINE);
        let exp_pos = out.find(LINE).unwrap();
        let added_pos = out.find("[Added Associations]").unwrap();
        assert!(exp_pos < added_pos, "handler must land in Default Applications");
    }
}
