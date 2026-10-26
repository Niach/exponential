//! N-5 / NEW-5: confinement of a repo-less claude run to its scratch dir.
//!
//! The run's `can_use_tool` answer and its `PreToolUse` hook (which fires
//! even under `bypassPermissions`) both ask [`confinement_denial`]. A file
//! tool may touch the scratch dir, a temp dir and the run's own claude state
//! ([`claude_harness_dirs`], FEED-142); a Bash command is scanned for the
//! paths it names ([`bash_offending`]).

use std::path::{Path, PathBuf};

use serde_json::Value;

/// The file tools whose path argument a confined run checks, and the input
/// keys that path rides under. `Glob`/`Grep`/`LS` without a `path` search the
/// cwd, which is inside.
const CONFINED_FILE_TOOLS: [&str; 9] = [
    "Read",
    "Edit",
    "Write",
    "MultiEdit",
    "NotebookEdit",
    "NotebookRead",
    "Glob",
    "Grep",
    "LS",
];
const CONFINED_PATH_KEYS: [&str; 3] = ["file_path", "path", "notebook_path"];

/// System prefixes a Bash command may name without leaving the sandbox in any
/// way that matters (interpreters, `/dev/null`, `/etc/hosts`). Everything
/// else outside the scratch and temp dirs (`/`, `/Users`, `/home`, `~`,
/// `$HOME`, another volume) is refused.
const BASH_SYSTEM_PREFIXES: [&str; 12] = [
    "/dev",
    "/usr",
    "/bin",
    "/sbin",
    "/etc",
    "/private/etc",
    "/opt/homebrew",
    "/nix",
    "/proc",
    "/sys",
    "/System",
    "/Library/Developer/CommandLineTools",
];

/// Commands that only pass their wrapped command through.
const WRAPPERS: [&str; 8] = ["sudo", "env", "command", "exec", "time", "nohup", "nice", "builtin"];
/// Commands whose quoted arguments are themselves shell code.
const SHELLS: [&str; 7] = ["bash", "sh", "zsh", "dash", "ksh", "fish", "eval"];
/// Commands whose first operand is a regex, never a file.
const PATTERN_COMMANDS: [&str; 6] = ["grep", "egrep", "fgrep", "rg", "ag", "ack"];
/// Commands whose first operand is a script (`/re/`, `s/a/b/`).
const SCRIPT_COMMANDS: [&str; 5] = ["sed", "awk", "gawk", "mawk", "nawk"];
/// Commands whose operands are text.
const TEXT_COMMANDS: [&str; 8] = ["echo", "printf", "tr", "expr", "let", "test", "[", "[["];

/// Where a confined run may go: the scratch dir plus the run's own harness
/// dirs ([`claude_harness_dirs`]). Temp dirs are always open.
pub struct Confinement<'a> {
    /// The scratch dir as configured (named in the deny message).
    pub root: &'a Path,
    /// FEED-142: the run's own claude state (memory, tool results).
    pub harness: &'a [PathBuf],
}

/// FEED-142: claude keeps a run's own state under
/// `<CLAUDE_CONFIG_DIR>/projects/<munged cwd>/`: the agent's memory dir
/// (`memory/`, named in its system prompt) and the large tool results it reads
/// back (`<session>/tool-results/`). Both sit outside the scratch dir, so a
/// confined run allows that ONE project dir (the cwd's, never a sibling's).
///
/// The name munges every non-alphanumeric UTF-16 unit to `-`; past 200 chars
/// claude keeps the first 200 and appends `-<hash>` (EXP-761), so a long name
/// matches the existing dirs on that prefix.
pub fn claude_harness_dirs(config: Option<&Path>, dirs: &[&Path]) -> Vec<PathBuf> {
    let Some(projects) = config.map(|config| config.join("projects")) else { return Vec::new() };
    let mut names: Vec<String> = Vec::new();
    for dir in dirs {
        for path in [dir.to_path_buf(), resolve_path(dir, Path::new("/"))] {
            let name = munge_project_name(&path.to_string_lossy());
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for name in names {
        if name.len() <= 200 {
            out.push(resolve_path(&projects.join(&name), Path::new("/")));
            continue;
        }
        let prefix = format!("{}-", &name[..200]);
        let Ok(entries) = std::fs::read_dir(&projects) else { continue };
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&prefix) {
                out.push(resolve_path(&entry.path(), Path::new("/")));
            }
        }
    }
    out.dedup();
    out
}

fn munge_project_name(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else {
            out.extend(std::iter::repeat_n('-', c.len_utf16()));
        }
    }
    out
}

