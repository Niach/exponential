//! The installed applications for the launcher (#EDE-12): freedesktop
//! desktop entries from the XDG data directories, their icons resolved to
//! files in the installed icon themes, and the search the menu's box drives.
//! Behaviour follows the Desktop Entry and Icon Theme specifications; no
//! code is shared with Cinnamon's menu.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};

/// One launchable entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    /// The desktop file's id (`org.gnome.Calculator.desktop`).
    pub id: String,
    pub name: String,
    pub comment: String,
    /// `Exec` with the field codes stripped, wrapped in a terminal when
    /// `Terminal=true`.
    pub exec: String,
    /// #EDE-65: the raw `Exec` when it takes URLs (`%u`/`%U`), for
    /// [`exec_with_url`]; `None` = the entry takes none.
    pub exec_url: Option<String>,
    pub icon: Option<PathBuf>,
    /// Lowercase haystack for the search: name, generic name, keywords, id.
    pub keywords: String,
    /// `Categories=` (freedesktop main and additional categories), for the
    /// start menu's groups.
    pub categories: Vec<String>,
}

impl App {
    pub fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "name": self.name,
            "comment": self.comment,
            "exec": self.exec,
            "icon": self.icon.as_ref().map(|p| format!("file://{}", p.display())).unwrap_or_default(),
            "categories": self.categories,
        })
    }

    /// The entry as the shell reducer's start menu sees it.
    pub fn start_menu_app(&self) -> uicore_model::StartMenuApp {
        uicore_model::StartMenuApp { id: self.id.trim_end_matches(".desktop").to_string(), name: self.name.clone(), comment: self.comment.clone(), categories: self.categories.clone(), keywords: self.keywords.clone() }
    }
}

/// `$XDG_DATA_HOME/applications`, then every `$XDG_DATA_DIRS/applications`;
/// the first file of an id wins, as the specification says.
pub fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let home = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")));
    if let Some(h) = home {
        dirs.push(h);
    }
    let system = std::env::var("XDG_DATA_DIRS").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    dirs.extend(system.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    // Flatpak and snap exports, when the data dirs do not list them.
    for extra in ["/var/lib/flatpak/exports/share", "/var/lib/snapd/desktop"] {
        let p = PathBuf::from(extra);
        if !dirs.contains(&p) {
            dirs.push(p);
        }
    }
    dirs
}

/// Every entry shown in eDE, sorted by name.
pub fn scan() -> Vec<App> {
    scan_in(&data_dirs(), false).apps
}

/// #EDE-227: the app id of eDE's component (`exponential-app`), the one
/// Exponential the desk shows while the component runs.
pub const COMPONENT_APP_ID: &str = "at.exponential";

/// What [`scan_in`] found.
#[derive(Debug, Default)]
pub struct Scan {
    /// The entries shown, sorted by name.
    pub apps: Vec<App>,
    /// #EDE-227: the ids (no `.desktop`) of the person's own app entries
    /// left out for the component ([`is_standalone_app`]), sorted.
    pub standalone: Vec<String>,
}

/// The entries of `dirs` (each with an `applications` folder, the first
/// file of an id wins). `component` = eDE's component runs as the
/// session's helper (#EDE-227): the person's own app entries (the
/// AppImage's, `exp-desktop`'s) are hidden then, so the start menu, its
/// search and favourites show one Exponential, not two.
pub fn scan_in(dirs: &[PathBuf], component: bool) -> Scan {
    let mut seen: HashMap<String, App> = HashMap::new();
    let mut standalone = Vec::new();
    for dir in dirs {
        let Ok(entries) = fs::read_dir(dir.join("applications")) else { continue };
        let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "desktop")).collect();
        files.sort();
        for file in files {
            let id = file.file_name().unwrap().to_string_lossy().into_owned();
            if seen.contains_key(&id) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&file) else { continue };
            let parsed = if component && is_standalone_app(&text) {
                standalone.push(id.trim_end_matches(".desktop").to_string());
                Parsed::Hidden
            } else {
                parse(&id, &text, "Exponential")
            };
            // A hidden entry in a higher-priority directory hides the id entirely.
            match parsed {
                Parsed::Shown(app) => {
                    seen.insert(id, app);
                }
                Parsed::Hidden => {
                    seen.insert(id, App { id: String::new(), name: String::new(), comment: String::new(), exec: String::new(), exec_url: None, icon: None, keywords: String::new(), categories: Vec::new() });
                }
                Parsed::NotAnApp => {}
            }
        }
    }
    let mut apps: Vec<App> = seen.into_values().filter(|a| !a.id.is_empty()).collect();
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    standalone.sort();
    Scan { apps, standalone }
}

