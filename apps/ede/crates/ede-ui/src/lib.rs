//! `ede-ui`: eDE's eUI-on-disk runtime (#NEW-107).
//!
//! The shell (#NEW-108) draws documents from `$XDG_CONFIG_HOME/ede/ui`:
//! one eUI surface per `<name>.json`, the look in `theme.json`. Agents
//! (ede-tools, #NEW-115) and people edit those files; this crate makes
//! that safe:
//!
//! - every file is validated by eui-core before it applies; a broken one
//!   never reaches the screen, the last good version stays (in memory, or
//!   from git at start-up), and the issues say why;
//! - the directory is a git repository and every applied edit is a commit
//!   (hand edits too, once they validate), so undo is `git revert`;
//! - [`watch`] reloads on change (inotify) and reports [`Change`]s.
//!
//! ```no_run
//! let (mut ui, changes) = ede_ui::Runtime::open(ede_ui::default_dir())?;
//! for c in changes { println!("{} {}", c.kind(), c.name()); }
//! let panel = ui.document("panel");
//! # Ok::<(), std::io::Error>(())
//! ```

pub mod apply;
pub mod doc;
pub mod git;
pub mod supervise;
pub mod surface;
pub mod watch;

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
};

pub use doc::{validate_document, validate_theme, Body, Document, Issue};
pub use surface::{normalize_surface, SurfaceHeader};
pub use eui_core::theme::ResolvedTheme;
pub use git::Commit;
pub use watch::{watch, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The theme's file; every other `*.json` is a document.
pub const THEME_FILE: &str = "theme.json";
/// The theme's name in [`Change`]s.
pub const THEME: &str = "theme";

/// Event kinds ([`Change::kind`]), in ede-protocol's `noun.verb` style.
pub mod events {
    pub const UI_APPLIED: &str = "ui.applied";
    pub const UI_REJECTED: &str = "ui.rejected";
    pub const UI_REMOVED: &str = "ui.removed";
    pub const THEME_APPLIED: &str = "theme.applied";
    pub const THEME_REJECTED: &str = "theme.rejected";
}

/// The factory documents (#EDE-9): `$EDE_FACTORY_DIR`, else `/usr/share/ede/ui`.
pub const FACTORY_DIR: &str = "/usr/share/ede/ui";

pub fn default_factory_dir() -> PathBuf {
    std::env::var_os("EDE_FACTORY_DIR").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(FACTORY_DIR))
}

/// A short content digest (FNV-1a 64, hex): what the shell reports with a
/// frame so the supervisor knows which edit it drew. Not a security hash.
pub fn digest(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// `$EDE_UI_DIR` (the supervisor hands it to the shell), else
/// `$XDG_CONFIG_HOME/ede/ui`, else `~/.config/ede/ui`.
pub fn default_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("EDE_UI_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    config.join("ede").join("ui")
}

/// A document name is the file stem: lowercase letters, digits, `-` and
/// `_`, starting with a letter or digit (`panel`, `app-grid`).
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        && name != THEME
}

/// What changed in one reload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Change {
    /// `name` applied; `commit` = the commit that recorded it (`None` when
    /// git already had it).
    #[serde(rename = "ui.applied")]
    Applied { name: String, commit: Option<String> },
    /// `name` on disk is broken; `fallback` = the last good version stays.
    #[serde(rename = "ui.rejected")]
    Rejected { name: String, issues: Vec<Issue>, fallback: bool },
    #[serde(rename = "ui.removed")]
    Removed { name: String, commit: Option<String> },
    /// `builtin` = no `theme.json`: eUI's default theme.
    #[serde(rename = "theme.applied")]
    ThemeApplied { id: String, builtin: bool, commit: Option<String> },
    #[serde(rename = "theme.rejected")]
    ThemeRejected { issues: Vec<Issue>, fallback: String },
}