/// The deny message for a confined run, or `None` when the call may run.
///
/// File tools: the `file_path`/`path`/`notebook_path` argument (`~` expanded,
/// relative = relative to `cwd`) is resolved (canonicalized as far as it
/// exists, so a symlink is followed to where it points, `..` folded) and must
/// land inside the scratch dir, a harness dir or a temp dir (`/tmp`, the
/// system temp dir). `Glob`'s `pattern` is checked too when it is absolute.
///
/// Bash: see [`bash_offending`]. It is a guard rail for an agent that plays
/// along, not a sandbox: a path built at runtime passes. The deny tells the
/// agent why, so it can self-correct.
pub fn confinement_denial(tool: &str, input: &Value, cwd: &Path, confine: &Confinement) -> Option<String> {
    let check = Check::new(cwd, confine);
    let offending = if CONFINED_FILE_TOOLS.contains(&tool) {
        let mut hit = CONFINED_PATH_KEYS
            .iter()
            .filter_map(|key| input.get(*key).and_then(Value::as_str))
            .filter(|raw| !raw.trim().is_empty())
            .find_map(|raw| check.outside(raw.trim(), cwd, false));
        if hit.is_none() && tool == "Glob" {
            hit = input
                .get("pattern")
                .and_then(Value::as_str)
                .filter(|pattern| pattern.starts_with('/') || pattern.starts_with('~'))
                .and_then(|pattern| check.outside(glob_base(pattern), cwd, false));
        }
        hit
    } else if tool == "Bash" {
        let command = input.get("command").and_then(Value::as_str).unwrap_or_default();
        bash_offending(command, &check)
    } else {
        None
    }?;
    Some(format!(
        "This session is confined to its scratch folder {}; it cannot read or change files \
         elsewhere on this machine (`{offending}` is outside it). Work inside the scratch \
         folder or a temp dir instead.",
        confine.root.display()
    ))
}

struct Check {
    cwd: PathBuf,
    allowed: Vec<PathBuf>,
}

impl Check {
    fn new(cwd: &Path, confine: &Confinement) -> Self {
        let mut allowed = vec![resolve_path(confine.root, cwd)];
        allowed.extend(confine.harness.iter().map(|dir| resolve_path(dir, cwd)));
        Self { cwd: cwd.to_path_buf(), allowed }
    }

    /// `Some(raw)` when `raw` (relative = relative to `base`) lands outside.
    fn outside(&self, raw: &str, base: &Path, bash: bool) -> Option<String> {
        let resolved = self.resolve(raw, base);
        if self.allowed.iter().any(|root| within(&resolved, root)) || in_temp_dir(&resolved) {
            return None;
        }
        if bash && BASH_SYSTEM_PREFIXES.iter().any(|prefix| within(&resolved, Path::new(prefix))) {
            return None;
        }
        Some(raw.to_string())
    }

    fn resolve(&self, raw: &str, base: &Path) -> PathBuf {
        resolve_path(&expand_home(raw), base)
    }
}

// ---------------------------------------------------------------------------
// Bash
// ---------------------------------------------------------------------------

/// The first path a Bash command names outside the confinement, if any.
///
/// The command is lexed like a shell would (quotes, escapes, `;|&()`,
/// backticks, `$(…)`, redirections, here-docs, `$((…))`) into simple
/// commands; each word is split further on `=:,` and quotes so `--out=/x`,
/// `PATH=/a:/b` and `open('/x')` still show their paths. A word is checked
/// when it is absolute (`/…`, not `//…`), starts with `~`, names
/// `$HOME`/`$PWD`/`$OLDPWD`, climbs with `..`, carries a path on a flag
/// (`-o/x`), or names something that EXISTS relative to the cwd (a symlink
/// out is followed). An absolute word under a top-level directory nobody has
/// (`/api/x` in a URL) is not a path.
///
/// `cd`/`pushd` targets join the bases relative words resolve against, so
/// `cd /tmp && cat link/x` follows `/tmp/link`; a bare `cd` (home) is refused.
///
/// The root `/` (or a glob straight under it, `/ho*`) is only a path as an
/// operand of its own (`ls /`, `find "/" …`): not inside a larger word
/// (`IFS=/`, `-F/`, `'{print $1"/"$2}'`, `split('/')`), nor as the operand
/// of a text command (`tr / -`, `[ "$p" = / ]`), a pattern (`grep -c / f`),
/// a script (`sed`, `awk`; there a `/re/` is not a path either) or a
/// delimiter flag (`cut -d /`, `awk -F /`, `sort -t /`). Quoted text a shell
/// runs (`bash -c 'ls /'`, a here-doc fed to `bash`) is lexed again as a
/// command; any other quoted text and here-doc is data, still checked for
/// the paths it names.
fn bash_offending(command: &str, check: &Check) -> Option<String> {
    let mut bases = vec![check.cwd.clone()];
    bash_offending_in(command, check, &mut bases, 0)
}

