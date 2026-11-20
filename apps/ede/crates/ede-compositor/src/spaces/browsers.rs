//! #EDE-138: a browser launched into a space gets a profile of the space.
//!
//! Without one, a space's browser and the person's meet. Snaps and Flatpaks
//! keep their data under the person's `HOME` (`~/snap/<name>`,
//! `~/.var/app/<id>`), which the space's own profile directories do not
//! change. With the person's Chromium open, the space's hands its window to
//! it (a window on the PERSON's desk); with it closed, the agent browses in
//! the person's signed-in profile. Firefox says "already running".
//!
//! So a known browser ([`KNOWN`]) gets its profile flag: Chromium's family
//! `--user-data-dir=`, Firefox's `--profile <dir> --new-instance`. The
//! directory is the space's own home for a browser of the system. For a
//! snap or a Flatpak it is inside the app's own data directory, the one
//! place the app may write: `~/snap/<name>/common/exponential-space/p<pid>-<space>`
//! or `~/.var/app/<id>/exponential-space/p<pid>-<space>`. It goes when the
//! space closes, and a launch that names a profile itself is refused.
//!
//! While the space renders in software ([`super::render`]), Chromium and
//! Electron also draw WebGL with SwiftShader (`--use-angle=swiftshader`): on
//! a GPU path that cannot present, their WebGL stays blank rather than
//! failing in a way they notice. `--enable-unsafe-swiftshader` is always
//! there, so ANGLE may fall back to SwiftShader when its GPU path fails.
//! Firefox is given nothing: on llvmpipe it switches to software WebRender
//! by itself.
//!
//! A snap of the Chromium family also gets `--ozone-platform=x11`: the
//! snap's AppArmor admits no Wayland socket but `/run/user/<uid>/wayland-N`,
//! so the space's is refused, and Chromium then exits where GTK (Firefox)
//! falls back to the space's Xwayland by itself (#EDE-180).
//!
//! A browser this table does not know runs as any other program: as a
//! snap or a Flatpak, with the person's profile (#EDE-166).

use std::{
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};

use ede_protocol::{ErrorCode, Failure};

use crate::{desk::DeskId, state::Ede};

/// Which profile flag and switches a browser takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `--user-data-dir=<dir>`, and the SwiftShader switches; as a snap,
    /// `--ozone-platform=x11`.
    Chromium,
    /// `--profile <dir> --new-instance`.
    Firefox,
    /// The SwiftShader switches alone (its data is under the space's
    /// `XDG_CONFIG_HOME` already).
    Electron,
}

/// A browser the table knows: the names it runs by, its snap's, its
/// Flatpaks' ids.
pub struct Known {
    pub names: &'static [&'static str],
    pub snap: Option<&'static str>,
    pub flatpaks: &'static [&'static str],
    pub kind: Kind,
}

/// The browsers a space gives a profile.
pub const KNOWN: &[Known] = &[
    Known { names: &["chromium", "chromium-browser"], snap: Some("chromium"), flatpaks: &["org.chromium.Chromium", "io.github.ungoogled_software.ungoogled_chromium"], kind: Kind::Chromium },
    Known { names: &["google-chrome", "google-chrome-stable", "google-chrome-beta", "google-chrome-unstable"], snap: None, flatpaks: &["com.google.Chrome"], kind: Kind::Chromium },
    Known { names: &["brave-browser", "brave-browser-stable", "brave"], snap: Some("brave"), flatpaks: &["com.brave.Browser"], kind: Kind::Chromium },
    Known { names: &["microsoft-edge", "microsoft-edge-stable"], snap: None, flatpaks: &["com.microsoft.Edge"], kind: Kind::Chromium },
    Known { names: &["vivaldi", "vivaldi-stable"], snap: None, flatpaks: &["com.vivaldi.Vivaldi"], kind: Kind::Chromium },
    Known { names: &["firefox", "firefox-esr"], snap: Some("firefox"), flatpaks: &["org.mozilla.firefox"], kind: Kind::Firefox },
    Known { names: &["librewolf"], snap: None, flatpaks: &["io.gitlab.librewolf-community"], kind: Kind::Firefox },
    Known { names: &["code", "code-insiders", "codium", "electron"], snap: Some("code"), flatpaks: &["com.visualstudio.code", "com.vscodium.codium"], kind: Kind::Electron },
];