impl Change {
    pub fn kind(&self) -> &'static str {
        match self {
            Change::Applied { .. } => events::UI_APPLIED,
            Change::Rejected { .. } => events::UI_REJECTED,
            Change::Removed { .. } => events::UI_REMOVED,
            Change::ThemeApplied { .. } => events::THEME_APPLIED,
            Change::ThemeRejected { .. } => events::THEME_REJECTED,
        }
    }

    /// The document's name, or [`THEME`].
    pub fn name(&self) -> &str {
        match self {
            Change::Applied { name, .. } | Change::Rejected { name, .. } | Change::Removed { name, .. } => name,
            Change::ThemeApplied { .. } | Change::ThemeRejected { .. } => THEME,
        }
    }
}

/// Why an edit through the API did not apply.
#[derive(Debug)]
pub enum EditError {
    /// eui-core refused it; nothing was written.
    Invalid(Vec<Issue>),
    /// Not a document name ([`valid_name`]).
    Name(String),
    Io(io::Error),
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditError::Invalid(issues) => {
                write!(f, "invalid:")?;
                for i in issues {
                    write!(f, "\n  {i}")?;
                }
                Ok(())
            }
            EditError::Name(name) => write!(f, "not a document name: {name:?} (lowercase letters, digits, - and _)"),
            EditError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for EditError {}

impl From<io::Error> for EditError {
    fn from(e: io::Error) -> Self {
        EditError::Io(e)
    }
}

/// What a file on disk was the last time a reload read it.
#[derive(Debug, Clone, PartialEq)]
enum Seen {
    Bytes(Vec<u8>),
    Missing,
}

/// The ui directory, its last good documents and theme.
pub struct Runtime {
    dir: PathBuf,
    repo: git::Repo,
    /// False in the shell process: it validates and reloads but never
    /// writes history (the supervisor owns git).
    commits: bool,
    factory: Option<PathBuf>,
    documents: BTreeMap<String, Document>,
    rejected: BTreeMap<String, Vec<Issue>>,
    theme: Arc<ResolvedTheme>,
    theme_builtin: bool,
    theme_rejected: Option<Vec<Issue>>,
    seen: BTreeMap<String, Seen>,
}

fn file_of(name: &str) -> String {
    if name == THEME {
        THEME_FILE.to_string()
    } else {
        format!("{name}.json")
    }
}

/// The names of the top-level `*.json` files (hidden files, editor
/// leftovers and anything that is not a document name are skipped).
fn names_on_disk(dir: &Path) -> io::Result<Vec<String>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let file = entry.file_name();
        let Some(file) = file.to_str() else { continue };
        if file == THEME_FILE {
            out.push(THEME.to_string());
        } else if let Some(stem) = file.strip_suffix(".json").filter(|s| valid_name(s)) {
            out.push(stem.to_string());
        }
    }
    out.sort();
    Ok(out)
}

fn read(path: &Path) -> io::Result<Seen> {
    match fs::read(path) {
        Ok(bytes) => Ok(Seen::Bytes(bytes)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Seen::Missing),
        Err(e) => Err(e),
    }
}

/// Write through a temporary file and a rename, so a reader (the watcher,
/// the shell) never sees half a document.
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_file_name(format!(".{}.tmp", path.file_name().and_then(|f| f.to_str()).unwrap_or("ede-ui")));
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

impl Runtime {
    /// Open (creating it and its git repository if needed) and load `dir`.
    /// The changes are what applied or was refused at start-up.
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<(Runtime, Vec<Change>)> {
        Self::open_with(dir.into(), None, true)
    }

    /// Open with the factory documents in `factory` (#EDE-9): every
    /// `<name>.json` there that the ui directory never had is copied in and
    /// committed as `Factory <name>`, so a fresh account starts with the
    /// shell and a revert always has somewhere to go.
    pub fn open_with_factory(dir: impl Into<PathBuf>, factory: impl Into<PathBuf>) -> io::Result<(Runtime, Vec<Change>)> {
        Self::open_with(dir.into(), Some(factory.into()), true)
    }