fn bash_offending_in(command: &str, check: &Check, bases: &mut Vec<PathBuf>, depth: usize) -> Option<String> {
    for words in shell_commands(command) {
        if let Some(hit) = command_offending(&words, check, bases, depth) {
            return Some(hit);
        }
    }
    None
}

/// What an operand is to its command.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// A file operand, an option value, a redirection target.
    Plain,
    /// A regex (`grep PATTERN`): never a file.
    Pattern,
    /// A sed/awk script or the operand of a text command: paths inside it
    /// are checked, a bare `/` is not the root.
    Data,
    /// A delimiter (`cut -d /`).
    Delimiter,
    /// A `cd`/`pushd` target.
    Cd,
    /// Shell code (`bash -c '…'`).
    Shell,
}

fn command_offending(words: &[ShellWord], check: &Check, bases: &mut Vec<PathBuf>, depth: usize) -> Option<String> {
    // The command word: past assignments, redirections and wrappers.
    let mut index = 0;
    while let Some(word) = words.get(index) {
        let name = basename(&word.text);
        if word.redirect || is_assignment(word) || WRAPPERS.contains(&name) || (index > 0 && name.starts_with('-') && WRAPPERS.contains(&basename(&words[index - 1].text))) {
            index += 1;
        } else {
            break;
        }
    }
    let name = words.get(index).map(|word| basename(&word.text).to_string()).unwrap_or_default();
    let name = name.as_str();
    let mut roles = vec![Role::Plain; words.len()];
    let mut next: Option<Role> = None;
    let mut operand_taken = false;
    let mut cd_target = false;
    let mut options_done = false;
    for (at, word) in words.iter().enumerate().skip(index + 1) {
        if word.redirect {
            continue;
        }
        if let Some(role) = next.take() {
            roles[at] = role;
            continue;
        }
        let flag = !options_done && word.text.starts_with('-') && word.text.len() > 1 && !word.quoted;
        if word.text == "--" && !word.quoted {
            options_done = true;
            continue;
        }
        if PATTERN_COMMANDS.contains(&name) {
            if flag {
                match word.text.as_str() {
                    "-e" | "--regexp" => {
                        next = Some(Role::Pattern);
                        operand_taken = true;
                    }
                    "-f" | "--file" => operand_taken = true,
                    _ if word.text.starts_with("--regexp=") => {
                        roles[at] = Role::Pattern;
                        operand_taken = true;
                    }
                    _ => {}
                }
            } else if !operand_taken {
                roles[at] = Role::Pattern;
                operand_taken = true;
            }
        } else if SCRIPT_COMMANDS.contains(&name) {
            if flag {
                match word.text.as_str() {
                    "-e" | "--expression" => {
                        next = Some(Role::Data);
                        operand_taken = true;
                    }
                    "-f" | "--file" => operand_taken = true,
                    "-F" if name != "sed" => next = Some(Role::Delimiter),
                    _ => {}
                }
            } else if !operand_taken {
                roles[at] = Role::Data;
                operand_taken = true;
            }
        } else if TEXT_COMMANDS.contains(&name) {
            roles[at] = Role::Data;
        } else if SHELLS.contains(&name) {
            if word.quoted || name == "eval" {
                roles[at] = Role::Shell;
            }
        } else if name == "cd" || name == "pushd" {
            if !flag && !cd_target {
                roles[at] = Role::Cd;
                cd_target = true;
            }
        } else if flag && delimiter_flag(name, &word.text) {
            next = Some(Role::Delimiter);
        }
    }
    if (name == "cd" || name == "pushd") && !cd_target {
        // A bare `cd` goes home.
        return Some("~".to_string());
    }
    for (at, word) in words.iter().enumerate() {
        let hit = match roles[at] {
            Role::Pattern | Role::Delimiter => None,
            Role::Shell if depth < 4 => bash_offending_in(&word.text, check, bases, depth + 1),
            Role::Cd => {
                let target = if word.text == "-" { "~".to_string() } else { word.text.clone() };
                let hit = word_offending(&ShellWord { text: target.clone(), ..word.clone() }, Role::Plain, check, bases);
                if hit.is_none() {
                    let resolved: Vec<PathBuf> =
                        bases.iter().map(|base| check.resolve(&expand_vars(&target, &check.cwd), base)).collect();
                    for path in resolved {
                        if !bases.contains(&path) {
                            bases.push(path);
                        }
                    }
                }
                hit
            }
            role => word_offending(word, role, check, bases),
        };
        if hit.is_some() {
            return hit;
        }
    }
    None
}

/// `cut -d /`, `awk -F /`, `sort -t /` …: the next word is a delimiter.
fn delimiter_flag(command: &str, flag: &str) -> bool {
    match command {
        "cut" | "paste" => matches!(flag, "-d" | "--delimiter"),
        "sort" | "join" => matches!(flag, "-t" | "--field-separator"),
        "column" => matches!(flag, "-s" | "-o" | "--separator" | "--output-separator"),
        "xargs" => matches!(flag, "-d" | "--delimiter"),
        _ => false,
    }
}

