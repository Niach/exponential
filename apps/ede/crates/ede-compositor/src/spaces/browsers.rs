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
//! space closes.
//!
//! A launch that names a profile itself keeps it (Playwright always gives
//! its own, in a temporary directory), unless that is, or is inside, one of
//! the person's own browser profiles ([`Persons`]): followed through every
//! link and `..`, from the launch's directory. That launch is refused, and
//! so is a flag that picks among the person's profiles (Firefox's `-P`),
//! names no directory (the browser's default), or steps out of the profile
//! (a Chromium `--profile-directory=` that is a path). The space's profile
//! is not made for a caller's own (#EDE-186).
//!
//! While the space renders in software ([`super::render`]), Chromium and
//! Electron also draw WebGL with SwiftShader (`--use-angle=swiftshader`): on
//! a GPU path that cannot present, their WebGL stays blank rather than
//! failing in a way they notice. `--enable-unsafe-swiftshader` is always
//! there, so ANGLE may fall back to SwiftShader when its GPU path fails.
//! That is every launch of the Chromium family, its own profile or the
//! space's: also its binaries started by name (`chrome`: Chrome's own, Chrome
//! for Testing's and Playwright's; `chrome-headless-shell`, `headless_shell`,
//! `msedge`, `vivaldi-bin`; #EDE-186). Firefox is given nothing: on llvmpipe
//! it switches to software WebRender by itself.
//!
//! A snap of the Chromium family also gets `--ozone-platform=x11`: the
//! snap's AppArmor admits no Wayland socket but `/run/user/<uid>/wayland-N`,
//! so the space's is refused, and Chromium then exits where GTK (Firefox)
//! falls back to the space's Xwayland by itself (#EDE-180).
//!
//! GNOME Web (Epiphany) gets `--profile=<dir>`: with it, it runs as an
//! instance of its own, never joining the person's.
//!
//! A browser this table does not know runs as any other program when it is
//! the system's: its data goes to the space's `XDG_*` directories. A snap or
//! a Flatpak the table does not know is refused when its `.desktop` file
//! says it is a browser (`Categories=WebBrowser`, or it opens `http`/`https`
//! links): it keeps its data under the person's `HOME`, so it would run in
//! the person's signed-in profile or hand its window to the person's own
//! instance (#EDE-166).

use std::{
    ffi::OsString,
    os::unix::fs::DirBuilderExt,
    path::{Component, Path, PathBuf},
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
    /// GNOME Web: `--profile=<dir>`, which also makes it an instance of its
    /// own (Epiphany's "standalone" mode is never the unique one).
    Epiphany,
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
    // #EDE-186: the binary itself, `/opt/google/chrome/chrome`, Chrome for
    // Testing's and Playwright's (`~/.cache/ms-playwright/chromium-*/chrome-linux64/chrome`),
    // and the headless shells.
    Known { names: &["chrome", "chrome-headless-shell", "headless_shell"], snap: None, flatpaks: &[], kind: Kind::Chromium },
    Known { names: &["brave-browser", "brave-browser-stable", "brave"], snap: Some("brave"), flatpaks: &["com.brave.Browser"], kind: Kind::Chromium },
    Known { names: &["microsoft-edge", "microsoft-edge-stable", "msedge"], snap: None, flatpaks: &["com.microsoft.Edge"], kind: Kind::Chromium },
    Known { names: &["vivaldi", "vivaldi-stable", "vivaldi-bin"], snap: None, flatpaks: &["com.vivaldi.Vivaldi"], kind: Kind::Chromium },
    Known { names: &["firefox", "firefox-esr"], snap: Some("firefox"), flatpaks: &["org.mozilla.firefox"], kind: Kind::Firefox },
    Known { names: &["librewolf"], snap: None, flatpaks: &["io.gitlab.librewolf-community"], kind: Kind::Firefox },
    // #EDE-166: Firefox's and Chromium's forks take their engine's flags.
    Known { names: &["zen", "zen-browser"], snap: None, flatpaks: &["app.zen_browser.zen"], kind: Kind::Firefox },
    Known { names: &["floorp"], snap: None, flatpaks: &["one.ablaze.floorp"], kind: Kind::Firefox },
    Known { names: &["waterfox"], snap: None, flatpaks: &["net.waterfox.waterfox"], kind: Kind::Firefox },
    Known { names: &["opera", "opera-beta", "opera-developer"], snap: Some("opera"), flatpaks: &["com.opera.Opera"], kind: Kind::Chromium },
    Known { names: &["epiphany", "epiphany-browser"], snap: None, flatpaks: &["org.gnome.Epiphany"], kind: Kind::Epiphany },
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
/// file is when every link is followed, the snap it runs when it is a
/// script that runs `/snap/bin/<name>` (Ubuntu's `/usr/bin/firefox`), and
/// whether snap `name` is installed (`/snap/bin/<name>`).
pub struct Found<'a> {
    pub path: Option<&'a Path>,
    pub canonical: Option<&'a Path>,
    pub runs_a_snap: Option<&'a str>,
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
        Some(snap) if found.runs_a_snap.is_some() && (found.snap_installed)(snap) => Packaging::Snap(snap),
        _ => Packaging::System,
    };
    Some(Browser { kind: known.kind, name: known.names[0], packaging, at: 0 })
}

/// A launch's snap or Flatpak, whatever it is: its snap's name or its
/// Flatpak's id, the same ways [`browser_of`] finds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Sandboxed {
    Snap(String),
    Flatpak(String),
}

