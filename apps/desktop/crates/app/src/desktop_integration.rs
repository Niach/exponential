//! Linux/BSD desktop integration: register THIS build as the handler for the
//! `exponential://` OAuth-callback / invite deep-link scheme (§5.7, scheme
//! literal centralized in `api::login::OAUTH_CALLBACK_SCHEME` — EXP-41).
//!
//! A packaged `.deb`/tarball ships a `.desktop` with `MimeType=x-scheme-
//! handler/exponential;`, but an AppImage (or a bare `cargo`/dev binary)
//! registers nothing — so the browser's `exponential://oauth-return#token=…`
//! callback has no handler and never reaches the app. We install the
//! `.desktop` ourselves, idempotently, on every primary launch:
//!
//! - `Exec=` points at the AppImage (`$APPIMAGE`, set inside AppImages) when
//!   packaged, else at `current_exe()` (installed binary / dev build).
//! - We only rewrite the file when its contents change, then refresh the
//!   desktop DB, and we assert ourselves as the `x-scheme-handler/exponential`
//!   default in `mimeapps.list` (so the channel you're running wins if both a
//!   prod and a staging build are installed). Installs that registered the
//!   pre-rename `exp://` scheme get their stale `x-scheme-handler/exp` default
//!   dropped in the same write.
//!
//! Everything here is best-effort: a failure just means deep links won't route
//! until a package installs the handler — it must never block startup.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::channel::{APP_ID, APP_NAME};

fn scheme_entry() -> String {
    format!("x-scheme-handler/{}", api::login::OAUTH_CALLBACK_SCHEME)
}

/// The pre-rename scheme's mimeapps key (EXP-41): dropped from the default
/// list whenever we assert the current one, so old installs don't keep a dead
/// `exp://` claim around.
const LEGACY_SCHEME_ENTRY: &str = "x-scheme-handler/exp";

/// The executable the browser should launch for an `exponential://` callback:
/// the AppImage path when packaged, otherwise this binary.
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

/// The white-on-transparent logo (the variant that reads on the dark shelves
/// desktop Linux defaults to), embedded so the RUNNING app can install it as
/// its hicolor icon — the AppImage's bundled icon lives inside the squashfs
/// where no icon theme can see it (EXP-68).
const ICON_SVG: &[u8] = include_bytes!("../../../assets/icons/logo-white.svg");

/// Install the app icon into the user hicolor theme
/// (`~/.local/share/icons/hicolor/scalable/apps/<APP_ID>.svg`) so the
/// `.desktop`'s `Icon={APP_ID}` resolves. Idempotent; best-effort.
fn ensure_icon_installed() {
    let Some(data_dir) = dirs::data_dir() else {
        return;
    };
    let icon_dir = data_dir.join("icons/hicolor/scalable/apps");
    let icon_path = icon_dir.join(format!("{APP_ID}.svg"));
    let current = std::fs::read(&icon_path).ok();
    if current.as_deref() == Some(ICON_SVG) {
        return;
    }
    if let Err(err) = std::fs::create_dir_all(&icon_dir)
        .and_then(|()| std::fs::write(&icon_path, ICON_SVG))
    {
        eprintln!("[exp-desktop] icon install failed: {err:#}");
    }
}

/// Install the `.desktop` handler and make it the default `exponential://`
/// handler.
pub fn ensure_scheme_registered() {
    let Some(exec) = launch_target() else { return };
    let Some(apps_dir) = applications_dir() else {
        return;
    };
    ensure_icon_installed();
    let desktop_path = apps_dir.join(desktop_file_name());

    // Taskbar icon association (EXP-68): the window's Wayland app_id / X11
    // WM_CLASS is `APP_ID` (set in `windows::open_workspace_window`), so the
    // compositor matches it against THIS file two ways — the desktop-file id
    // (`<APP_ID>.desktop`) and `StartupWMClass` — and pulls `Icon=` from it.
    // Packaged runs (AppImage) list a real launcher entry; bare dev binaries
    // stay `NoDisplay=true` so a `target/debug` path never lingers in app
    // grids (the icon/WM_CLASS matching works for NoDisplay entries too).
    let packaged = std::env::var_os("APPIMAGE").is_some();
    let contents = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={APP_NAME}\n\
         Comment=Exponential desktop IDE\n\
         Exec={exec} %U\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         {no_display}\
         Categories=Development;\n\
         MimeType={scheme_entry};\n\
         StartupWMClass={APP_ID}\n",
        exec = exec.display(),
        no_display = if packaged { "" } else { "NoDisplay=true\n" },
        scheme_entry = scheme_entry(),
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

/// Ensure `mimeapps.list` maps `x-scheme-handler/exponential` to our
/// `.desktop` under `[Default Applications]` (what `xdg-mime default` writes)
/// — done in-process so it works without the `xdg-utils` binaries. Also drops
/// the stale pre-rename `x-scheme-handler/exp` default (EXP-41 cleanup).
/// Rewrites only on change.
fn ensure_default_handler(desktop_name: &str) -> std::io::Result<()> {
    let Some(config_dir) = dirs::config_dir() else {
        return Ok(());
    };
    let path = config_dir.join("mimeapps.list");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let entry = scheme_entry();
    let desired_line = format!("{entry}={desktop_name}");

    let updated = upsert_default_application(&existing, &entry, &desired_line);
    let updated = remove_default_application(&updated, LEGACY_SCHEME_ENTRY);
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

/// Pure helper: return `contents` with any `key=` line removed from the
/// `[Default Applications]` section (other sections untouched).
fn remove_default_application(contents: &str, key: &str) -> String {
    let key_prefix = format!("{key}=");
    let lines: Vec<&str> = contents.lines().collect();

    let Some(start) = lines
        .iter()
        .position(|line| line.trim() == "[Default Applications]")
    else {
        return contents.to_string();
    };
    let end = lines[start + 1..]
        .iter()
        .position(|line| line.trim_start().starts_with('['))
        .map(|offset| start + 1 + offset)
        .unwrap_or(lines.len());

    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    kept.extend_from_slice(&lines[..start + 1]);
    kept.extend(
        lines[start + 1..end]
            .iter()
            .filter(|line| !line.trim_start().starts_with(&key_prefix)),
    );
    kept.extend_from_slice(&lines[end..]);
    if kept.len() == lines.len() {
        return contents.to_string();
    }

    let mut out = kept.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::{remove_default_application, upsert_default_application};

    const KEY: &str = "x-scheme-handler/exponential";
    const LINE: &str = "x-scheme-handler/exponential=at.exponential.desktop";

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
            "[Default Applications]\nx-scheme-handler/exponential=old.desktop\nx-scheme-handler/claude=c.desktop\n";
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

    #[test]
    fn removes_stale_legacy_scheme_default() {
        // The pre-rename `exp` entry goes; the new `exponential` entry (which
        // shares the prefix) and unrelated keys stay.
        let input = format!(
            "[Default Applications]\nx-scheme-handler/exp=at.exponential.desktop\n{LINE}\n\
             [Added Associations]\nx-scheme-handler/exp=y.desktop;\n"
        );
        let out = remove_default_application(&input, "x-scheme-handler/exp");
        assert!(!out.contains("x-scheme-handler/exp=at.exponential.desktop"));
        assert!(out.contains(LINE));
        // Other sections are untouched.
        assert!(out.contains("x-scheme-handler/exp=y.desktop;"));
    }

    #[test]
    fn remove_is_noop_without_the_key() {
        let input = format!("[Default Applications]\n{LINE}\n");
        let out = remove_default_application(&input, "x-scheme-handler/exp");
        assert_eq!(out, input);
    }
}