fn word_offending(word: &ShellWord, role: Role, check: &Check, bases: &[PathBuf]) -> Option<String> {
    for part in word.text.split(|c: char| {
        c.is_whitespace() || matches!(c, '\'' | '"' | '`' | ';' | '|' | '&' | '(' | ')' | '<' | '>' | '=' | ':' | ',')
    }) {
        if part.is_empty() {
            continue;
        }
        let (raw, attached) = match part.strip_prefix('-') {
            Some(_) => match part.find('/') {
                Some(slash) => (&part[slash..], true),
                None => continue,
            },
            None => (part, false),
        };
        let expanded = expand_vars(raw, &check.cwd);
        let path = expanded.as_str();
        if path.starts_with('/') {
            if path.starts_with("//") {
                continue;
            }
            let literal = glob_base(path);
            let is_root = check.resolve(literal, &check.cwd) == Path::new("/");
            if is_root {
                // The root alone: a path only as an operand of its own.
                if role == Role::Plain && !attached && raw == word.text {
                    return Some(raw.to_string());
                }
                continue;
            }
            if expanded == raw && !top_level_exists(literal) {
                continue;
            }
            if role == Role::Data && path.ends_with('/') {
                // `sed '/usr/d'`, `awk '/home/ {…}'`: a regex, not a file.
                continue;
            }
            if check.outside(literal, &check.cwd, true).is_some() {
                return Some(raw.to_string());
            }
            continue;
        }
        if path.starts_with('~') {
            if check.outside(glob_base(path), &check.cwd, true).is_some() {
                return Some(raw.to_string());
            }
            continue;
        }
        if attached || path.contains('$') || path.contains("://") {
            continue;
        }
        let climbs =
            path == ".." || path.starts_with("../") || path.contains("/../") || path.ends_with("/..");
        let literal = glob_base(path);
        let first = literal.split('/').next().unwrap_or_default();
        for base in bases {
            let exists = climbs || (!first.is_empty() && base.join(first).symlink_metadata().is_ok());
            if exists && check.outside(literal, base, true).is_some() {
                return Some(raw.to_string());
            }
        }
    }
    None
}

/// `$HOME`/`${HOME}` → `~`, `$PWD` → the cwd, `$OLDPWD` → `~` (unknown, so
/// treated as outside).
fn expand_vars(raw: &str, cwd: &Path) -> String {
    let text = raw
        .replace("${HOME}", "$HOME")
        .replace("${PWD}", "$PWD")
        .replace("${OLDPWD}", "$OLDPWD");
    if let Some(rest) = text.strip_prefix("$HOME").or_else(|| text.strip_prefix("$OLDPWD")) {
        return format!("~{rest}");
    }
    if let Some(rest) = text.strip_prefix("$PWD") {
        return format!("{}{rest}", cwd.display());
    }
    if text.contains("$HOME") || text.contains("$OLDPWD") {
        return "~".to_string();
    }
    text
}

fn basename(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

/// `NAME=value` before the command word.
fn is_assignment(word: &ShellWord) -> bool {
    match word.text.split_once('=') {
        Some((name, _)) => {
            !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit())
        }
        None => false,
    }
}

/// One shell word: its text with the quotes removed, whether any of it was
/// quoted (quoted text is data unless a shell runs it), and whether it is a
/// redirection target.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ShellWord {
    pub text: String,
    pub quoted: bool,
    pub redirect: bool,
}

#[derive(Default)]
struct Lexer {
    commands: Vec<Vec<ShellWord>>,
    word: ShellWord,
    started: bool,
    redirect_next: bool,
    /// Pending here-docs: (delimiter, `<<-`, the command they feed).
    heredocs: Vec<(String, bool, usize)>,
}

impl Lexer {
    fn flush(&mut self) {
        if self.started {
            let mut word = std::mem::take(&mut self.word);
            word.redirect = std::mem::take(&mut self.redirect_next);
            self.commands.last_mut().expect("one command").push(word);
        }
        self.word = ShellWord::default();
        self.started = false;
    }

    fn data(&mut self, command: usize, text: String) {
        self.commands[command].push(ShellWord { text, quoted: true, redirect: false });
    }

    fn split(&mut self) {
        self.flush();
        self.redirect_next = false;
        if !self.commands.last().expect("one command").is_empty() {
            self.commands.push(Vec::new());
        }
    }
}