/// How the browser is packaged: where its profile can be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Packaging {
    System,
    Snap(&'static str),
    Flatpak(&'static str),
}

/// A launch's browser: what it is, how it is packaged, and the index in
/// argv after which its flags go (the browser's own name).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Browser {
    pub kind: Kind,
    /// The table's first name for it: the system profile's directory name.
    pub name: &'static str,
    pub packaging: Packaging,
    pub at: usize,
}

/// What the system says about argv[0]: where `PATH` finds it, what that
/// file is when every link is followed, whether it is a script that runs
/// `/snap/bin/…` (Ubuntu's `/usr/bin/firefox`), and whether snap `name` is
/// installed (`/snap/bin/<name>`).
pub struct Found<'a> {
    pub path: Option<&'a Path>,
    pub canonical: Option<&'a Path>,
    pub runs_a_snap: bool,
    pub snap_installed: &'a dyn Fn(&str) -> bool,
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The first argument after `from` that is not an option: `snap run`'s and
/// `flatpak run`'s app.
fn app_after(argv: &[String], from: usize) -> Option<usize> {
    (from..argv.len()).find(|&i| !argv[i].starts_with('-'))
}

/// The browser `argv` starts, if the table knows it.
pub fn browser_of(argv: &[String], found: &Found) -> Option<Browser> {
    let base = file_name(argv.first()?);
    let by_name = |name: &str| KNOWN.iter().find(|known| known.names.contains(&name) || known.snap == Some(name));
    // `snap run <app>` and `flatpak run [options] <id>`.
    if base == "snap" && argv.get(1).map(String::as_str) == Some("run") {
        let at = app_after(argv, 2)?;
        let snap = argv[at].split('.').next()?;
        let known = KNOWN.iter().find(|known| known.snap == Some(snap))?;
        return Some(Browser { kind: known.kind, name: known.names[0], packaging: Packaging::Snap(known.snap?), at });
    }
    if base == "flatpak" && argv.get(1).map(String::as_str) == Some("run") {
        let at = app_after(argv, 2)?;
        let (known, id) = KNOWN.iter().find_map(|known| known.flatpaks.iter().find(|id| **id == argv[at]).map(|id| (known, *id)))?;
        return Some(Browser { kind: known.kind, name: known.names[0], packaging: Packaging::Flatpak(id), at });
    }
    let path = found.path.map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| argv[0].clone());
    // A Flatpak's exported launcher: `…/flatpak/exports/bin/<id>`.
    if path.contains("/flatpak/exports/bin/") {
        let (known, id) = KNOWN.iter().find_map(|known| known.flatpaks.iter().find(|id| **id == base).map(|id| (known, *id)))?;
        return Some(Browser { kind: known.kind, name: known.names[0], packaging: Packaging::Flatpak(id), at: 0 });
    }
    // A snap's command: `/snap/bin/<name>` (a link to `/usr/bin/snap`).
    let snap_command = path.starts_with("/snap/bin/") || found.canonical.is_some_and(|c| c.file_name().is_some_and(|f| f == "snap"));
    if snap_command {
        let known = by_name(base.split('.').next()?)?;
        return Some(Browser { kind: known.kind, name: known.names[0], packaging: Packaging::Snap(known.snap?), at: 0 });
    }
    let canonical = found.canonical.and_then(|c| c.file_name()).and_then(|f| f.to_str());
    let known = KNOWN.iter().find(|known| known.names.contains(&base) || canonical.is_some_and(|c| known.names.contains(&c)))?;
    // Ubuntu's `/usr/bin/firefox` and `/usr/bin/chromium-browser`: scripts
    // that run the snap.
    let packaging = match known.snap {
        Some(snap) if found.runs_a_snap && (found.snap_installed)(snap) => Packaging::Snap(snap),
        _ => Packaging::System,
    };
    Some(Browser { kind: known.kind, name: known.names[0], packaging, at: 0 })
}

/// Where `browser`'s profile in space `space` of compositor `compositor`
/// is: inside a snap's or a Flatpak's own data directory under the
/// person's `home`, in the space's `space_home` otherwise; `None` for
/// Electron (no profile flag).
pub fn profile_dir(browser: &Browser, home: &Path, space_home: &Path, compositor: u32, space: &str) -> Option<PathBuf> {
    let leaf = format!("p{compositor}-{space}");
    match (browser.kind, &browser.packaging) {
        (Kind::Electron, _) => None,
        (_, Packaging::System) => Some(space_home.join("browsers").join(browser.name)),
        (_, Packaging::Snap(snap)) => Some(home.join("snap").join(snap).join("common/exponential-space").join(leaf)),
        (_, Packaging::Flatpak(id)) => Some(home.join(".var/app").join(id).join("exponential-space").join(leaf)),
    }
}