/// The snap or Flatpak `argv` starts, if it starts one.
pub fn sandboxed_of(argv: &[String], found: &Found) -> Option<Sandboxed> {
    let base = file_name(argv.first()?);
    if (base == "snap" || base == "flatpak") && argv.get(1).map(String::as_str) == Some("run") {
        let app = &argv[app_after(argv, 2)?];
        return Some(if base == "snap" { Sandboxed::Snap(app.split('.').next()?.to_string()) } else { Sandboxed::Flatpak(app.clone()) });
    }
    let path = found.path.map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| argv[0].clone());
    if path.contains("/flatpak/exports/bin/") {
        return Some(Sandboxed::Flatpak(base.to_string()));
    }
    if path.starts_with("/snap/bin/") || found.canonical.is_some_and(|c| c.file_name().is_some_and(|f| f == "snap")) {
        return Some(Sandboxed::Snap(base.split('.').next()?.to_string()));
    }
    let snap = found.runs_a_snap.filter(|snap| (found.snap_installed)(snap))?;
    Some(Sandboxed::Snap(snap.to_string()))
}

/// Whether a `.desktop` file's `[Desktop Entry]` says its app is a web
/// browser: `WebBrowser` in its `Categories`, or `x-scheme-handler/http`
/// or `https` in its `MimeType`.
pub fn desktop_is_browser(entry: &str) -> bool {
    let mut in_entry = false;
    for line in entry.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| in_entry) else { continue };
        let mut values = value.split(';').map(str::trim);
        match key.trim() {
            "Categories" if values.any(|c| c == "WebBrowser") => return true,
            "MimeType" if values.any(|m| m == "x-scheme-handler/http" || m == "x-scheme-handler/https") => return true,
            _ => {}
        }
    }
    false
}

/// The `.desktop` files of `app` in the `applications` directories `dirs`:
/// a snap's are `<snap>_<app>.desktop`, a Flatpak's `<id>.desktop` and
/// `<id>.<more>.desktop`.
pub fn desktop_files(app: &Sandboxed, dirs: &[PathBuf]) -> Vec<PathBuf> {
    let ours = |name: &str| {
        let Some(stem) = name.strip_suffix(".desktop") else { return false };
        match app {
            Sandboxed::Snap(snap) => stem.strip_prefix(snap.as_str()).is_some_and(|rest| rest.starts_with('_')),
            Sandboxed::Flatpak(id) => stem == id || stem.strip_prefix(id.as_str()).is_some_and(|rest| rest.starts_with('.')),
        }
    };
    dirs.iter().flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten()).filter(|entry| entry.file_name().to_str().is_some_and(ours)).map(|entry| entry.path()).collect()
}

/// Where this system's `.desktop` files are: the person's data directory,
/// `XDG_DATA_DIRS`, and the Flatpak and snap exports also when
/// `XDG_DATA_DIRS` does not name them.
fn applications_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into());
    let data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|dir| dir.is_absolute()).unwrap_or_else(|| home.join(".local/share"));
    let data_dirs = std::env::var("XDG_DATA_DIRS").ok().filter(|dirs| !dirs.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let mut dirs: Vec<PathBuf> = [data_home.clone(), data_home.join("flatpak/exports/share")].into_iter().chain(data_dirs.split(':').filter(|dir| dir.starts_with('/')).map(PathBuf::from)).chain(["/var/lib/flatpak/exports/share".into(), "/var/lib/snapd/desktop".into()]).map(|dir| dir.join("applications")).collect();
    dirs.dedup();
    dirs
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

/// The person's, for the profile a caller names: their home, the
/// directories their browsers keep profiles in under it, and where a
/// relative path starts (the launch's working directory).
pub struct Persons<'a> {
    pub home: &'a Path,
    /// `~/.config`, and `$XDG_CONFIG_HOME` where that is elsewhere.
    pub configs: &'a [PathBuf],
    /// `~/.local/share`, and `$XDG_DATA_HOME` where that is elsewhere.
    pub datas: &'a [PathBuf],
    pub cwd: &'a Path,
}

/// Where the person's browsers keep their profiles: the entries of
/// `parent` that `named` admits, but for `except` inside one (where a
/// snap's or a Flatpak's data holds the spaces' own).
struct Place {
    parent: PathBuf,
    named: fn(&str) -> bool,
    except: Option<&'static str>,
}

/// Under `~/.config` (or `$XDG_CONFIG_HOME`): Chromium's, Chrome's
/// (`google-chrome*`: also Chrome for Testing's `google-chrome-for-testing`),
/// Brave's, Edge's, Vivaldi's, Opera's, and Firefox's, LibreWolf's and
/// Zen's where they keep theirs there (XDG).
fn config_profile(name: &str) -> bool {
    ["chromium", "BraveSoftware", "mozilla", "librewolf", "zen"].contains(&name) || ["google-chrome", "microsoft-edge", "vivaldi", "opera"].iter().any(|prefix| name.starts_with(prefix))
}

/// Under `~/.local/share` (or `$XDG_DATA_HOME`): GNOME Web's.
fn data_profile(name: &str) -> bool {
    name == "epiphany"
}

impl Place {
    /// Whether `target` (resolved) is in this place: under an entry by its
    /// name, there or not yet (a browser makes it), or under where an entry
    /// that is a link leads.
    fn holds(&self, target: &Path) -> bool {
        let Some(parent) = resolved(&self.parent) else { return false };
        let named = target.strip_prefix(&parent).ok().and_then(|rest| rest.components().next()).and_then(|first| first.as_os_str().to_str()).filter(|name| (self.named)(name)).map(|name| parent.join(name));
        let linked = std::fs::read_dir(&self.parent).into_iter().flatten().flatten().filter(|entry| entry.file_name().to_str().is_some_and(self.named)).filter_map(|entry| resolved(&entry.path()));
        named.into_iter().chain(linked).any(|root| target.starts_with(&root) && !self.except.is_some_and(|except| target.starts_with(root.join(except))))
    }
}