/// The simple commands of a command line (see [`bash_offending`]).
pub(crate) fn shell_commands(command: &str) -> Vec<Vec<ShellWord>> {
    let chars: Vec<char> = command.chars().collect();
    let mut lexer = Lexer { commands: vec![Vec::new()], ..Lexer::default() };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '\\' => {
                if let Some(escaped) = next.filter(|escaped| *escaped != '\n') {
                    lexer.word.text.push(escaped);
                    lexer.started = true;
                }
                i += 2;
                continue;
            }
            '\'' => {
                let end = (i + 1..chars.len()).find(|at| chars[*at] == '\'').unwrap_or(chars.len());
                lexer.word.text.extend(&chars[i + 1..end]);
                lexer.word.quoted = true;
                lexer.started = true;
                i = end + 1;
                continue;
            }
            '"' => {
                let mut at = i + 1;
                while at < chars.len() && chars[at] != '"' {
                    if chars[at] == '\\' && matches!(chars.get(at + 1), Some('"' | '\\' | '$' | '`')) {
                        at += 1;
                    }
                    lexer.word.text.push(chars[at]);
                    at += 1;
                }
                lexer.word.quoted = true;
                lexer.started = true;
                i = at + 1;
                continue;
            }
            '$' if next == Some('(') && chars.get(i + 2) == Some(&'(') => {
                // `$((…))` is arithmetic: data.
                let start = i + 3;
                let mut depth = 2usize;
                let mut at = start;
                while at < chars.len() {
                    match chars[at] {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    at += 1;
                }
                let end = at.saturating_sub(1).clamp(start.min(chars.len()), chars.len());
                lexer.word.text.extend(&chars[start.min(end)..end]);
                lexer.word.quoted = true;
                lexer.started = true;
                i = at + 1;
                continue;
            }
            '$' if next == Some('(') => {
                lexer.split();
                i += 2;
                continue;
            }
            '<' if next == Some('<') && chars.get(i + 2) != Some(&'<') => {
                lexer.flush();
                let mut at = i + 2;
                let strip = chars.get(at) == Some(&'-');
                if strip {
                    at += 1;
                }
                while at < chars.len() && matches!(chars[at], ' ' | '\t') {
                    at += 1;
                }
                let mut delimiter = String::new();
                while at < chars.len() && !chars[at].is_whitespace() && !matches!(chars[at], ';' | '|' | '&' | '<' | '>' | '(' | ')') {
                    if !matches!(chars[at], '\'' | '"' | '\\') {
                        delimiter.push(chars[at]);
                    }
                    at += 1;
                }
                let command = lexer.commands.len() - 1;
                lexer.heredocs.push((delimiter, strip, command));
                i = at;
                continue;
            }
            '<' | '>' => {
                lexer.flush();
                lexer.redirect_next = true;
                i += 1;
                while i < chars.len() && matches!(chars[i], '<' | '>' | '&' | '|') {
                    i += 1;
                }
                continue;
            }
            '\n' => {
                lexer.split();
                i += 1;
                for (delimiter, strip, command) in std::mem::take(&mut lexer.heredocs) {
                    let mut body = String::new();
                    while i < chars.len() {
                        let end = (i..chars.len()).find(|at| chars[*at] == '\n').unwrap_or(chars.len());
                        let line: String = chars[i..end].iter().collect();
                        i = end + 1;
                        let line_text = if strip { line.trim_start_matches('\t') } else { line.as_str() };
                        if line_text == delimiter {
                            break;
                        }
                        body.push_str(&line);
                        body.push('\n');
                    }
                    lexer.data(command, body);
                }
                continue;
            }
            ';' | '|' | '&' | '(' | ')' | '`' => {
                lexer.split();
                i += 1;
                continue;
            }
            '#' if !lexer.started => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            c if c.is_whitespace() => lexer.flush(),
            c => {
                lexer.word.text.push(c);
                lexer.started = true;
            }
        }
        i += 1;
    }
    lexer.flush();
    lexer.commands.into_iter().filter(|words| !words.is_empty()).collect()
}

// ---------------------------------------------------------------------------
// paths
// ---------------------------------------------------------------------------

/// `~` / `~/x` → the home dir. `~user` is refused as unresolvable (it maps to
/// `/`, outside by construction).
fn expand_home(raw: &str) -> PathBuf {
    if raw == "~" || raw.starts_with("~/") {
        let home = home_dir().unwrap_or_else(|| PathBuf::from("/"));
        return match raw.strip_prefix("~/") {
            Some(rest) => home.join(rest),
            None => home,
        };
    }
    if raw.starts_with('~') {
        return PathBuf::from("/");
    }
    PathBuf::from(raw)
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// Absolute, `..`-free, symlinks resolved as far as the path exists.
fn resolve_path(path: &Path, cwd: &Path) -> PathBuf {
    let absolute = if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) };
    let mut lexical = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::ParentDir => {
                lexical.pop();
            }
            std::path::Component::CurDir => {}
            other => lexical.push(other.as_os_str()),
        }
    }
    // Canonicalize the longest existing prefix; re-append the rest.
    let mut existing = lexical.clone();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(canonical) = std::fs::canonicalize(&existing) {
            let mut out = canonical;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (existing.file_name().map(|name| name.to_os_string()), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                existing = parent.to_path_buf();
            }
            _ => return lexical,
        }
    }
}