/// #EDE-227: whether a desktop file is the person's own Exponential app,
/// as its launcher integrations write it: `Exec` runs an AppImage (the
/// app's `desktop_integration`, AppImageLauncher's copy of the AppImage's
/// entry) or `exp-desktop` (the package template), and `StartupWMClass`
/// is the component's app id (its windows' app id; a staging build's
/// `at.exponential.staging` is another app and stays).
pub fn is_standalone_app(text: &str) -> bool {
    let (mut section, mut exec, mut class) = ("", None, None);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        if section != "Desktop Entry" || line.starts_with('#') {
            continue;
        }
        match line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
            Some(("Exec", v)) if exec.is_none() => exec = Some(v),
            Some(("StartupWMClass", v)) if class.is_none() => class = Some(v),
            _ => {}
        }
    }
    if !class.is_some_and(|c| c.eq_ignore_ascii_case(COMPONENT_APP_ID)) {
        return false;
    }
    let Some(program) = exec.map(exec_program) else { return false };
    let name = Path::new(&program).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    name == "exp-desktop" || name.to_lowercase().ends_with(".appimage")
}

/// The program of a raw `Exec` value: its first argument, unquoted as the
/// Desktop Entry spec says (the key-file layer's `\\` first, then a
/// double-quoted argument's `\"` `\`` `\$` `\\` escapes).
fn exec_program(exec: &str) -> String {
    let mut unescaped = String::with_capacity(exec.len());
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('s') => unescaped.push(' '),
                Some('n') => unescaped.push('\n'),
                Some('t') => unescaped.push('\t'),
                Some('r') => unescaped.push('\r'),
                Some(other) => unescaped.push(other),
                None => {}
            }
        } else {
            unescaped.push(c);
        }
    }
    let Some(quoted) = unescaped.strip_prefix('"') else {
        return unescaped.split_whitespace().next().unwrap_or_default().to_string();
    };
    let mut program = String::new();
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => program.extend(chars.next()),
            c => program.push(c),
        }
    }
    program
}

/// Fixed sample entries for the goldens (`EDE_SHELL_DEMO=1`): no icons, so
/// the fallback glyph draws the same on every machine.
pub fn demo() -> Vec<App> {
    [
        ("org.gnome.Calculator", "Calculator", "Perform arithmetic, scientific or financial calculations", "gnome-calculator", "Utility"),
        ("nemo", "Files", "Access and organize files", "nemo", "Utility"),
        ("firefox", "Firefox", "Browse the web", "firefox", "Network"),
        ("org.gnome.TextEditor", "Text Editor", "View and edit text files", "gnome-text-editor", "Utility"),
        ("terminal", "Terminal", "Use the command line", "x-terminal-emulator", "System"),
    ]
    .into_iter()
    .map(|(id, name, comment, exec, category)| App { id: format!("{id}.desktop"), name: name.into(), comment: comment.into(), exec: exec.into(), exec_url: None, icon: None, keywords: format!("{name} {id}").to_lowercase(), categories: vec![category.into()] })
    .collect()
}

pub enum Parsed {
    Shown(App),
    /// `NoDisplay`, `Hidden`, or not for this desktop.
    Hidden,
    NotAnApp,
}