/// A profile flag of the caller's in `args`, which the space gives instead.
fn callers_profile(kind: Kind, args: &[String]) -> Option<&str> {
    args.iter().map(String::as_str).find(|arg| match kind {
        Kind::Chromium => arg.starts_with("--user-data-dir"),
        Kind::Firefox => ["--profile", "-profile", "-P", "--P", "-ProfileManager", "--ProfileManager"].contains(arg) || arg.starts_with("--profile="),
        Kind::Electron => false,
    })
}

/// `argv` with `browser`'s profile in `dir` and its switches (`software`:
/// the space renders in software); `Err` names a profile flag the caller
/// gave.
pub fn with_profile(argv: &[String], browser: &Browser, dir: Option<&Path>, software: bool) -> Result<Vec<String>, String> {
    let after = &argv[browser.at + 1..];
    if let Some(flag) = callers_profile(browser.kind, after) {
        return Err(flag.to_string());
    }
    let mut flags: Vec<String> = Vec::new();
    match (browser.kind, dir) {
        (Kind::Chromium, Some(dir)) => flags.push(format!("--user-data-dir={}", dir.display())),
        (Kind::Firefox, Some(dir)) => flags.extend(["--profile".to_string(), dir.display().to_string(), "--new-instance".to_string()]),
        _ => {}
    }
    if matches!(browser.kind, Kind::Chromium | Kind::Electron) {
        flags.push("--enable-unsafe-swiftshader".into());
        if software {
            flags.push("--use-angle=swiftshader".into());
        }
    }
    if browser.kind == Kind::Chromium && matches!(browser.packaging, Packaging::Snap(_)) {
        flags.push("--ozone-platform=x11".into());
    }
    let mut out = argv[..=browser.at].to_vec();
    out.extend(flags);
    out.extend(after.iter().cloned());
    Ok(out)
}

/// What [`Found`] needs, from this system: `PATH`, the links, the script.
pub fn look_up(program: &str) -> (Option<PathBuf>, Option<PathBuf>, bool) {
    let path = if program.contains('/') {
        Some(PathBuf::from(program))
    } else {
        std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|dir| dir.join(program)).find(|p| p.is_file()))
    };
    let canonical = path.as_deref().and_then(|p| std::fs::canonicalize(p).ok());
    // A script that runs a snap: `#!` and `/snap/bin/` in its first 4 KiB.
    let runs_a_snap = canonical.as_deref().and_then(|p| {
        use std::io::Read;
        let mut head = [0u8; 4096];
        let n = std::fs::File::open(p).ok()?.read(&mut head).ok()?;
        let head = &head[..n];
        Some(head.starts_with(b"#!") && head.windows(10).any(|w| w == b"/snap/bin/"))
    });
    (path, canonical, runs_a_snap.unwrap_or(false))
}

/// The profiles under `parent` (an app's `exponential-space`) of
/// compositors that no longer run: what a crashed session left.
pub fn stale_siblings(parent: &Path, alive: impl Fn(u32) -> bool) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(parent) else { return Vec::new() };
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let pid = name.to_str().and_then(|n| n.strip_prefix('p')).and_then(|n| n.split('-').next()).and_then(|n| n.parse::<u32>().ok());
            pid.is_some_and(|pid| !alive(pid))
        })
        .map(|entry| entry.path())
        .collect()
}