fn within(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
}

fn in_temp_dir(path: &Path) -> bool {
    let mut roots = vec![PathBuf::from("/tmp"), PathBuf::from("/private/tmp")];
    let temp = std::env::temp_dir();
    roots.push(resolve_path(&temp, Path::new("/")));
    roots.push(temp);
    roots.iter().any(|root| root != Path::new("/") && within(path, root))
}

/// `/a/b/**/*.rs` → `/a/b`: the literal directory a glob pattern starts in.
pub(crate) fn glob_base(pattern: &str) -> &str {
    let Some(cut) = pattern.find(['*', '?', '[', '{']) else { return pattern };
    match pattern[..cut].rfind('/') {
        Some(0) => "/",
        Some(index) => &pattern[..index],
        None => "",
    }
}

/// Whether `/<first>` exists: an absolute-looking word under a top-level
/// directory nobody has is a pattern or a URL path, not a file.
fn top_level_exists(word: &str) -> bool {
    let first = word.trim_start_matches('/').split('/').next().unwrap_or_default();
    first.is_empty() || Path::new("/").join(first).exists()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn scratch() -> PathBuf {
        PathBuf::from("/nonexistent-exp-scratch/scratch/root/session")
    }

    fn denial_in(tool: &str, input: Value, dir: &Path, harness: &[PathBuf]) -> Option<String> {
        confinement_denial(tool, &input, dir, &Confinement { root: dir, harness })
    }

    fn denied(tool: &str, input: Value) -> bool {
        denial_in(tool, input, &scratch(), &[]).is_some()
    }

    fn bash(command: &str) -> bool {
        denied("Bash", json!({ "command": command }))
    }

    fn home() -> String {
        home_dir().expect("HOME").display().to_string()
    }

    /// A fresh dir under the temp dir (the temp dirs are open, so tests that
    /// need an outside target point a symlink at `$HOME`).
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("exp-confine-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_confined_file_tool_inside_the_scratch_dir_is_allowed() {
        let dir = scratch();
        assert!(!denied("Read", json!({ "file_path": dir.join("notes.md").display().to_string() })));
        assert!(!denied("Write", json!({ "file_path": "out/report.md", "content": "x" })));
        assert!(!denied("Edit", json!({ "file_path": "./a.txt" })));
        assert!(!denied("Glob", json!({ "pattern": "**/*.md" })));
        assert!(!denied("Grep", json!({ "pattern": "todo" })));
        assert!(!denied("LS", json!({ "path": dir.display().to_string() })));
        // Not a file tool at all.
        assert!(!denied("WebFetch", json!({ "url": "https://example.com/Users/x" })));
    }

    #[test]
    fn a_confined_file_tool_outside_the_scratch_dir_is_denied() {
        assert!(denied("Read", json!({ "file_path": "/Users/someone/repo/src/lib.rs" })));
        assert!(denied("Edit", json!({ "file_path": "/home/someone/repo/src/lib.rs" })));
        assert!(denied("NotebookEdit", json!({ "notebook_path": "~/nb.ipynb" })));
        assert!(denied("Grep", json!({ "pattern": "x", "path": "/" })));
        assert!(denied("Glob", json!({ "pattern": "/Users/someone/**/*.rs" })));
        let dir = scratch();
        let message = denial_in("Read", json!({ "file_path": "/Users/a/b" }), &dir, &[]).expect("denied");
        assert!(message.contains("confined to its scratch folder"));
        assert!(message.contains(&dir.display().to_string()));
    }

    #[test]
    fn a_relative_path_climbing_out_of_the_scratch_dir_is_denied() {
        assert!(denied("Read", json!({ "file_path": "../../other/file.rs" })));
        assert!(denied("LS", json!({ "path": ".." })));
        // Climbing and coming back in stays inside.
        assert!(!denied("Read", json!({ "file_path": "../session/notes.md" })));
    }

    #[test]
    fn bash_naming_an_absolute_path_outside_is_denied() {
        let home = home();
        assert!(bash(&format!("ls {home}/code")));
        assert!(bash("find / -name exponential -type d"));
        assert!(bash("cat ~/code/repo/README.md"));
        assert!(bash("cd $HOME && git status"));
        assert!(bash("grep -r x \"${HOME}/code\""));
        assert!(bash("cat ../../secrets"));
        // Inside, system tools and non-path slashes pass.
        assert!(!bash("ls -la && echo hi > out.txt"));
        assert!(!bash("/usr/bin/env python3 x.py 2>/dev/null"));
        assert!(!bash("curl -s https://example.com/api/x"));
        assert!(!bash("grep -rn \"/nonexistent-top/route\" ."));
    }

    /// N-5: the escapes the first heuristic let through.
    #[test]
    fn bash_escapes_through_flags_globs_vars_and_shells_are_denied() {
        let home = home();
        // A path glued to a flag, a glob straight under the root.
        assert!(bash(&format!("tar -cf out.tar -C{home} .")));
        assert!(bash("cat /ho*/someone/repo/README.md"));
        assert!(bash("ls /*"));
        // `$PWD/..`, `$OLDPWD`.
        assert!(bash("cat $PWD/../../other/file"));
        assert!(bash("cd \"$OLDPWD\""));
        // A quoted root operand, in a redirection, in an assignment.
        assert!(bash("find \"/\" -name exponential"));
        assert!(bash(&format!("echo hi > {home}/x")));
        assert!(bash(&format!("REPO={home}/code make")));
        // Shell code inside quotes and here-docs is lexed again.
        assert!(bash("bash -c 'cd / && ls'"));
        assert!(bash("sh -c \"ls /\""));
        assert!(bash("bash <<'EOF'\nls /\nEOF"));
        assert!(bash(&format!("echo $(cat {home}/x)")));
        assert!(bash(&format!("cat `ls {home}`")));
    }

    #[test]
    fn a_cd_in_a_compound_command_is_followed() {
        // A bare `cd` (and `cd -`) goes home.
        assert!(bash("cd && cat .bashrc"));
        assert!(bash("cd; ls"));
        assert!(bash("cd -"));
        assert!(bash("cd ~ && ls"));
        // `cd` into a system dir, then climb out of it.
        assert!(bash("cd /usr && cat ../home/someone/x"));
        assert!(bash("cd /usr/share; cd ../..; ls home"));
        // Inside stays allowed.
        assert!(!bash("cd sub && ls"));
        assert!(!bash("cd /tmp && ls && cd -- /usr/share && ls"));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_the_scratch_dir_is_followed() {
        let dir = temp("symlink");
        let scratch = dir.join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        let home = home_dir().expect("HOME");
        std::os::unix::fs::symlink(&home, scratch.join("out")).unwrap();
        std::os::unix::fs::symlink(&home, dir.join("hop")).unwrap();
        let denied = |tool: &str, input: Value| denial_in(tool, input, &scratch, &[]).is_some();
        // File tools follow the link.
        assert!(denied("Read", json!({ "file_path": "out/.bashrc" })));
        assert!(denied("LS", json!({ "path": scratch.join("out").display().to_string() })));
        // Bash: a relative word naming the link, a `cd` through it.
        assert!(denied("Bash", json!({ "command": "cat out/.bashrc" })));
        assert!(denied("Bash", json!({ "command": "ls out" })));
        assert!(denied("Bash", json!({ "command": "cd out && cat .bashrc" })));
        // A link in a temp dir the command `cd`s into.
        let command = format!("cd {} && cat hop/.bashrc", dir.display());
        assert!(denied("Bash", json!({ "command": command })));
        // A link that stays inside is fine.
        std::fs::create_dir_all(scratch.join("real")).unwrap();
        std::os::unix::fs::symlink(scratch.join("real"), scratch.join("alias")).unwrap();
        assert!(!denied("Bash", json!({ "command": "ls alias && cat alias/x" })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The root as data, not a path: today's misfires (`cut -d/`, `awk -F/`).
    #[test]
    fn a_bare_slash_in_a_pattern_regex_or_delimiter_is_not_the_root() {
        for command in [
            "git log --format=%an | cut -d/ -f2",
            "echo a/b | cut -d '/' -f1",
            "echo a/b | cut -d / -f1",
            "awk -F/ '{print $NF}' list.txt",
            "awk -F'/' '{print $1}' list.txt",
            "awk -F / '{print $1}' list.txt",
            "awk '{print $1\"/\"$2}' list.txt",
            "awk '/home/ {print}' list.txt",
            "sed 's/\\//-/g' list.txt",
            "sed -e 's|/|-|g' list.txt",
            "sed '/usr/d' list.txt",
            "grep -c / list.txt",
            "grep -rn '/' .",
            "grep -e / list.txt",
            "rg '^/' .",
            "tr / - < list.txt",
            "sort -t / -k2 list.txt",
            "IFS=/ read -r a b <<< \"$x\"",
            "printf '%s/%s\\n' a b",
            "echo $((10 / 2))",
            "[ \"$p\" = / ] && echo root",
            "python3 -c \"print('/'.join(['a', 'b']))\"",
            "python3 - <<'EOF'\nprint(4 / 2)\nEOF",
            "jq -r '.a / .b' data.json",
        ] {
            assert!(!bash(command), "{command}");
        }
        // The same commands still check real paths.
        assert!(bash(&format!("awk -F/ '{{print $1}}' {}/list.txt", home())));
        assert!(bash(&format!("grep -c / {}/list.txt", home())));
        assert!(bash(&format!("sed -i 's/a/b/' {}/list.txt", home())));
    }

    #[test]
    fn the_temp_dirs_stay_allowed_in_a_confined_run() {
        assert!(!denied("Read", json!({ "file_path": "/tmp/x.txt" })));
        assert!(!denied("Write", json!({ "file_path": "/tmp/sub/y.txt" })));
        assert!(!bash("echo hi > /tmp/x && cat /tmp/x"));
        let temp = std::env::temp_dir().join("exp-confine-test.txt");
        assert!(!denied("Read", json!({ "file_path": temp.display().to_string() })));
    }

    /// FEED-142: the run's own claude project dir (memory, tool results) is
    /// open; a sibling project's is not.
    #[test]
    fn the_runs_own_memory_and_tool_results_dirs_are_allowed() {
        let config = PathBuf::from(format!("{}/.nonexistent-exp-config/agents/claude/0e38a3e4", home()));
        let dir = scratch();
        let harness = claude_harness_dirs(Some(&config), &[&dir]);
        let project = config.join("projects/-nonexistent-exp-scratch-scratch-root-session");
        assert_eq!(harness, vec![project.clone()]);
        let memory = project.join("memory/MEMORY.md").display().to_string();
        let results = project.join("6f1c/tool-results/toolu_01.txt").display().to_string();
        let allowed = |tool: &str, input: Value| denial_in(tool, input, &dir, &harness).is_none();
        assert!(allowed("Read", json!({ "file_path": memory })));
        assert!(allowed("Write", json!({ "file_path": project.join("memory/coordinator.md"), "content": "x" })));
        assert!(allowed("Edit", json!({ "file_path": memory })));
        assert!(allowed("Read", json!({ "file_path": results })));
        assert!(allowed("Bash", json!({ "command": format!("cat {results} | head -50") })));
        assert!(allowed("Bash", json!({ "command": format!("ls {}/memory/", project.display()) })));
        // A sibling project, the config root, another account: still outside.
        let sibling = config.join("projects/-home-someone-repo/memory/MEMORY.md");
        assert!(!allowed("Read", json!({ "file_path": sibling })));
        assert!(!allowed("Read", json!({ "file_path": config.join(".claude.json") })));
        assert!(!allowed("Bash", json!({ "command": format!("ls {}/projects", config.display()) })));
        // No harness without a config dir.
        assert!(claude_harness_dirs(None, &[&dir]).is_empty());
    }

    /// FEED-142: past 200 chars claude caps the project dir name and appends
    /// a hash, so the long name matches on its prefix.
    #[test]
    fn a_long_scratch_dir_matches_its_hash_suffixed_project_dir() {
        let base = temp("harness");
        let config = base.join("config");
        let dir = PathBuf::from(format!("/nonexistent-exp-scratch/{}/root/session", "x".repeat(220)));
        let munged = munge_project_name(&dir.display().to_string());
        assert!(munged.len() > 200);
        let capped = config.join("projects").join(format!("{}-71h4ur", &munged[..200]));
        std::fs::create_dir_all(capped.join("memory")).unwrap();
        std::fs::create_dir_all(config.join("projects/-other/memory")).unwrap();
        let harness = claude_harness_dirs(Some(&config), &[&dir]);
        assert_eq!(harness, vec![resolve_path(&capped, Path::new("/"))]);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_project_name_munges_like_claude() {
        assert_eq!(munge_project_name("/home/a/.local/x_y"), "-home-a--local-x-y");
        // One `-` per UTF-16 unit, like the JS regex.
        assert_eq!(munge_project_name("/ä😀"), "----");
    }

    #[test]
    fn the_lexer_splits_simple_commands_and_keeps_quotes() {
        let commands = shell_commands("cd a && cat 'b c' > out; echo \"x\\\"y\" | wc # note\nls");
        let texts: Vec<Vec<&str>> =
            commands.iter().map(|words| words.iter().map(|word| word.text.as_str()).collect()).collect();
        assert_eq!(texts, vec![vec!["cd", "a"], vec!["cat", "b c", "out"], vec!["echo", "x\"y"], vec!["wc"], vec!["ls"]]);
        assert!(commands[1][1].quoted);
        assert!(commands[1][2].redirect);
    }

    #[test]
    fn the_glob_base_is_the_literal_directory() {
        assert_eq!(glob_base("/a/b/**/*.rs"), "/a/b");
        assert_eq!(glob_base("/*"), "/");
        assert_eq!(glob_base("/a/b/c.rs"), "/a/b/c.rs");
        assert_eq!(glob_base("*.rs"), "");
        assert_eq!(glob_base("/tmp"), "/tmp");
        assert_eq!(glob_base("~/a*"), "~");
    }
}