impl Persons<'_> {
    fn places(&self) -> Vec<Place> {
        let mut places = vec![
            Place { parent: self.home.to_path_buf(), named: |name| [".mozilla", ".librewolf", ".zen", ".floorp", ".waterfox"].contains(&name), except: None },
            Place { parent: self.home.join("snap"), named: |name| KNOWN.iter().any(|known| known.snap == Some(name)), except: Some("common/exponential-space") },
            Place { parent: self.home.join(".var/app"), named: |name| KNOWN.iter().any(|known| known.flatpaks.contains(&name)), except: Some("exponential-space") },
        ];
        places.extend(self.configs.iter().map(|parent| Place { parent: parent.clone(), named: config_profile, except: None }));
        places.extend(self.datas.iter().map(|parent| Place { parent: parent.clone(), named: data_profile, except: None }));
        places
    }

    /// Whether `dir`, as a caller gave it (`~` the home, a relative path
    /// from the launch's directory), is or is inside one of the person's
    /// browser profiles: those of [`config_profile`] in each of `configs`,
    /// of [`data_profile`] in each of `datas`, `~/.mozilla`, `~/.librewolf`,
    /// `~/.zen`, `~/.floorp`, `~/.waterfox`, a known browser's snap
    /// (`~/snap/<name>`) or Flatpak (`~/.var/app/<id>`) data but for its
    /// `exponential-space`. A loop of links counts too: it leads nowhere a
    /// browser could start in.
    pub fn profile(&self, dir: &str) -> bool {
        let path = match dir.strip_prefix('~') {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => self.home.join(rest.trim_start_matches('/')),
            _ => self.cwd.join(dir),
        };
        let Some(target) = resolved(&path) else { return true };
        self.places().iter().any(|place| place.holds(&target))
    }
}

/// `path` as the kernel finds it: every link followed, every `..` taken
/// where it stands; what is not there yet as written (a browser makes it).
/// A relative one from the compositor's directory. `None`: a loop of links.
pub fn resolved(path: &Path) -> Option<PathBuf> {
    /// `path`'s names and `..`s, the last first.
    fn parts(path: &Path) -> Vec<OsString> {
        path.components()
            .rev()
            .filter_map(|part| match part {
                Component::Normal(name) => Some(name.to_owned()),
                Component::ParentDir => Some("..".into()),
                _ => None,
            })
            .collect()
    }
    let path = if path.is_absolute() { path.to_path_buf() } else { std::env::current_dir().ok()?.join(path) };
    let mut rest = parts(&path);
    let mut out = PathBuf::from("/");
    let mut links = 0;
    while let Some(part) = rest.pop() {
        if part == ".." {
            out.pop();
            continue;
        }
        let next = out.join(&part);
        match std::fs::read_link(&next) {
            Ok(target) => {
                // Linux's MAXSYMLINKS.
                links += 1;
                if links > 40 {
                    return None;
                }
                if target.is_absolute() {
                    out = PathBuf::from("/");
                }
                rest.extend(parts(&target));
            }
            Err(_) => out = next,
        }
    }
    Some(out)
}

/// Whether the caller gave `kind` a profile of its own in `args` (what
/// follows the browser's name), which it then keeps: `Err` says why one is
/// refused. `persons` says whether a directory is the person's
/// ([`Persons::profile`]).
///
/// Chromium reads `--user-data-dir=` and `-user-data-dir=`, the last one
/// winning, and no switch after `--`; `--profile-directory=` is a name in
/// it, never a path. Firefox reads `-profile <dir>` with
/// one or two dashes in any case; `-P <name>`, `-ProfileManager` and
/// `-CreateProfile` go to the person's `profiles.ini`. GNOME Web reads
/// `--profile <dir>` and `--profile=<dir>`; `--private-instance` (`-p`) and
/// `--automation-mode` bring a temporary profile of their own.
pub fn callers_profile(kind: Kind, args: &[String], persons: &dyn Fn(&str) -> bool) -> Result<bool, String> {
    let mut own = false;
    match kind {
        Kind::Chromium => {
            for arg in args.iter().take_while(|arg| *arg != "--") {
                let Some(switch) = arg.strip_prefix("--").or_else(|| arg.strip_prefix('-')) else { continue };
                let (name, value) = switch.split_once('=').map_or((switch, None), |(name, value)| (name, Some(value)));
                match (name, value) {
                    ("user-data-dir", None | Some("")) => return Err(format!("{arg} names no directory: the browser would take its default, the person's profile")),
                    ("user-data-dir", Some(dir)) if persons(dir) => return Err(format!("{arg} is the person's own browser profile: a browser in a space never uses it")),
                    ("user-data-dir", Some(_)) => own = true,
                    // Inside the user data directory: a path could step out of it.
                    ("profile-directory", Some(dir)) if dir.contains('/') || dir == ".." => return Err(format!("{arg}: a profile directory is a name inside the user data directory")),
                    _ => {}
                }
            }
        }
        Kind::Firefox => {
            for (at, arg) in args.iter().enumerate() {
                let Some(flag) = arg.strip_prefix("--").or_else(|| arg.strip_prefix('-')) else { continue };
                let flag = flag.to_ascii_lowercase();
                match flag.split_once('=').map_or(flag.as_str(), |(name, _)| name) {
                    "profile" if flag.contains('=') => return Err(format!("{arg}: in a space a browser's own profile is given as --profile <dir>")),
                    "profile" => match args.get(at + 1) {
                        Some(dir) if !dir.starts_with('-') && persons(dir) => return Err(format!("{arg} {dir} is the person's own browser profile: a browser in a space never uses it")),
                        Some(dir) if !dir.starts_with('-') => own = true,
                        _ => return Err(format!("{arg} names no directory")),
                    },
                    "p" | "profilemanager" | "createprofile" => return Err(format!("{arg} picks among the person's own profiles: in a space a browser's own profile is given as --profile <dir>")),
                    _ => {}
                }
            }
        }
        Kind::Epiphany => {
            for (at, arg) in args.iter().enumerate().take_while(|(_, arg)| *arg != "--") {
                match arg.split_once('=').map_or((arg.as_str(), None), |(name, value)| (name, Some(value))) {
                    ("--profile", Some(dir)) if dir.is_empty() => return Err(format!("{arg} names no directory")),
                    ("--profile", Some(dir)) if persons(dir) => return Err(format!("{arg} is the person's own browser profile: a browser in a space never uses it")),
                    ("--profile", Some(_)) => own = true,
                    ("--profile", None) => match args.get(at + 1) {
                        Some(dir) if !dir.starts_with('-') && persons(dir) => return Err(format!("{arg} {dir} is the person's own browser profile: a browser in a space never uses it")),
                        Some(dir) if !dir.starts_with('-') => own = true,
                        _ => return Err(format!("{arg} names no directory")),
                    },
                    ("--private-instance" | "-p" | "--automation-mode", None) => own = true,
                    _ => {}
                }
            }
        }
        Kind::Electron => {}
    }
    Ok(own)
}