impl Ede {
    /// `argv` for space `desk`: a known browser's with its profile (made
    /// now, readable by the user alone, removed at the close) and its
    /// switches; any other program's as it is.
    pub(crate) fn space_browser(&mut self, desk: DeskId, argv: Vec<String>) -> Result<Vec<String>, Failure> {
        let Some(program) = argv.first() else { return Ok(argv) };
        let (path, canonical, runs_a_snap) = look_up(program);
        let installed = |snap: &str| Path::new("/snap/bin").join(snap).exists();
        let found = Found { path: path.as_deref(), canonical: canonical.as_deref(), runs_a_snap, snap_installed: &installed };
        let Some(browser) = browser_of(&argv, &found) else { return Ok(argv) };
        let space = self.spaces.get(desk).ok_or_else(|| Failure::new(ErrorCode::NotFound, "no such space"))?;
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into());
        let dir = profile_dir(&browser, &home, &self.spaces.home_of(space), std::process::id(), space.id.as_str());
        let argv = with_profile(&argv, &browser, dir.as_deref(), space.software_rendering).map_err(|flag| Failure::new(ErrorCode::Invalid, &format!("{flag}: in a space {} has the space's own profile", browser.name)))?;
        let Some(dir) = dir else { return Ok(argv) };
        if browser.packaging != Packaging::System {
            // What a crashed session left in the app's data.
            if let Some(parent) = dir.parent() {
                for stale in stale_siblings(parent, |pid| Path::new(&format!("/proc/{pid}")).exists()) {
                    let _ = super::close::remove_tree(&stale);
                }
            }
        }
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).map_err(|e| Failure::new(ErrorCode::Unavailable, &format!("{}'s profile: {e}", browser.name)))?;
        if browser.packaging != Packaging::System {
            if let Some(space) = self.spaces.get_mut(desk).filter(|space| !space.browser_dirs.contains(&dir)) {
                space.browser_dirs.push(dir);
            }
        }
        Ok(argv)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    fn found<'a>(path: Option<&'a Path>, canonical: Option<&'a Path>, runs_a_snap: bool, snaps: &'a dyn Fn(&str) -> bool) -> Found<'a> {
        Found { path, canonical, runs_a_snap, snap_installed: snaps }
    }

    /// The table per packaging: the system's, a snap's (its command, `snap
    /// run`, Ubuntu's wrapper script), a Flatpak's (`flatpak run`, its
    /// exported launcher); a program it does not know is left alone.
    #[test]
    fn a_browser_is_known_by_how_it_was_started() {
        let none = |_: &str| false;
        let chromium_snap = |snap: &str| snap == "chromium" || snap == "firefox";
        let system = browser_of(&argv(&["chromium", "https://example.org"]), &found(Some(Path::new("/usr/bin/chromium")), Some(Path::new("/usr/lib/chromium/chromium")), false, &none)).unwrap();
        assert_eq!((system.kind, system.name, &system.packaging, system.at), (Kind::Chromium, "chromium", &Packaging::System, 0));
        let link = browser_of(&argv(&["/snap/bin/chromium"]), &found(Some(Path::new("/snap/bin/chromium")), Some(Path::new("/usr/bin/snap")), false, &none)).unwrap();
        assert_eq!(link.packaging, Packaging::Snap("chromium"));
        let run = browser_of(&argv(&["snap", "run", "firefox", "-private"]), &found(None, None, false, &none)).unwrap();
        assert_eq!((run.kind, &run.packaging, run.at), (Kind::Firefox, &Packaging::Snap("firefox"), 2));
        let wrapper = browser_of(&argv(&["firefox"]), &found(Some(Path::new("/usr/bin/firefox")), Some(Path::new("/usr/bin/firefox")), true, &chromium_snap)).unwrap();
        assert_eq!(wrapper.packaging, Packaging::Snap("firefox"), "Ubuntu's /usr/bin/firefox runs the snap");
        let not_wrapped = browser_of(&argv(&["firefox"]), &found(Some(Path::new("/usr/bin/firefox")), Some(Path::new("/usr/lib/firefox/firefox")), false, &chromium_snap)).unwrap();
        assert_eq!(not_wrapped.packaging, Packaging::System);
        let flatpak = browser_of(&argv(&["flatpak", "run", "--branch=stable", "org.mozilla.firefox", "about:blank"]), &found(None, None, false, &none)).unwrap();
        assert_eq!((&flatpak.packaging, flatpak.at), (&Packaging::Flatpak("org.mozilla.firefox"), 3));
        let exported = browser_of(&argv(&["com.google.Chrome"]), &found(Some(Path::new("/var/lib/flatpak/exports/bin/com.google.Chrome")), None, false, &none)).unwrap();
        assert_eq!((exported.kind, &exported.packaging), (Kind::Chromium, &Packaging::Flatpak("com.google.Chrome")));
        let alternative = browser_of(&argv(&["x-www-browser"]), &found(Some(Path::new("/usr/bin/x-www-browser")), Some(Path::new("/usr/lib/chromium/chromium")), false, &none)).unwrap();
        assert_eq!(alternative.name, "chromium", "found by what the link resolves to");
        for unknown in [argv(&["xterm"]), argv(&["snap", "run", "spotify"]), argv(&["flatpak", "run", "org.gnome.Epiphany"])] {
            assert_eq!(browser_of(&unknown, &found(None, None, false, &none)), None, "{unknown:?}");
        }
        assert_eq!(browser_of(&argv(&["code", "."]), &found(None, None, false, &none)).unwrap().kind, Kind::Electron);
    }

    #[test]
    fn the_profile_is_the_spaces_and_inside_a_sandboxed_apps_own_data() {
        let home = Path::new("/home/a");
        let space_home = Path::new("/home/a/.cache/exponential-desktop/spaces/p9/space-1");
        let browser = |kind, packaging| Browser { kind, name: "chromium", packaging, at: 0 };
        assert_eq!(profile_dir(&browser(Kind::Chromium, Packaging::System), home, space_home, 9, "space-1").unwrap(), space_home.join("browsers/chromium"));
        assert_eq!(profile_dir(&browser(Kind::Firefox, Packaging::Snap("firefox")), home, space_home, 9, "space-1").unwrap(), Path::new("/home/a/snap/firefox/common/exponential-space/p9-space-1"));
        assert_eq!(profile_dir(&browser(Kind::Chromium, Packaging::Flatpak("org.chromium.Chromium")), home, space_home, 9, "space-1").unwrap(), Path::new("/home/a/.var/app/org.chromium.Chromium/exponential-space/p9-space-1"));
        assert_eq!(profile_dir(&browser(Kind::Electron, Packaging::System), home, space_home, 9, "space-1"), None);
    }

    #[test]
    fn the_flags_go_after_the_browser_and_a_callers_profile_is_refused() {
        let dir = Path::new("/p");
        let chromium = Browser { kind: Kind::Chromium, name: "chromium", packaging: Packaging::System, at: 0 };
        assert_eq!(with_profile(&argv(&["chromium", "https://x"]), &chromium, Some(dir), true).unwrap(), argv(&["chromium", "--user-data-dir=/p", "--enable-unsafe-swiftshader", "--use-angle=swiftshader", "https://x"]));
        assert_eq!(with_profile(&argv(&["chromium"]), &chromium, Some(dir), false).unwrap(), argv(&["chromium", "--user-data-dir=/p", "--enable-unsafe-swiftshader"]), "a space with a GPU path: ANGLE may still fall back");
        assert_eq!(with_profile(&argv(&["chromium", "--user-data-dir=/mine"]), &chromium, Some(dir), true), Err("--user-data-dir=/mine".into()));
        let snap = Browser { packaging: Packaging::Snap("chromium"), ..chromium.clone() };
        assert_eq!(with_profile(&argv(&["chromium", "https://x"]), &snap, Some(dir), true).unwrap(), argv(&["chromium", "--user-data-dir=/p", "--enable-unsafe-swiftshader", "--use-angle=swiftshader", "--ozone-platform=x11", "https://x"]), "the snap's AppArmor refuses the space's Wayland socket");
        let flatpak = Browser { packaging: Packaging::Flatpak("org.chromium.Chromium"), at: 2, ..chromium.clone() };
        assert_eq!(with_profile(&argv(&["flatpak", "run", "org.chromium.Chromium"]), &flatpak, Some(dir), false).unwrap(), argv(&["flatpak", "run", "org.chromium.Chromium", "--user-data-dir=/p", "--enable-unsafe-swiftshader"]));
        let firefox = Browser { kind: Kind::Firefox, name: "firefox", packaging: Packaging::Snap("firefox"), at: 2 };
        assert_eq!(with_profile(&argv(&["snap", "run", "firefox", "about:blank"]), &firefox, Some(dir), true).unwrap(), argv(&["snap", "run", "firefox", "--profile", "/p", "--new-instance", "about:blank"]), "GTK falls back to X11 by itself");
        for flag in ["-P", "--profile", "-profile", "--profile=/x", "--ProfileManager"] {
            assert!(with_profile(&argv(&["firefox", flag]), &Browser { at: 0, ..firefox.clone() }, Some(dir), true).is_err(), "{flag}");
        }
        let code = Browser { kind: Kind::Electron, name: "code", packaging: Packaging::System, at: 0 };
        assert_eq!(with_profile(&argv(&["code", "."]), &code, None, true).unwrap(), argv(&["code", "--enable-unsafe-swiftshader", "--use-angle=swiftshader", "."]));
    }

    #[test]
    fn a_crashed_compositors_profiles_are_stale() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["p10-space-1", "p11-space-2", "p11-space-3", "other"] {
            std::fs::create_dir(dir.path().join(name)).unwrap();
        }
        let mut stale = stale_siblings(dir.path(), |pid| pid == 11);
        stale.sort();
        assert_eq!(stale, [dir.path().join("p10-space-1")]);
    }
}