/// One desktop file; `desktop` is `XDG_CURRENT_DESKTOP`.
pub fn parse(id: &str, text: &str, desktop: &str) -> Parsed {
    let mut section = "";
    let mut kv: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        if section != "Desktop Entry" || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            kv.entry(k.trim()).or_insert(v.trim());
        }
    }
    let get = |k: &str| kv.get(k).copied().unwrap_or("");
    if get("Type") != "Application" || get("Exec").is_empty() {
        return Parsed::NotAnApp;
    }
    let truthy = |v: &str| v.eq_ignore_ascii_case("true");
    if truthy(get("NoDisplay")) || truthy(get("Hidden")) {
        return Parsed::Hidden;
    }
    fn list(v: &str) -> Vec<&str> {
        v.split(';').map(str::trim).filter(|s| !s.is_empty()).collect()
    }
    let only = list(get("OnlyShowIn"));
    if !only.is_empty() && !only.iter().any(|d| d.eq_ignore_ascii_case(desktop)) {
        return Parsed::Hidden;
    }
    if list(get("NotShowIn")).iter().any(|d| d.eq_ignore_ascii_case(desktop)) {
        return Parsed::Hidden;
    }
    if let Some(try_exec) = kv.get("TryExec").filter(|v| !v.is_empty()) {
        if !executable_exists(try_exec) {
            return Parsed::Hidden;
        }
    }
    let exec = strip_field_codes(get("Exec"));
    let terminal = truthy(get("Terminal"));
    let exec = if terminal { format!("x-terminal-emulator -e {exec}") } else { exec };
    let raw = get("Exec");
    let exec_url = (raw.contains("%u") || raw.contains("%U")).then(|| if terminal { format!("x-terminal-emulator -e {raw}") } else { raw.to_string() });
    let name = get("Name").to_string();
    let generic = get("GenericName");
    let keywords = format!("{} {} {} {}", name, generic, get("Keywords").replace(';', " "), id.trim_end_matches(".desktop")).to_lowercase();
    Parsed::Shown(App {
        id: id.to_string(),
        name,
        comment: get("Comment").to_string(),
        exec,
        exec_url,
        icon: Some(get("Icon")).filter(|i| !i.is_empty()).and_then(icon_file),
        keywords,
        categories: list(get("Categories")).into_iter().map(String::from).collect(),
    })
}

fn executable_exists(name: &str) -> bool {
    if name.contains('/') {
        return Path::new(name).exists();
    }
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).exists()))
}

/// #EDE-65: the command that opens `url` with an entry's raw `Exec`
/// ([`App::exec_url`]): `%u`/`%U` become the URL, shell-quoted (the shell
/// launches through `sh -c`), the other field codes go, `%%` is a percent.
pub fn exec_with_url(exec: &str, url: &str) -> String {
    let quoted = format!("'{}'", url.replace('\'', "'\\''"));
    let mut out = String::with_capacity(exec.len() + quoted.len());
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('u' | 'U') => out.push_str(&quoted),
            Some(_) | None => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `%f %F %u %U %i %c %k` and the deprecated ones go; `%%` is a percent.
pub fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some(_) | None => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The icon themes searched, most specific first; `hicolor` is the
/// fallback every theme inherits.
fn themes() -> Vec<String> {
    let mut themes: Vec<String> = Vec::new();
    if let Ok(t) = std::env::var("EDE_ICON_THEME") {
        themes.push(t);
    }
    for t in ["Papirus-Dark", "Papirus", "Yaru-dark", "Yaru", "Adwaita", "breeze-dark", "breeze", "hicolor"] {
        if !themes.iter().any(|x| x == t) {
            themes.push(t.into());
        }
    }
    themes
}

fn icon_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(h) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&h).join(".local/share/icons"));
        roots.push(PathBuf::from(h).join(".icons"));
    }
    for d in data_dirs() {
        roots.push(d.join("icons"));
    }
    roots
}