/// `argv` with `browser`'s switches (`software`: the space renders in
/// software) and the space's profile in `dir` (`None`: the caller's own, or
/// Electron's none).
pub fn with_profile(argv: &[String], browser: &Browser, dir: Option<&Path>, software: bool) -> Vec<String> {
    let after = &argv[browser.at + 1..];
    let mut flags: Vec<String> = Vec::new();
    match (browser.kind, dir) {
        (Kind::Chromium, Some(dir)) => flags.push(format!("--user-data-dir={}", dir.display())),
        (Kind::Firefox, Some(dir)) => flags.extend(["--profile".to_string(), dir.display().to_string(), "--new-instance".to_string()]),
        (Kind::Epiphany, Some(dir)) => flags.push(format!("--profile={}", dir.display())),
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
    out
}

/// What [`Found`] needs, from this system: `PATH`, the links, the snap a
/// script runs.
pub fn look_up(program: &str) -> (Option<PathBuf>, Option<PathBuf>, Option<String>) {
    let path = if program.contains('/') {
        Some(PathBuf::from(program))
    } else {
        std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|dir| dir.join(program)).find(|p| p.is_file()))
    };
    let canonical = path.as_deref().and_then(|p| std::fs::canonicalize(p).ok());
    // A script that runs a snap: `#!` and `/snap/bin/<name>` in its first 4 KiB.
    let runs_a_snap = canonical.as_deref().and_then(|p| {
        use std::io::Read;
        let mut head = [0u8; 4096];
        let n = std::fs::File::open(p).ok()?.read(&mut head).ok()?;
        snap_of_script(&head[..n])
    });
    (path, canonical, runs_a_snap)
}