    /// The shell's view: validate, keep the last good version, reload on
    /// change, but never commit (the supervisor owns the history).
    pub fn open_readonly(dir: impl Into<PathBuf>) -> io::Result<(Runtime, Vec<Change>)> {
        Self::open_with(dir.into(), None, false)
    }

    fn open_with(dir: PathBuf, factory: Option<PathBuf>, commits: bool) -> io::Result<(Runtime, Vec<Change>)> {
        fs::create_dir_all(&dir)?;
        let repo = git::Repo::open(&dir)?;
        let mut ui = Runtime {
            dir,
            repo,
            commits,
            factory,
            documents: BTreeMap::new(),
            rejected: BTreeMap::new(),
            theme: eui_core::themes::default_theme(),
            theme_builtin: true,
            theme_rejected: None,
            seen: BTreeMap::new(),
        };
        let mut changes = Vec::new();
        for name in ui.factory_names()? {
            let file = file_of(&name);
            if !ui.dir.join(&file).exists() && ui.repo.show_head(&file)?.is_none() {
                changes.extend(ui.restore_factory(&name)?);
            }
        }
        changes.extend(ui.reload()?);
        Ok((ui, changes))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The documents the factory directory holds, sorted.
    pub fn factory_names(&self) -> io::Result<Vec<String>> {
        match &self.factory {
            Some(dir) if dir.is_dir() => names_on_disk(dir).map(|names| names.into_iter().filter(|n| n != THEME).collect()),
            _ => Ok(Vec::new()),
        }
    }

    /// Put the factory copy of `name` back, as the commit `Factory <name>`
    /// (the apply machine's `factory` effect).
    pub fn restore_factory(&mut self, name: &str) -> io::Result<Vec<Change>> {
        let Some(factory) = &self.factory else {
            return Err(io::Error::other("no factory directory"));
        };
        let file = file_of(name);
        let bytes = fs::read(factory.join(&file)).map_err(|e| io::Error::other(format!("factory {file}: {e}")))?;
        write_atomic(&self.dir.join(&file), &bytes)?;
        self.reload_with(Some(&format!("Factory {name}")))
    }

    /// Every factory document back as it ships, each a commit (#EDE-16:
    /// the restore hotkey; the person's edits stay in the history).
    pub fn reset_to_factory(&mut self) -> io::Result<Vec<Change>> {
        let mut changes = Vec::new();
        for name in self.factory_names()? {
            changes.extend(self.restore_factory(&name)?);
        }
        Ok(changes)
    }

    /// The git commit a document's current file sits in (`None` while it
    /// has an uncommitted edit or does not exist).
    pub fn head(&self) -> io::Result<Option<Commit>> {
        Ok(self.repo.log(1)?.into_iter().next())
    }

    /// Revert `commit` as a new commit (a rollback the person can undo
    /// again) and reload.
    pub fn revert_commit(&mut self, commit: &str, summary: &str) -> io::Result<(String, Vec<Change>)> {
        let id = self.repo.revert(commit, summary)?;
        let changes = self.reload()?;
        Ok((id, changes))
    }

    /// The last good version of `name`.
    pub fn document(&self, name: &str) -> Option<&Document> {
        self.documents.get(name)
    }

    pub fn documents(&self) -> impl Iterator<Item = &Document> {
        self.documents.values()
    }

    /// The theme in effect (the last good `theme.json`, else eUI's default).
    pub fn theme(&self) -> &Arc<ResolvedTheme> {
        &self.theme
    }

    /// Why the file on disk is not what is shown, per document.
    pub fn rejected(&self) -> &BTreeMap<String, Vec<Issue>> {
        &self.rejected
    }

    pub fn theme_rejected(&self) -> Option<&[Issue]> {
        self.theme_rejected.as_deref()
    }

    /// Re-read every file that changed since the last reload, apply what
    /// validates and commit it. Hand edits get the message `Edit <files>`.
    pub fn reload(&mut self) -> io::Result<Vec<Change>> {
        self.reload_with(None)
    }

    fn reload_with(&mut self, message: Option<&str>) -> io::Result<Vec<Change>> {
        let mut names = names_on_disk(&self.dir)?;
        for name in self.seen.keys().chain(self.documents.keys()) {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        if !names.iter().any(|n| n == THEME) {
            names.push(THEME.to_string());
        }
        names.sort();
        names.dedup();

        let mut changes = Vec::new();
        // Files to commit, and the index in `changes` of the change each commit fills in.
        let mut to_commit: Vec<(String, usize)> = Vec::new();
        for name in names {
            let file = file_of(&name);
            let now = read(&self.dir.join(&file))?;
            if self.seen.get(&name) == Some(&now) {
                continue;
            }
            let first = self.seen.insert(name.clone(), now.clone()).is_none();
            let change = if name == THEME { self.load_theme(&now, first)? } else { self.load_document(&name, &now)? };
            if let Some(change) = change {
                if matches!(change, Change::Applied { .. } | Change::Removed { .. } | Change::ThemeApplied { .. }) {
                    to_commit.push((file, changes.len()));
                }
                changes.push(change);
            }
        }

        let files: Vec<String> = to_commit.iter().map(|(f, _)| f.clone()).collect();
        let message = message.map(str::to_string).unwrap_or_else(|| format!("Edit {}", files.join(", ")));
        if !self.commits {
            return Ok(changes);
        }
        if let Some(id) = self.commit_tracked(&files, &message)? {
            for (_, i) in &to_commit {
                if let Change::Applied { commit, .. } | Change::Removed { commit, .. } | Change::ThemeApplied { commit, .. } = &mut changes[*i] {
                    *commit = Some(id.clone());
                }
            }
        }
        Ok(changes)
    }

    /// Commit `files`, leaving out a deleted one git never had.
    fn commit_tracked(&self, files: &[String], message: &str) -> io::Result<Option<String>> {
        let mut tracked = Vec::new();
        for f in files {
            if self.dir.join(f).exists() || self.repo.show_head(f)?.is_some() {
                tracked.push(f.clone());
            }
        }
        self.repo.commit(&tracked, message)
    }

    /// HEAD's version of `file` if it still validates (the last good one
    /// across restarts).
    fn from_head<T>(&self, file: &str, validate: impl Fn(&Value) -> Result<T, Vec<Issue>>) -> io::Result<Option<T>> {
        let Some(bytes) = self.repo.show_head(file)? else { return Ok(None) };
        Ok(doc::parse(&bytes).ok().and_then(|v| validate(&v).ok()))
    }

    fn load_document(&mut self, name: &str, now: &Seen) -> io::Result<Option<Change>> {
        let Seen::Bytes(bytes) = now else {
            self.rejected.remove(name);
            return Ok(self.documents.remove(name).map(|_| Change::Removed { name: name.to_string(), commit: None }));
        };
        match doc::parse(bytes).and_then(|v| validate_document(name, &v)) {
            Ok(mut document) => {
                document.digest = digest(bytes);
                self.rejected.remove(name);
                self.documents.insert(name.to_string(), document);
                Ok(Some(Change::Applied { name: name.to_string(), commit: None }))
            }
            Err(issues) => {
                if !self.documents.contains_key(name) {
                    if let Some(good) = self.from_head(&file_of(name), |v| validate_document(name, v))? {
                        self.documents.insert(name.to_string(), good);
                    }
                }
                self.rejected.insert(name.to_string(), issues.clone());
                Ok(Some(Change::Rejected { name: name.to_string(), issues, fallback: self.documents.contains_key(name) }))
            }
        }
    }

    /// `first` = the first reload: no theme.json reports the default.
    fn load_theme(&mut self, now: &Seen, first: bool) -> io::Result<Option<Change>> {
        let Seen::Bytes(bytes) = now else {
            let was_builtin = self.theme_builtin;
            self.theme = eui_core::themes::default_theme();
            self.theme_builtin = true;
            self.theme_rejected = None;
            return Ok((!was_builtin || first).then(|| Change::ThemeApplied { id: self.theme.id.clone(), builtin: true, commit: None }));
        };
        match doc::parse(bytes).and_then(|v| validate_theme(&v)) {
            Ok(theme) => {
                self.theme = theme;
                self.theme_builtin = false;
                self.theme_rejected = None;
                Ok(Some(Change::ThemeApplied { id: self.theme.id.clone(), builtin: false, commit: None }))
            }
            Err(issues) => {
                if self.theme_builtin {
                    if let Some(good) = self.from_head(THEME_FILE, validate_theme)? {
                        self.theme = good;
                        self.theme_builtin = false;
                    }
                }
                self.theme_rejected = Some(issues.clone());
                Ok(Some(Change::ThemeRejected { issues, fallback: self.theme.id.clone() }))
            }
        }
    }

    /// Validate `source` and, if eui-core accepts it, write `<name>.json`
    /// and commit it with `message`. A refused document writes nothing.
    pub fn write(&mut self, name: &str, source: &Value, message: &str) -> Result<Vec<Change>, EditError> {
        if !valid_name(name) {
            return Err(EditError::Name(name.to_string()));
        }
        validate_document(name, source).map_err(EditError::Invalid)?;
        self.write_file(&file_of(name), source, message)
    }

    /// The same for `theme.json`.
    pub fn write_theme(&mut self, source: &Value, message: &str) -> Result<Vec<Change>, EditError> {
        validate_theme(source).map_err(EditError::Invalid)?;
        self.write_file(THEME_FILE, source, message)
    }

    fn write_file(&mut self, file: &str, source: &Value, message: &str) -> Result<Vec<Change>, EditError> {
        if !self.commits {
            return Err(EditError::Io(io::Error::other("a read-only runtime never writes (the supervisor owns the history)")));
        }
        let mut bytes = serde_json::to_vec_pretty(source).map_err(io::Error::other)?;
        bytes.push(b'\n');
        write_atomic(&self.dir.join(file), &bytes)?;
        Ok(self.reload_with(Some(message))?)
    }

    /// Delete `<name>.json` (or the theme: back to the default) as a commit.
    pub fn remove(&mut self, name: &str, message: &str) -> Result<Vec<Change>, EditError> {
        if name != THEME && !valid_name(name) {
            return Err(EditError::Name(name.to_string()));
        }
        match fs::remove_file(self.dir.join(file_of(name))) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        Ok(self.reload_with(Some(message))?)
    }

    /// Throw away the uncommitted (broken) edit of `name`: the file goes
    /// back to the last commit.
    pub fn discard(&mut self, name: &str) -> io::Result<Vec<Change>> {
        self.repo.restore(&file_of(name))?;
        self.reload()
    }

    /// Undo the newest edit: `git revert` it as a new commit (so undo is
    /// itself undoable) and reload. `Ok(None)` when there is nothing left to
    /// undo. Refused while a file that edit touched has an uncommitted
    /// change ([`Self::discard`] it first).
    pub fn undo(&mut self) -> io::Result<Option<(Commit, Vec<Change>)>> {
        let log = self.repo.log(2)?;
        if log.len() < 2 {
            return Ok(None);
        }
        let last = &log[0];
        for file in self.repo.files_of(&last.id)? {
            let disk = read(&self.dir.join(&file))?;
            let head = self.repo.show_head(&file)?.map(Seen::Bytes).unwrap_or(Seen::Missing);
            if disk != head {
                return Err(io::Error::other(format!("{file} has an uncommitted edit; discard it first")));
            }
        }
        let id = self.repo.revert(&last.id, &last.summary)?;
        let changes = self.reload()?;
        let commit = self.repo.log(1)?.into_iter().next().unwrap_or(Commit { id, summary: String::new(), time: 0 });
        Ok(Some((commit, changes)))
    }

    /// The history, newest first.
    pub fn log(&self, limit: usize) -> io::Result<Vec<Commit>> {
        self.repo.log(limit)
    }
}