/// A desktop entry's `Icon` as a file: an absolute path as is, else the
/// first match in the themes' `apps` (then any) directories, scalable SVG
/// before the largest PNG, then `/usr/share/pixmaps`.
pub fn icon_file(name: &str) -> Option<PathBuf> {
    if name.starts_with('/') {
        return Path::new(name).exists().then(|| PathBuf::from(name));
    }
    let name = name.strip_suffix(".png").or_else(|| name.strip_suffix(".svg")).unwrap_or(name);
    for theme in themes() {
        for root in icon_roots() {
            let theme_dir = root.join(&theme);
            if !theme_dir.is_dir() {
                continue;
            }
            if let Some(found) = find_in_theme(&theme_dir, name) {
                return Some(found);
            }
        }
    }
    for ext in ["svg", "png", "xpm"] {
        let p = PathBuf::from("/usr/share/pixmaps").join(format!("{name}.{ext}"));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

fn find_in_theme(theme_dir: &Path, name: &str) -> Option<PathBuf> {
    // Scalable first, then the sizes from large to small, apps before the rest.
    let mut candidates: Vec<(u32, PathBuf)> = Vec::new();
    let Ok(sizes) = fs::read_dir(theme_dir) else { return None };
    for size in sizes.filter_map(|e| e.ok()) {
        let size_name = size.file_name().to_string_lossy().into_owned();
        let rank = if size_name == "scalable" { 100_000 } else { size_name.split('x').next().and_then(|n| n.parse::<u32>().ok()).unwrap_or(0) };
        let Ok(contexts) = fs::read_dir(size.path()) else { continue };
        for ctx in contexts.filter_map(|e| e.ok()) {
            let bonus = if ctx.file_name() == "apps" { 1 } else { 0 };
            for ext in ["svg", "png"] {
                let p = ctx.path().join(format!("{name}.{ext}"));
                if p.exists() {
                    candidates.push((rank * 2 + bonus, p));
                }
            }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.into_iter().next().map(|(_, p)| p)
}

/// The apps matching `query` (every word must appear), all of them for an
/// empty query.
pub fn filter(apps: &[App], query: &str) -> Vec<Value> {
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
    apps.iter().filter(|a| words.iter().all(|w| a.keywords.contains(w.as_str()))).map(App::to_json).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CALC: &str = "[Desktop Entry]\nType=Application\nName=Calculator\nComment=Do sums\nExec=gnome-calculator %U\nIcon=/nonexistent/calc.png\nKeywords=math;arithmetic;\nCategories=Utility;\n";

    #[test]
    fn parses_an_application_and_strips_field_codes() {
        let Parsed::Shown(app) = parse("org.gnome.Calculator.desktop", CALC, "Exponential") else { panic!("shown") };
        assert_eq!(app.name, "Calculator");
        assert_eq!(app.exec, "gnome-calculator");
        assert_eq!(app.exec_url.as_deref(), Some("gnome-calculator %U"));
        assert!(app.keywords.contains("arithmetic") && app.keywords.contains("org.gnome.calculator"));
        assert_eq!(strip_field_codes("sh -c 'x %% y' %f %U"), "sh -c 'x % y'");
        assert_eq!(exec_with_url("exponential-desktop %U", "exponential://issue/EXP-12"), "exponential-desktop 'exponential://issue/EXP-12'");
        assert_eq!(exec_with_url("app --x %u %i", "exponential://session/s1?opens=card&message=m1"), "app --x 'exponential://session/s1?opens=card&message=m1'");
        assert_eq!(exec_with_url("app %U", "it's"), "app 'it'\\''s'");
    }

    #[test]
    fn hidden_and_foreign_entries_stay_out() {
        assert!(matches!(parse("a.desktop", &format!("{CALC}NoDisplay=true\n"), "Exponential"), Parsed::Hidden));
        assert!(matches!(parse("a.desktop", &format!("{CALC}OnlyShowIn=GNOME;\n"), "Exponential"), Parsed::Hidden));
        assert!(matches!(parse("a.desktop", &format!("{CALC}OnlyShowIn=GNOME;Exponential;\n"), "Exponential"), Parsed::Shown(_)));
        assert!(matches!(parse("a.desktop", &format!("{CALC}NotShowIn=Exponential;\n"), "Exponential"), Parsed::Hidden));
        assert!(matches!(parse("a.desktop", "[Desktop Entry]\nType=Link\nURL=x\n", "Exponential"), Parsed::NotAnApp));
        let Parsed::Shown(term) = parse("t.desktop", &format!("{CALC}Terminal=true\n"), "Exponential") else { panic!() };
        assert_eq!(term.exec, "x-terminal-emulator -e gnome-calculator");
    }

    #[test]
    fn the_search_wants_every_word() {
        let Parsed::Shown(app) = parse("c.desktop", CALC, "Exponential") else { panic!() };
        let apps = vec![app];
        assert_eq!(filter(&apps, "").len(), 1);
        assert_eq!(filter(&apps, "calc math").len(), 1);
        assert_eq!(filter(&apps, "calc music").len(), 0);
        assert_eq!(filter(&apps, "CALC")[0]["exec"], "gnome-calculator");
    }

    /// #EDE-227: what the app's launcher integration writes (its
    /// `desktop_integration`), for an AppImage at a path with a space.
    fn integration_entry(exec: &str, class: &str) -> String {
        format!("[Desktop Entry]\nType=Application\nName=Exponential\nComment=Exponential desktop IDE\nExec={exec} %U\nIcon={class}\nTerminal=false\nCategories=Development;\nMimeType=x-scheme-handler/exponential;\nStartupWMClass={class}\n")
    }

    #[test]
    fn the_persons_own_app_entries_are_recognised_by_exec_and_class() {
        assert!(is_standalone_app(&integration_entry("/home/u/Apps/Exponential-0.14.150-x86_64.AppImage", "at.exponential")));
        assert!(is_standalone_app(&integration_entry("\"/home/u/My Apps/Exponential.AppImage\"", "at.exponential")), "a quoted path with a space");
        assert!(is_standalone_app(&integration_entry("\"/home/u/a\\\\$b/exp.appimage\"", "at.exponential")), "the spec's double escapes");
        // The package template (apps/desktop/assets/packaging/exponential.desktop).
        let template = include_str!("../../../assets/packaging/exponential.desktop");
        assert!(is_standalone_app(template), "exp-desktop %U");
        assert!(is_standalone_app(&integration_entry("/usr/bin/exp-desktop", "at.exponential")));
        // Not the person's app: another AppImage, a staging build, eDE's own component, a name alike.
        assert!(!is_standalone_app(&integration_entry("/home/u/Apps/Obsidian.AppImage", "obsidian")));
        assert!(!is_standalone_app(&integration_entry("/home/u/Apps/Exponential-staging.AppImage", "at.exponential.staging")));
        assert!(!is_standalone_app(&integration_entry("/usr/bin/exponential-app", "at.exponential")));
        assert!(!is_standalone_app(&integration_entry("/usr/bin/exponential", "at.exponential")));
        assert!(!is_standalone_app("[Desktop Entry]\nType=Application\nName=Exponential\nExec=/home/u/Exponential.AppImage\n"), "no class: a name guess");
    }

    #[test]
    fn with_the_component_the_persons_app_entry_is_left_out() {
        let home = tempfile::tempdir().unwrap();
        let system = tempfile::tempdir().unwrap();
        let write = |root: &Path, id: &str, text: &str| {
            fs::create_dir_all(root.join("applications")).unwrap();
            fs::write(root.join("applications").join(id), text).unwrap();
        };
        // The person's AppImage entry shadows eDE's own (hidden) one of the same id.
        write(home.path(), "at.exponential.desktop", &integration_entry("/home/u/Exponential.AppImage", "at.exponential"));
        write(home.path(), "appimagekit_1234-Exponential.desktop", &integration_entry("/home/u/Exponential.AppImage", "at.exponential"));
        write(system.path(), "at.exponential.desktop", "[Desktop Entry]\nType=Application\nName=Exponential\nExec=/usr/bin/exponential-app\nNoDisplay=true\nOnlyShowIn=Exponential;\nStartupWMClass=at.exponential\n");
        write(system.path(), "exponential.desktop", include_str!("../../../assets/packaging/exponential.desktop"));
        write(system.path(), "org.gnome.Calculator.desktop", CALC);
        let dirs = [home.path().to_path_buf(), system.path().to_path_buf()];
        let ids = |scan: &Scan| scan.apps.iter().map(|a| a.id.clone()).collect::<Vec<_>>();

        let without = scan_in(&dirs, false);
        assert_eq!(ids(&without).len(), 4, "without the component every entry shows");
        assert!(without.standalone.is_empty());

        let with = scan_in(&dirs, true);
        assert_eq!(ids(&with), ["org.gnome.Calculator.desktop"], "one Exponential: the component's, which has no entry of its own");
        assert_eq!(with.standalone, ["appimagekit_1234-Exponential", "at.exponential", "exponential"]);
        assert!(filter(&with.apps, "exponential").is_empty(), "nor in the search");
    }
}