/// The snap a script (its first bytes `head`) runs: the name after its
/// first `/snap/bin/`.
pub fn snap_of_script(head: &[u8]) -> Option<String> {
    if !head.starts_with(b"#!") {
        return None;
    }
    let at = head.windows(10).position(|w| w == b"/snap/bin/")? + 10;
    let name: String = head[at..].iter().take_while(|b| b.is_ascii_alphanumeric() || **b == b'-' || **b == b'_' || **b == b'.').map(|b| *b as char).collect();
    Some(name.split('.').next().unwrap_or_default().to_string()).filter(|name| !name.is_empty())
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
    /// `argv` for space `desk`, started in `cwd`: a known browser's with
    /// its switches and its profile, the caller's own or the space's (made
    /// now, readable by the user alone, removed at the close); any other
    /// program's as it is.
    pub(crate) fn space_browser(&mut self, desk: DeskId, argv: Vec<String>, cwd: &Path) -> Result<Vec<String>, Failure> {
        let Some(program) = argv.first() else { return Ok(argv) };
        let (path, canonical, runs_a_snap) = look_up(program);
        let installed = |snap: &str| Path::new("/snap/bin").join(snap).exists();
        let found = Found { path: path.as_deref(), canonical: canonical.as_deref(), runs_a_snap: runs_a_snap.as_deref(), snap_installed: &installed };
        let sandboxed = sandboxed_of(&argv, &found);
        // #EDE-158: what the person installed for themselves alone is not there.
        if let Some(Sandboxed::Flatpak(id)) = &sandboxed {
            if let Some(why) = super::flatpak::persons_alone(&argv, id, &super::flatpak::installations()) {
                return Err(Failure::new(ErrorCode::NotFound, &why));
            }
        }
        let Some(browser) = browser_of(&argv, &found) else {
            // #EDE-166: a sandboxed browser the table does not know would run in
            // the person's profile. Its `.desktop` file says whether it is one:
            // in this system's places, and in an exported launcher's own
            // installation, whether the system names that or not.
            let mut dirs = applications_dirs();
            dirs.extend(path.as_deref().and_then(|path| path.parent()).filter(|bin| bin.ends_with("exports/bin")).and_then(Path::parent).map(|exports| exports.join("share/applications")));
            if let Some(app) = sandboxed.filter(|app| desktop_files(app, &dirs).iter().any(|file| std::fs::read_to_string(file).is_ok_and(|entry| desktop_is_browser(&entry)))) {
                let (what, name) = match &app {
                    Sandboxed::Snap(name) => ("snap", name),
                    Sandboxed::Flatpak(id) => ("Flatpak", id),
                };
                return Err(Failure::new(ErrorCode::Forbidden, &format!("{name} is a browser as a {what} that agent spaces do not know yet: it would run with the person's own profile, so it does not start in a space; a browser of the system or a known one does (#EDE-166)")));
            }
            return Ok(argv);
        };
        let space = self.spaces.get(desk).ok_or_else(|| Failure::new(ErrorCode::NotFound, "no such space"))?;
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into());
        let mut configs = vec![home.join(".config")];
        configs.extend(std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|dir| dir.is_absolute() && *dir != configs[0]));
        let mut datas = vec![home.join(".local/share")];
        datas.extend(std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|dir| dir.is_absolute() && *dir != datas[0]));
        let persons = Persons { home: &home, configs: &configs, datas: &datas, cwd };
        let own = callers_profile(browser.kind, &argv[browser.at + 1..], &|dir| persons.profile(dir)).map_err(|why| Failure::new(ErrorCode::Invalid, &why))?;
        let dir = if own { None } else { profile_dir(&browser, &home, &self.spaces.home_of(space), std::process::id(), space.id.as_str()) };
        let argv = with_profile(&argv, &browser, dir.as_deref(), space.software_rendering);
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
        Found { path, canonical, runs_a_snap: runs_a_snap.then_some("firefox"), snap_installed: snaps }
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
        for unknown in [argv(&["xterm"]), argv(&["snap", "run", "spotify"]), argv(&["flatpak", "run", "org.gnome.Maps"])] {
            assert_eq!(browser_of(&unknown, &found(None, None, false, &none)), None, "{unknown:?}");
        }
        assert_eq!(browser_of(&argv(&["code", "."]), &found(None, None, false, &none)).unwrap().kind, Kind::Electron);
        // #EDE-186: the Chromium family's own binaries, started by their path.
        for (path, name) in [
            ("/home/a/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome", "chrome"),
            ("/opt/google/chrome/chrome", "chrome"),
            ("/home/a/.cache/ms-playwright/chromium_headless_shell-1248/chrome-headless-shell-linux64/chrome-headless-shell", "chrome"),
            ("/home/a/.cache/ms-playwright/chromium_headless_shell-1155/chrome-linux/headless_shell", "chrome"),
            ("/opt/microsoft/msedge/msedge", "microsoft-edge"),
            ("/opt/vivaldi/vivaldi-bin", "vivaldi"),
        ] {
            let browser = browser_of(&argv(&[path, "--headless"]), &found(Some(Path::new(path)), Some(Path::new(path)), false, &none)).unwrap_or_else(|| panic!("{path} is not known"));
            assert_eq!((browser.kind, browser.name, &browser.packaging, browser.at), (Kind::Chromium, name, &Packaging::System, 0), "{path}");
        }
        // #EDE-166: the forks and GNOME Web.
        for (args, kind, packaging) in [
            (&["flatpak", "run", "org.gnome.Epiphany"][..], Kind::Epiphany, Packaging::Flatpak("org.gnome.Epiphany")),
            (&["flatpak", "run", "app.zen_browser.zen"], Kind::Firefox, Packaging::Flatpak("app.zen_browser.zen")),
            (&["snap", "run", "opera"], Kind::Chromium, Packaging::Snap("opera")),
            (&["epiphany"], Kind::Epiphany, Packaging::System),
        ] {
            let browser = browser_of(&argv(args), &found(None, None, false, &none)).unwrap_or_else(|| panic!("{args:?} is not known"));
            assert_eq!((browser.kind, browser.packaging), (kind, packaging), "{args:?}");
        }
        let linked = browser_of(&argv(&["/home/a/bin/my-chrome"]), &found(Some(Path::new("/home/a/bin/my-chrome")), Some(Path::new("/opt/google/chrome/chrome")), false, &none)).unwrap();
        assert_eq!(linked.name, "chrome", "found by what the link resolves to");
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
    fn the_flags_go_after_the_browser() {
        let dir = Path::new("/p");
        let chromium = Browser { kind: Kind::Chromium, name: "chromium", packaging: Packaging::System, at: 0 };
        assert_eq!(with_profile(&argv(&["chromium", "https://x"]), &chromium, Some(dir), true), argv(&["chromium", "--user-data-dir=/p", "--enable-unsafe-swiftshader", "--use-angle=swiftshader", "https://x"]));
        assert_eq!(with_profile(&argv(&["chromium"]), &chromium, Some(dir), false), argv(&["chromium", "--user-data-dir=/p", "--enable-unsafe-swiftshader"]), "a space with a GPU path: ANGLE may still fall back");
        let snap = Browser { packaging: Packaging::Snap("chromium"), ..chromium.clone() };
        assert_eq!(with_profile(&argv(&["chromium", "https://x"]), &snap, Some(dir), true), argv(&["chromium", "--user-data-dir=/p", "--enable-unsafe-swiftshader", "--use-angle=swiftshader", "--ozone-platform=x11", "https://x"]), "the snap's AppArmor refuses the space's Wayland socket");
        let flatpak = Browser { packaging: Packaging::Flatpak("org.chromium.Chromium"), at: 2, ..chromium.clone() };
        assert_eq!(with_profile(&argv(&["flatpak", "run", "org.chromium.Chromium"]), &flatpak, Some(dir), false), argv(&["flatpak", "run", "org.chromium.Chromium", "--user-data-dir=/p", "--enable-unsafe-swiftshader"]));
        let firefox = Browser { kind: Kind::Firefox, name: "firefox", packaging: Packaging::Snap("firefox"), at: 2 };
        assert_eq!(with_profile(&argv(&["snap", "run", "firefox", "about:blank"]), &firefox, Some(dir), true), argv(&["snap", "run", "firefox", "--profile", "/p", "--new-instance", "about:blank"]), "GTK falls back to X11 by itself");
        let code = Browser { kind: Kind::Electron, name: "code", packaging: Packaging::System, at: 0 };
        assert_eq!(with_profile(&argv(&["code", "."]), &code, None, true), argv(&["code", "--enable-unsafe-swiftshader", "--use-angle=swiftshader", "."]));
        // #EDE-186: a caller's own profile stays as it is; the switches come
        // all the same (the snap's too).
        let chrome = Browser { name: "chrome", ..chromium.clone() };
        let playwright = argv(&["chrome", "--user-data-dir=/tmp/playwright_chromiumdev_profile-x", "--remote-debugging-pipe", "about:blank"]);
        assert_eq!(with_profile(&playwright, &chrome, None, true), argv(&["chrome", "--enable-unsafe-swiftshader", "--use-angle=swiftshader", "--user-data-dir=/tmp/playwright_chromiumdev_profile-x", "--remote-debugging-pipe", "about:blank"]));
        assert_eq!(with_profile(&argv(&["chrome", "--user-data-dir=/tmp/x"]), &chrome, None, false), argv(&["chrome", "--enable-unsafe-swiftshader", "--user-data-dir=/tmp/x"]));
        assert_eq!(with_profile(&argv(&["chromium", "--user-data-dir=/tmp/x"]), &snap, None, true), argv(&["chromium", "--enable-unsafe-swiftshader", "--use-angle=swiftshader", "--ozone-platform=x11", "--user-data-dir=/tmp/x"]));
        assert_eq!(with_profile(&argv(&["firefox", "-profile", "/tmp/x"]), &Browser { at: 0, ..firefox.clone() }, None, true), argv(&["firefox", "-profile", "/tmp/x"]), "Firefox is given nothing");
    }

    /// #EDE-186: which of a caller's profile flags are its own (kept), which
    /// are refused: one that is the person's, one that picks among the
    /// person's, one that names no directory.
    #[test]
    fn a_callers_own_profile_is_kept_unless_it_is_the_persons() {
        let persons = |dir: &str| dir.starts_with("/home/a/.config/chromium") || dir.starts_with("/home/a/.mozilla");
        let profile = |kind, args: &[&str]| callers_profile(kind, &argv(args), &persons);
        assert_eq!(profile(Kind::Chromium, &["https://x"]), Ok(false), "no profile of its own: the space's");
        assert_eq!(profile(Kind::Chromium, &["--user-data-dir=/tmp/playwright_chromiumdev_profile-x", "--headless"]), Ok(true));
        assert_eq!(profile(Kind::Chromium, &["-user-data-dir=/tmp/x"]), Ok(true), "one dash starts a switch too");
        assert_eq!(profile(Kind::Chromium, &["--", "--user-data-dir=/tmp/x"]), Ok(false), "after `--` it is a page to open, not a switch");
        assert_eq!(profile(Kind::Chromium, &["--User-Data-Dir=/home/a/.config/chromium"]), Ok(false), "switches are lower case: an unknown one");
        assert_eq!(profile(Kind::Chromium, &["--user-data-dir=/tmp/x", "--profile-directory=Profile 1"]), Ok(true));
        assert_eq!(profile(Kind::Chromium, &["--profile-directory=Default"]), Ok(false), "a profile in the space's");
        for refused in [
            &["--user-data-dir=/home/a/.config/chromium"][..],
            &["-user-data-dir=/home/a/.config/chromium/Default"],
            &["--user-data-dir=/tmp/x", "--user-data-dir=/home/a/.config/chromium"],
            &["--user-data-dir"],
            &["--user-data-dir=", "about:blank"],
            &["--user-data-dir", "/tmp/x"],
            &["--user-data-dir=/tmp/x", "--profile-directory=../../home/a/.config/chromium/Default"],
            &["--profile-directory=/home/a/.config/chromium/Default"],
            &["--profile-directory=.."],
        ] {
            assert!(profile(Kind::Chromium, refused).is_err(), "{refused:?}");
        }
        assert_eq!(profile(Kind::Firefox, &["-no-remote", "-profile", "/tmp/playwright_firefoxdev_profile-x", "-juggler-pipe"]), Ok(true));
        assert_eq!(profile(Kind::Firefox, &["--profile", "/tmp/x", "-private"]), Ok(true));
        assert_eq!(profile(Kind::Firefox, &["-private-window", "about:blank"]), Ok(false));
        for refused in [
            &["-profile", "/home/a/.mozilla/firefox/x.default"][..],
            &["-PROFILE", "/home/a/.mozilla/firefox/x.default"],
            &["--profile", "/tmp/x", "--profile", "/home/a/.mozilla/firefox/x.default"],
            &["--profile"],
            &["--profile", "-private"],
            &["--profile=/tmp/x"],
            &["-P"],
            &["-P", "default"],
            &["-p", "default"],
            &["--P", "default"],
            &["--ProfileManager"],
            &["-profilemanager"],
            &["-CreateProfile", "x /tmp/x"],
        ] {
            assert!(profile(Kind::Firefox, refused).is_err(), "{refused:?}");
        }
        assert_eq!(profile(Kind::Electron, &["--user-data-dir=/home/a/.config/chromium"]), Ok(false), "Electron has no profile flag of the space's");
    }

    /// #EDE-186: the person's own profiles, however a path leads there: by
    /// a link, by `..`, from a relative path, by `~`; and the places beside
    /// them that are not the person's.
    #[test]
    fn the_persons_profiles_are_found_through_links_and_dots() {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir().unwrap();
        let at = |rel: &str| root.path().join(rel);
        let home = at("home");
        for dir in [
            "home/.config/chromium/Default",
            "home/.config/google-chrome-beta",
            "home/.config/BraveSoftware/Brave-Browser",
            "home/.config/my-tests",
            "home/.mozilla/firefox/x.default",
            "home/snap/chromium/2345",
            "home/snap/chromium/common/chromium/Default",
            "home/snap/chromium/common/exponential-space/p9-space-1",
            "home/snap/spotify/common",
            "home/.var/app/org.mozilla.firefox/.mozilla/firefox",
            "home/.var/app/org.mozilla.firefox/exponential-space/p9-space-1",
            "home/.var/app/org.gnome.Maps/data",
            "home/.var/app/org.gnome.Epiphany/data/epiphany",
            "home/.local/share/epiphany",
            "home/.local/share/my-data",
            "home/.zen/x.default",
            "elsewhere/librewolf",
            "xdg/chromium",
            "work/a",
        ] {
            std::fs::create_dir_all(at(dir)).unwrap();
        }
        symlink("2345", at("home/snap/chromium/current")).unwrap();
        symlink(at("home/.config/chromium"), at("work/link")).unwrap();
        symlink("../home/.mozilla", at("work/relative-link")).unwrap();
        symlink(at("elsewhere/librewolf"), at("home/.librewolf")).unwrap();
        symlink(at("work/loop"), at("work/loop")).unwrap();
        symlink(&home, at("home-link")).unwrap();
        let configs = [home.join(".config"), at("xdg")];
        let datas = [home.join(".local/share")];
        let persons = Persons { home: &home, configs: &configs, datas: &datas, cwd: &at("work") };
        let path = |rel: &str| at(rel).to_string_lossy().into_owned();

        for kept in [
            path("work/playwright_chromiumdev_profile-x"),
            path("work/a"),
            path("work/a/../b"),
            "pw-profile".into(),
            path("home/.config/my-tests"),
            path("home/.config"),
            path("home"),
            path("home/snap/chromium/common/exponential-space/p9-space-1"),
            path("home/snap/spotify/common"),
            path("home/.var/app/org.mozilla.firefox/exponential-space/p9-space-1"),
            path("home/.var/app/org.gnome.Maps/data"),
            path("home/.local/share/my-data"),
        ] {
            assert!(!persons.profile(&kept), "{kept} is not the person's");
        }
        for refused in [
            path("home/.config/chromium"),
            path("home/.config/chromium/Default"),
            path("home/.config/google-chrome"),
            path("home/.config/google-chrome-beta/Default"),
            path("home/.config/google-chrome-for-testing"),
            path("home/.config/BraveSoftware/Brave-Browser"),
            path("home/.config/microsoft-edge-dev"),
            path("home/.config/vivaldi"),
            path("home/.config/mozilla/firefox"),
            path("home/.mozilla/firefox/x.default"),
            path("home/.librewolf"),
            path("elsewhere/librewolf/x.default"),
            // #EDE-166: GNOME Web's, Zen's, Opera's.
            path("home/.local/share/epiphany"),
            path("home/.var/app/org.gnome.Epiphany/data/epiphany"),
            path("home/.zen/x.default"),
            path("home/.config/opera"),
            path("xdg/chromium"),
            // A link to it, a relative one, through a link to the home.
            path("work/link"),
            path("work/link/Default"),
            path("work/relative-link/firefox"),
            path("home-link/.config/chromium"),
            // `..` into it, also past a directory that is not there yet.
            path("work/../home/.config/chromium"),
            path("work/a/../../home/.mozilla"),
            path("work/not-yet/../../home/.config/BraveSoftware"),
            path("work/link/../chromium/Default"),
            "../home/.config/chromium".into(),
            "a/../link".into(),
            // The snap's and the Flatpak's data, but for the spaces' own.
            path("home/snap/chromium/common/chromium"),
            path("home/snap/chromium/current/.config/chromium"),
            path("home/snap/chromium/2345"),
            path("home/snap/chromium/common/exponential-space/../chromium"),
            path("home/snap/firefox/common/.mozilla/firefox"),
            path("home/.var/app/org.mozilla.firefox/.mozilla/firefox"),
            path("home/.var/app/com.google.Chrome/config/google-chrome"),
            path("home/.var/app/org.mozilla.firefox/exponential-space/../.mozilla"),
            // A loop of links leads nowhere a browser could start in.
            path("work/loop"),
        ] {
            assert!(persons.profile(&refused), "{refused} is the person's");
        }
        let tilde = Persons { home: &home, configs: &configs, datas: &datas, cwd: Path::new("/") };
        assert!(tilde.profile("~/.config/chromium") && tilde.profile("~/.mozilla") && !tilde.profile("~/work"));
        assert_eq!(resolved(&at("work/link/../BraveSoftware/x")), Some(std::fs::canonicalize(home.join(".config")).unwrap().join("BraveSoftware/x")), "`..` after a link is taken where the link leads");
    }

    /// #EDE-166: what a launch's snap or Flatpak is, known or not.
    #[test]
    fn a_launchs_snap_or_flatpak_is_found_known_or_not() {
        let none = |_: &str| false;
        let opera = |snap: &str| snap == "unknown-browser";
        let of = |args: &[&str], path: Option<&str>, canonical: Option<&str>, script: Option<&'static str>, snaps: &dyn Fn(&str) -> bool| {
            sandboxed_of(&argv(args), &Found { path: path.map(Path::new), canonical: canonical.map(Path::new), runs_a_snap: script, snap_installed: snaps })
        };
        assert_eq!(of(&["snap", "run", "some-browser.beta", "x"], None, None, None, &none), Some(Sandboxed::Snap("some-browser".into())));
        assert_eq!(of(&["flatpak", "run", "--branch=stable", "org.example.Web", "x"], None, None, None, &none), Some(Sandboxed::Flatpak("org.example.Web".into())));
        assert_eq!(of(&["org.example.Web"], Some("/home/a/.local/share/flatpak/exports/bin/org.example.Web"), None, None, &none), Some(Sandboxed::Flatpak("org.example.Web".into())));
        assert_eq!(of(&["some-browser"], Some("/snap/bin/some-browser"), Some("/usr/bin/snap"), None, &none), Some(Sandboxed::Snap("some-browser".into())));
        assert_eq!(of(&["web"], Some("/usr/bin/web"), Some("/usr/bin/web"), Some("unknown-browser"), &opera), Some(Sandboxed::Snap("unknown-browser".into())), "a script that runs an installed snap");
        assert_eq!(of(&["web"], Some("/usr/bin/web"), Some("/usr/bin/web"), Some("unknown-browser"), &none), None, "its snap is not installed");
        assert_eq!(of(&["xterm"], Some("/usr/bin/xterm"), Some("/usr/bin/xterm"), None, &none), None);
        assert_eq!(snap_of_script(b"#!/bin/sh\nexec /snap/bin/firefox.beta \"$@\"\n").as_deref(), Some("firefox"));
        assert_eq!(snap_of_script(b"#!/bin/sh\nexec /usr/lib/firefox \"$@\"\n"), None);
        assert_eq!(snap_of_script(b"ELF /snap/bin/firefox"), None, "not a script");
    }

    /// #EDE-166: a `.desktop` file says its app is a browser by its
    /// category or the links it opens; found by the snap's or Flatpak's
    /// name.
    #[test]
    fn a_browser_is_known_by_its_desktop_file() {
        assert!(desktop_is_browser("[Desktop Entry]\nName=Web\nCategories=Network;WebBrowser;\n"));
        assert!(desktop_is_browser("[Desktop Entry]\nMimeType=text/html;x-scheme-handler/https;\n"));
        assert!(!desktop_is_browser("[Desktop Entry]\nCategories=Network;Chat;\nMimeType=x-scheme-handler/irc;\n"));
        assert!(!desktop_is_browser("[Desktop Entry]\nCategories=Game;\n[Desktop Action web]\nCategories=WebBrowser;\n"), "another group's key");
        let dir = tempfile::tempdir().unwrap();
        for name in ["org.example.Web.desktop", "org.example.Web.Private.desktop", "org.example.WebTools.desktop", "some-browser_some-browser.desktop", "some-browserx_a.desktop"] {
            std::fs::write(dir.path().join(name), "").unwrap();
        }
        let names = |app| {
            let mut names: Vec<String> = desktop_files(&app, &[dir.path().to_path_buf(), dir.path().join("missing")]).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
            names.sort();
            names
        };
        assert_eq!(names(Sandboxed::Flatpak("org.example.Web".into())), ["org.example.Web.Private.desktop", "org.example.Web.desktop"]);
        assert_eq!(names(Sandboxed::Snap("some-browser".into())), ["some-browser_some-browser.desktop"]);
    }

    /// #EDE-166: GNOME Web's profile flag, a caller's own, the person's.
    #[test]
    fn gnome_webs_profile_is_the_spaces_unless_its_own() {
        let dir = Path::new("/p");
        let web = Browser { kind: Kind::Epiphany, name: "epiphany", packaging: Packaging::Flatpak("org.gnome.Epiphany"), at: 2 };
        assert_eq!(with_profile(&argv(&["flatpak", "run", "org.gnome.Epiphany", "https://x"]), &web, Some(dir), true), argv(&["flatpak", "run", "org.gnome.Epiphany", "--profile=/p", "https://x"]), "no SwiftShader switches: WebKit");
        let persons = |dir: &str| dir.starts_with("/home/a/.local/share/epiphany");
        let profile = |args: &[&str]| callers_profile(Kind::Epiphany, &argv(args), &persons);
        assert_eq!(profile(&["https://x"]), Ok(false));
        assert_eq!(profile(&["--profile=/tmp/x"]), Ok(true));
        assert_eq!(profile(&["--profile", "/tmp/x"]), Ok(true));
        assert_eq!(profile(&["--private-instance"]), Ok(true), "a temporary profile of its own");
        assert_eq!(profile(&["--automation-mode"]), Ok(true));
        assert_eq!(profile(&["--", "--profile=/home/a/.local/share/epiphany"]), Ok(false));
        for refused in [&["--profile=/home/a/.local/share/epiphany"][..], &["--profile", "/home/a/.local/share/epiphany"], &["--profile="], &["--profile"], &["--profile", "--private-instance"]] {
            assert!(profile(refused).is_err(), "{refused:?}");
        }
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
