//! N-5 / NEW-5: confinement of a repo-less claude run to its scratch dir.
//!
//! The run's `can_use_tool` answer and its `PreToolUse` hook (which fires
//! even under `bypassPermissions`) both ask [`confinement_denial`]. A file
//! tool may touch the scratch dir, a temp dir and the run's own claude state
//! ([`claude_harness_dirs`], FEED-142); a Bash command is scanned for the
//! paths it names ([`bash_offending`]).
//!
//! What this guarantees, and what it does not:
//!
//! - File tools (`Read`, `Edit`, `Write`, `Glob`, `Grep`, `LS`, notebooks):
//!   the path argument, resolved through symlinks and `..`, lands inside the
//!   scratch dir, the run's own claude project dir or a temp dir. A `Glob`
//!   pattern may not climb out with `..`.
//! - Bash: every path the command line spells out statically (literal,
//!   `~`, `$HOME`/`$PWD`/`$TMPDIR`, a variable assigned a literal earlier in
//!   the same command, a brace expansion, a `cd` chain, a symlink that exists
//!   now) is checked, including inside `$(…)`, backticks, `bash -c '…'` and
//!   here-docs fed to a shell. A word the scanner cannot resolve FAILS
//!   CLOSED: a `${…}` modifier on a variable that could name a path, a
//!   `cd` into a computed dir, a path built on a command substitution or an
//!   untracked variable (`$(…)/x`, `$dir/x`), a command substitution used as
//!   a file operand, unparseable quoting. The agent is told why.
//! - NOT guaranteed: code an interpreter runs (`python -c`, `node -e`,
//!   `perl`, a script file, awk's `system()` beyond the paths it spells out)
//!   builds paths at runtime no hook can see; neither can a file's contents
//!   fed to `xargs`, nor an untracked variable used whole (`cat "$f"`). This
//!   is a guard rail that stops an agent wandering out, not an OS sandbox.

use std::collections::HashMap;
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
/// The file tools that ALWAYS name their file: one of these with none of
/// [`CONFINED_PATH_KEYS`] is a payload the guard could not read, refused
/// (`Glob`/`Grep` legitimately default to the cwd).
const PATH_REQUIRED_TOOLS: [&str; 7] =
    ["Read", "Edit", "Write", "MultiEdit", "NotebookEdit", "NotebookRead", "LS"];

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

/// Under [`BASH_SYSTEM_PREFIXES`] but NOT system: macOS mounts every volume
/// below `/System/Volumes`, and the Data volume holds `/Users`, `/home` (a
/// symlink to `/System/Volumes/Data/home`) and `/private` again, so
/// `/{home,x}/…` resolving through the link must not ride the `/System` pass.
const BASH_SYSTEM_EXCLUDED: [&str; 1] = ["/System/Volumes"];

/// A command that only passes its wrapped command through: the options that
/// take a separate value, the options whose value is shell code, and how
/// many operands come before the wrapped command.
struct Wrapper {
    name: &'static str,
    valued: &'static [&'static str],
    shell: &'static [&'static str],
    operands: usize,
}

const fn wrapper(name: &'static str, valued: &'static [&'static str], shell: &'static [&'static str], operands: usize) -> Wrapper {
    Wrapper { name, valued, shell, operands }
}

static WRAPPERS: [Wrapper; 18] = [
    wrapper("sudo", &["-u", "-g", "-h", "-p", "-C", "-D", "-r", "-t", "-U", "--user", "--group", "--host", "--prompt", "--chdir"], &[], 0),
    wrapper("doas", &["-u", "-C"], &[], 0),
    wrapper("env", &["-u", "-C", "--unset", "--chdir"], &["-S", "--split-string"], 0),
    wrapper("command", &[], &[], 0),
    wrapper("exec", &["-a"], &[], 0),
    wrapper("time", &["-f", "-o", "--format", "--output"], &[], 0),
    wrapper("nohup", &[], &[], 0),
    wrapper("nice", &["-n", "--adjustment"], &[], 0),
    wrapper("builtin", &[], &[], 0),
    wrapper("timeout", &["-s", "-k", "--signal", "--kill-after"], &[], 1),
    wrapper(
        "xargs",
        &["-a", "-d", "-E", "-I", "-L", "-n", "-P", "-s", "--arg-file", "--delimiter", "--max-args", "--max-procs", "--max-chars", "--max-lines", "--process-slot-var"],
        &[],
        0,
    ),
    wrapper("ionice", &["-c", "-n", "-p", "-P", "-u", "--class", "--classdata", "--pid", "--pgid", "--uid"], &[], 0),
    wrapper("stdbuf", &["-i", "-o", "-e", "--input", "--output", "--error"], &[], 0),
    wrapper("flock", &["-w", "-E", "--timeout", "--wait", "--conflict-exit-code"], &["-c", "--command"], 1),
    wrapper("watch", &["-n", "--interval"], &[], 0),
    wrapper("script", &["-E", "-I", "-O", "-T", "-B", "--echo", "--log-in", "--log-out", "--log-io", "--log-timing"], &["-c", "--command"], 0),
    wrapper("su", &["-g", "-G", "-s", "-w", "--group", "--supp-group", "--shell", "--whitelist-environment"], &["-c", "--command"], 0),
    wrapper("chrt", &["-p"], &[], 1),
];
/// Shell keywords that stand before a command word.
const RESERVED: [&str; 15] =
    ["if", "then", "else", "elif", "fi", "do", "done", "while", "until", "!", "{", "}", "esac", "function", "coproc"];
/// Commands whose quoted arguments are themselves shell code (`watch` joins
/// its arguments into a `sh -c`, `su` hands them to the user's shell).
const SHELLS: [&str; 9] = ["bash", "sh", "zsh", "dash", "ksh", "fish", "eval", "watch", "su"];
/// Commands whose first operand is a regex, never a file.
const PATTERN_COMMANDS: [&str; 6] = ["grep", "egrep", "fgrep", "rg", "ag", "ack"];
/// Commands whose first operand is a script (`/re/`, `s/a/b/`).
const SCRIPT_COMMANDS: [&str; 5] = ["sed", "awk", "gawk", "mawk", "nawk"];
/// Commands whose operands are text.
const TEXT_COMMANDS: [&str; 8] = ["echo", "printf", "tr", "expr", "let", "test", "[", "[["];
/// Commands whose operands may be assignments.
const DECLARE_COMMANDS: [&str; 5] = ["export", "local", "declare", "typeset", "readonly"];

/// The lexer's marks inside a word: a `$` the shell expands (`$x`, `${x}`),
/// the output of a command substitution, and an untracked variable's value.
const EXPAND: char = '\u{E000}';
const UNKNOWN: char = '\u{E001}';
const OPAQUE: char = '\u{E002}';

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
/// claude keeps the first 200 and appends `-<hash>` (EXP-761), computed here
/// EXACTLY ([`capped_project_name`]): a sibling cwd sharing the first 200
/// chars has another hash, so its memory dir never opens.
pub fn claude_harness_dirs(config: Option<&Path>, dirs: &[&Path]) -> Vec<PathBuf> {
    let Some(projects) = config.map(|config| config.join("projects")) else { return Vec::new() };
    let mut names: Vec<String> = Vec::new();
    for dir in dirs {
        for path in [dir.to_path_buf(), resolve_path(dir, Path::new("/"))] {
            let name = capped_project_name(&path.to_string_lossy());
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    let mut out: Vec<PathBuf> =
        names.iter().map(|name| resolve_path(&projects.join(name), Path::new("/"))).collect();
    out.dedup();
    out
}

/// The 200-char cap: the munged name is kept as is up to 200 chars; past
/// that claude (2.1.308, `J0`) writes the first 200 plus
/// `-<base36(|hashCode(path)|)>`, the hash being Java's `String.hashCode`
/// (`h = 31·h + unit`, wrapping i32, over the UTF-16 units of the ORIGINAL
/// path), `Math.abs` taken as a JS number (so `i32::MIN` is 2³¹).
fn capped_project_name(path: &str) -> String {
    const CAP: usize = 200;
    let name = munge_project_name(path);
    if name.len() <= CAP {
        return name;
    }
    let mut hash: i32 = 0;
    for unit in path.encode_utf16() {
        hash = hash.wrapping_shl(5).wrapping_sub(hash).wrapping_add(i32::from(unit));
    }
    let mut rest = (i64::from(hash)).unsigned_abs();
    let mut digits: Vec<u8> = Vec::new();
    loop {
        let digit = (rest % 36) as u8;
        digits.push(if digit < 10 { b'0' + digit } else { b'a' + digit - 10 });
        rest /= 36;
        if rest == 0 {
            break;
        }
    }
    digits.reverse();
    // The munged name is ASCII, so the slice lands on a char boundary.
    format!("{}-{}", &name[..CAP], String::from_utf8(digits).expect("base36 digits are ASCII"))
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
/// system temp dir). `Glob`'s `pattern` is checked too, relative to its
/// `path`; a `..` after a wildcard is refused.
///
/// Bash: see [`bash_offending`] and the module docs for what is and is not
/// guaranteed. The deny tells the agent why, so it can self-correct.
pub fn confinement_denial(tool: &str, input: &Value, cwd: &Path, confine: &Confinement) -> Option<String> {
    let check = Check::new(cwd, confine);
    let offense = if CONFINED_FILE_TOOLS.contains(&tool) {
        let paths: Vec<&str> = CONFINED_PATH_KEYS
            .iter()
            .filter_map(|key| input.get(*key).and_then(Value::as_str))
            .map(str::trim)
            .filter(|raw| !raw.is_empty())
            .collect();
        // Fail closed: a tool that always names its file carrying no path
        // (a payload the hook could not read in full) is refused, never
        // waved through as "nothing to check".
        if paths.is_empty() && PATH_REQUIRED_TOOLS.contains(&tool) {
            return Some(no_path_denial(confine));
        }
        let mut hit = paths.iter().find_map(|raw| check.outside(raw, cwd, false));
        if hit.is_none() && tool == "Glob" {
            let base = match input.get("path").and_then(Value::as_str).map(str::trim).filter(|path| !path.is_empty()) {
                Some(path) => check.resolve(path, cwd),
                None => cwd.to_path_buf(),
            };
            hit = input.get("pattern").and_then(Value::as_str).and_then(|pattern| match literal_base(pattern) {
                Some(literal) => check.outside(literal, &base, false).map(|_| pattern.to_string()),
                None => Some(pattern.to_string()),
            });
        }
        hit.map(Offense::Outside)
    } else if tool == "Bash" {
        let command = input.get("command").and_then(Value::as_str).unwrap_or_default();
        bash_offending(command, &check)
    } else {
        None
    }?;
    let root = confine.root.display();
    Some(match offense {
        Offense::Outside(path) => format!(
            "This session is confined to its scratch folder {root}; it cannot read or change files \
             elsewhere on this machine (`{path}` is outside it). Work inside the scratch \
             folder or a temp dir instead."
        ),
        Offense::Unresolved(what) => format!(
            "This session is confined to its scratch folder {root}; the guard cannot check {what} \
             statically (a `${{…}}` modifier, a command substitution or an untracked variable used \
             as a path, a `cd` into a computed dir, or unparseable quoting), so the command is \
             refused. Spell the path out literally, inside the scratch folder or a temp dir."
        ),
    })
}

/// The deny for a file tool that names no path at all ([`PATH_REQUIRED_TOOLS`]).
fn no_path_denial(confine: &Confinement) -> String {
    let root = confine.root.display();
    format!(
        "This session is confined to its scratch folder {root}; the call names no file path \
         (`file_path`, `path` or `notebook_path`), so the guard cannot check it and refuses it. \
         Name a path inside the scratch folder or a temp dir."
    )
}

/// The deny for a hook payload the guard could not read (unparseable, or
/// no tool name): the hook FAILS CLOSED rather than waving the call through.
pub fn unreadable_call_denial(root: &Path) -> String {
    let root = root.display();
    format!(
        "This session is confined to its scratch folder {root}; the guard could not read this \
         tool call, so it is refused. Retry the call with its full arguments."
    )
}

/// Why a call is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Offense {
    /// A path outside the confinement.
    Outside(String),
    /// Something the scanner cannot resolve: it fails closed.
    Unresolved(String),
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
        if bash
            && BASH_SYSTEM_PREFIXES.iter().any(|prefix| within(&resolved, Path::new(prefix)))
            && !BASH_SYSTEM_EXCLUDED.iter().any(|prefix| within(&resolved, Path::new(prefix)))
        {
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

/// The first path a Bash command names outside the confinement, or the first
/// word it cannot resolve.
///
/// The command is lexed like a shell would (quotes, escapes, `;|&()`,
/// `$(…)` and backticks, also inside double quotes and here-docs, `$((…))`,
/// redirections, here-docs) into simple commands. Each word has its
/// variables resolved (`$HOME`, `$PWD`, `$TMPDIR`, ones assigned earlier in
/// the command), its braces expanded (`/{a,b}`), and is split further on
/// `=:,` and quotes so `--out=/x`, `PATH=/a:/b` and `open('/x')` still show
/// their paths. A part is checked when it is absolute (`//x` = `/x`), starts
/// with `~`, climbs with `..`, carries a path on a flag (`-o/x`), or names
/// something that EXISTS relative to the cwd (a symlink out is followed). An
/// absolute part under a top-level directory nobody has (`/api/x` in a URL)
/// is not a path.
///
/// `cd`/`pushd` targets join the bases relative words resolve against, so
/// `cd /tmp && cat link/x` follows `/tmp/link`; a bare `cd` (home) and a
/// `cd` into a dir the scanner cannot compute are refused.
///
/// The root `/` (or a glob straight under it, `/ho*`) is only a path as an
/// operand of its own (`ls /`, `find "/" …`): not inside a larger word
/// (`IFS=/`, `-F/`, `'{print $1"/"$2}'`, `split('/')`), nor as the operand
/// of a text command (`tr / -`, `[ "$p" = / ]`), a pattern (`grep -c / f`),
/// a script (`sed`, `awk`; there a `/re/` opening the script is not a path
/// either) or a delimiter flag (`cut -d /`, `awk -F /`, `sort -t /`). A
/// pattern still may not name `~`, `$HOME` or a real absolute path. Quoted
/// text after a shell name (`timeout 9 bash -c 'ls /'`, a here-doc fed to
/// `bash`) is lexed again as a command; any other quoted text and here-doc
/// is data, still checked for the paths it names. A text command piped into
/// `xargs` passes its operands on as arguments, so they count as paths.
fn bash_offending(command: &str, check: &Check) -> Option<Offense> {
    let mut scan = Scan { check, bases: vec![check.cwd.clone()], pwd: check.cwd.clone(), vars: HashMap::new() };
    scan.run(command, 0)
}

/// What an operand is to its command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// A file operand, an option value, a redirection target.
    Plain,
    /// A regex (`grep PATTERN`): absolute paths and `~` are checked, the
    /// bare root and relative words are not.
    Pattern,
    /// A sed/awk script: paths inside it are checked, a bare `/` is not the
    /// root, a `/re/` opening it is not a path.
    Script,
    /// The operand of a text command (`echo`, `tr`, `[`): paths inside it
    /// are checked, a bare `/` is not the root.
    Text,
    /// A delimiter (`cut -d /`).
    Delimiter,
    /// A `cd`/`pushd` target.
    Cd,
    /// Shell code (`bash -c '…'`).
    Shell,
}

/// How a grep/sed/awk flag affects the operand that follows.
enum FlagEffect {
    /// The next word has this role (and, for a pattern or script, the
    /// pattern is given).
    Next(Role, bool),
    /// This word itself carries the pattern or script (`-epat`).
    Attached(Role),
    /// The pattern is given elsewhere (`--files`, `-f file`).
    Given,
    None,
}

/// The state a command line carries from one simple command to the next.
struct Scan<'a> {
    check: &'a Check,
    /// Every dir relative words may resolve against (the cwd plus each `cd`).
    bases: Vec<PathBuf>,
    /// `$PWD`: the last `cd`.
    pwd: PathBuf,
    /// Variables assigned in this command line: `None` = an unknown value
    /// (a command substitution, `read`).
    vars: HashMap<String, Option<String>>,
}

impl Scan<'_> {
    fn run(&mut self, command: &str, depth: usize) -> Option<Offense> {
        let commands = match lex(command, 0) {
            Ok(commands) => commands,
            Err(why) => return Some(Offense::Unresolved(why)),
        };
        for (at, command) in commands.iter().enumerate() {
            let feeds_args = command.feeds_next
                && commands[at + 1..].iter().find(|next| !next.nested).is_some_and(|next| {
                    next.words.iter().any(|word| matches!(basename(&word.text), "xargs" | "parallel"))
                });
            if let Some(hit) = self.command_offending(&command.words, feeds_args, depth) {
                return Some(hit);
            }
        }
        None
    }

    fn command_offending(&mut self, words: &[ShellWord], feeds_args: bool, depth: usize) -> Option<Offense> {
        let mut roles = vec![Role::Plain; words.len()];
        // The command word: past assignments, redirections, keywords and
        // wrappers with their options.
        let mut index = 0;
        let mut wrapper: Option<&Wrapper> = None;
        let mut operands = 0;
        let mut valued = false;
        while let Some(word) = words.get(index) {
            if std::mem::take(&mut valued) {
                index += 1;
                continue;
            }
            if word.redirect || is_assignment(word) || (!word.quoted && RESERVED.contains(&word.text.as_str())) {
                index += 1;
                continue;
            }
            if let Some(found) = WRAPPERS.iter().find(|wrapper| wrapper.name == basename(&word.text)) {
                wrapper = Some(found);
                operands = found.operands;
                index += 1;
                continue;
            }
            if let Some(current) = wrapper {
                if is_flag(word) {
                    if current.shell.contains(&word.text.as_str()) {
                        if let Some(role) = roles.get_mut(index + 1) {
                            *role = Role::Shell;
                        }
                        valued = true;
                    } else {
                        valued = current.valued.contains(&word.text.as_str());
                    }
                    index += 1;
                    continue;
                }
                if operands > 0 {
                    operands -= 1;
                    index += 1;
                    continue;
                }
            }
            break;
        }
        if let Some(word) = words.get(index) {
            // `watch -n 1 'ls /'`: the "command" is shell code.
            if words[..index].iter().any(|word| SHELLS.contains(&basename(&word.text)))
                && (word.quoted || word.text.contains(char::is_whitespace))
            {
                roles[index] = Role::Shell;
            }
        }
        let name = words.get(index).map(|word| basename(&word.text).to_string()).unwrap_or_default();
        let name = name.as_str();
        let mut next: Option<Role> = None;
        let mut given = false;
        let mut cd_target = false;
        let mut options_done = false;
        for (at, word) in words.iter().enumerate().skip(index + 1) {
            let after_shell = words[..at].iter().any(|word| SHELLS.contains(&basename(&word.text)));
            if word.data {
                roles[at] = if after_shell { Role::Shell } else { Role::Text };
                continue;
            }
            if word.redirect {
                continue;
            }
            if let Some(role) = next.take() {
                roles[at] = role;
                continue;
            }
            if name == "eval" || (after_shell && (word.quoted || word.text.contains(char::is_whitespace))) {
                roles[at] = Role::Shell;
                continue;
            }
            let flag = !options_done && is_flag(word);
            if word.text == "--" && !word.quoted {
                options_done = true;
                continue;
            }
            if PATTERN_COMMANDS.contains(&name) || SCRIPT_COMMANDS.contains(&name) {
                let script = SCRIPT_COMMANDS.contains(&name);
                if flag {
                    match operand_flag(name, &word.text) {
                        FlagEffect::Next(role, gives) => {
                            next = Some(role);
                            given |= gives;
                        }
                        FlagEffect::Attached(role) => {
                            roles[at] = role;
                            given = true;
                        }
                        FlagEffect::Given => given = true,
                        FlagEffect::None => {}
                    }
                } else if !given {
                    roles[at] = if script { Role::Script } else { Role::Pattern };
                    given = true;
                }
            } else if TEXT_COMMANDS.contains(&name) {
                roles[at] = if feeds_args { Role::Plain } else { Role::Text };
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
            return Some(Offense::Outside("~".to_string()));
        }
        for (at, word) in words.iter().enumerate() {
            let hit = match roles[at] {
                Role::Delimiter => None,
                Role::Shell if depth < 4 => self.run(&word.text, depth + 1),
                Role::Shell => Some(Offense::Unresolved("shell code nested this deep".to_string())),
                Role::Cd => self.cd(word),
                role => {
                    let assignment = is_assignment(word) && (at < index || DECLARE_COMMANDS.contains(&name));
                    self.word_offending(word, role, assignment)
                }
            };
            if hit.is_some() {
                return hit;
            }
        }
        self.record(words, index, name);
        None
    }

    /// Remembers what this command assigns, for the commands after it.
    fn record(&mut self, words: &[ShellWord], index: usize, name: &str) {
        let declares = DECLARE_COMMANDS.contains(&name);
        for (at, word) in words.iter().enumerate() {
            if !(is_assignment(word) && (at < index || declares)) {
                continue;
            }
            let (var, value) = word.text.split_once('=').expect("an assignment");
            let value = self.resolve_vars(value).ok().filter(|value| !value.contains(UNKNOWN));
            self.vars.insert(var.to_string(), value);
        }
        let operands = words.iter().skip(index + 1).filter(|word| !word.redirect && !is_flag(word));
        match name {
            "read" | "mapfile" | "readarray" => {
                for word in operands {
                    self.vars.insert(word.text.clone(), None);
                }
            }
            "for" | "select" => {
                let mut operands = operands;
                if let Some(var) = operands.next() {
                    let unknown = operands.any(|word| self.resolve_vars(&word.text).map_or(true, |text| text.contains(UNKNOWN)));
                    if unknown {
                        self.vars.insert(var.text.clone(), None);
                    } else {
                        self.vars.remove(&var.text);
                    }
                }
            }
            _ => {}
        }
    }

    /// A `cd` target: refused when outside or not computable, else a new
    /// base for the relative words after it.
    fn cd(&mut self, word: &ShellWord) -> Option<Offense> {
        let text = match self.resolve_vars(&word.text) {
            Ok(text) => text,
            Err(offense) => return Some(offense),
        };
        if text.contains([UNKNOWN, OPAQUE]) || text.contains(['*', '?', '[', '{']) {
            return Some(Offense::Unresolved(format!("the `cd` target `{}`", shown(&word.text))));
        }
        let target = if text == "-" { "~".to_string() } else { collapse_slashes(&text) };
        let hit = self.word_offending(&ShellWord { text: target.clone(), ..word.clone() }, Role::Plain, false);
        if hit.is_none() {
            let resolved: Vec<PathBuf> = self.bases.iter().map(|base| self.check.resolve(&target, base)).collect();
            self.pwd = self.check.resolve(&target, &self.pwd);
            for path in resolved {
                if !self.bases.contains(&path) {
                    self.bases.push(path);
                }
            }
        }
        hit
    }

    fn word_offending(&self, word: &ShellWord, role: Role, assignment: bool) -> Option<Offense> {
        let text = match self.resolve_vars(&word.text) {
            Ok(text) => text,
            Err(offense) => return Some(offense),
        };
        let Some(alternatives) = brace_expand(&text) else {
            return Some(Offense::Unresolved(format!("`{}` (too many brace alternatives)", shown(&word.text))));
        };
        for alternative in &alternatives {
            for (start, part) in split_parts(alternative) {
                let whole = part.len() == alternative.len();
                if let Some(hit) = self.part_offending(word, part, role, start == 0, whole, assignment) {
                    return Some(hit);
                }
            }
        }
        None
    }

    /// One part of a word (`opens` = it starts the word, `whole` = it is the
    /// whole word).
    fn part_offending(&self, word: &ShellWord, part: &str, role: Role, opens: bool, whole: bool, assignment: bool) -> Option<Offense> {
        let unresolved = || Some(Offense::Unresolved(format!("`{}`", shown(&word.text))));
        if role == Role::Plain {
            // A path built on a value the scanner does not know.
            if let Some(at) = part.find([UNKNOWN, OPAQUE]) {
                if part[at..].contains('/') {
                    return unresolved();
                }
            }
            // A command substitution's output used as a file operand.
            if part.contains(UNKNOWN) && !assignment {
                return unresolved();
            }
        }
        let (raw, attached) = match part.strip_prefix('-') {
            Some(_) => match part.find('/') {
                Some(slash) => (&part[slash..], true),
                None => return None,
            },
            None => (part, false),
        };
        let path = collapse_slashes(raw);
        if path.starts_with('/') {
            let Some(literal) = literal_base(&path) else { return unresolved() };
            if self.check.resolve(literal, &self.check.cwd) == Path::new("/") {
                // The root alone: a path only as an operand of its own.
                if role == Role::Plain && !attached && whole {
                    return Some(Offense::Outside(shown(raw)));
                }
                return None;
            }
            if !top_level_exists(literal) && first_component(literal) != first_component(&self.check.cwd.to_string_lossy()) {
                return None;
            }
            if role == Role::Script && opens && path.ends_with('/') {
                // `sed '/usr/'`, `awk '/home/ {…}'`: a regex, not a file.
                return None;
            }
            return self.check.outside(literal, &self.check.cwd, true).map(|_| Offense::Outside(shown(raw)));
        }
        if path.starts_with('~') {
            let Some(literal) = literal_base(&path) else { return unresolved() };
            return self.check.outside(literal, &self.check.cwd, true).map(|_| Offense::Outside(shown(raw)));
        }
        if role == Role::Pattern || attached || path.contains("://") {
            return None;
        }
        let climbs = path == ".." || path.starts_with("../") || path.contains("/../") || path.ends_with("/..");
        let Some(literal) = literal_base(&path) else { return unresolved() };
        let first = literal.split('/').next().unwrap_or_default();
        for base in &self.bases {
            let exists = climbs || (!first.is_empty() && base.join(first).symlink_metadata().is_ok());
            if exists && self.check.outside(literal, base, true).is_some() {
                return Some(Offense::Outside(shown(raw)));
            }
        }
        None
    }

    /// The word with its `$` expansions resolved: a known value spelled out,
    /// an untracked variable as [`OPAQUE`], an unknown value as [`UNKNOWN`].
    /// A `${…}` the scanner cannot follow fails closed.
    fn resolve_vars(&self, text: &str) -> Result<String, Offense> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        while i < chars.len() {
            if chars[i] != EXPAND {
                out.push(chars[i]);
                i += 1;
                continue;
            }
            match chars.get(i + 1) {
                Some('{') => {
                    let Some(end) = (i + 2..chars.len()).find(|at| chars[*at] == '}') else {
                        return Err(Offense::Unresolved(format!("`{}`", shown(text))));
                    };
                    let inner: String = chars[i + 2..end].iter().collect();
                    out.push_str(&self.braced_var(&inner).ok_or_else(|| Offense::Unresolved(format!("`${{{}}}`", shown(&inner))))?);
                    i = end + 1;
                }
                Some(c) if *c == '_' || c.is_ascii_alphabetic() => {
                    let end = (i + 1..chars.len()).find(|at| !(chars[*at] == '_' || chars[*at].is_ascii_alphanumeric())).unwrap_or(chars.len());
                    let name: String = chars[i + 1..end].iter().collect();
                    out.push_str(&self.var(&name));
                    i = end;
                }
                Some(_) => {
                    // `$1`, `$@`, `$?`, …
                    out.push(OPAQUE);
                    i += 2;
                }
                None => {
                    out.push('$');
                    i += 1;
                }
            }
        }
        Ok(out)
    }

    /// `${name}`, `${#name}`, or `${name<modifier>}` on a variable that
    /// cannot name a path; `None` = refuse.
    fn braced_var(&self, inner: &str) -> Option<String> {
        if is_name(inner) {
            return Some(self.var(inner));
        }
        if inner.strip_prefix('#').is_some_and(|name| is_name(name) || name.chars().all(|c| c.is_ascii_digit())) {
            return Some("0".to_string());
        }
        if !inner.is_empty() && (inner.chars().all(|c| c.is_ascii_digit()) || matches!(inner, "@" | "*" | "#" | "?" | "$" | "!" | "-")) {
            return Some(OPAQUE.to_string());
        }
        // A modifier: only on an untracked variable, and only one that
        // cannot spell a path.
        let name: String = inner.chars().take_while(|c| *c == '_' || c.is_ascii_alphanumeric()).collect();
        let modifier = &inner[name.len()..];
        let tracked = self.vars.contains_key(&name) || matches!(name.as_str(), "HOME" | "PWD" | "OLDPWD" | "TMPDIR");
        if !is_name(&name) || tracked || modifier.contains("..") || modifier.contains(['/', '~', '$', EXPAND, UNKNOWN, '`']) {
            return None;
        }
        Some(OPAQUE.to_string())
    }

    fn var(&self, name: &str) -> String {
        match self.vars.get(name) {
            Some(Some(value)) => value.clone(),
            Some(None) => UNKNOWN.to_string(),
            None => match name {
                "HOME" => home_dir().unwrap_or_else(|| PathBuf::from("/")).display().to_string(),
                "PWD" => self.pwd.display().to_string(),
                "TMPDIR" => std::env::temp_dir().display().to_string(),
                "OLDPWD" => UNKNOWN.to_string(),
                _ => OPAQUE.to_string(),
            },
        }
    }
}

/// What a grep-ish or sed/awk flag does to the operands after it.
fn operand_flag(command: &str, flag: &str) -> FlagEffect {
    let script = SCRIPT_COMMANDS.contains(&command);
    let code = if script { Role::Script } else { Role::Pattern };
    if let Some(long) = flag.strip_prefix("--") {
        let (long, value) = match long.split_once('=') {
            Some((long, _)) => (long, true),
            None => (long, false),
        };
        return match long {
            "regexp" | "expression" | "source" => {
                if value {
                    FlagEffect::Attached(code)
                } else {
                    FlagEffect::Next(code, true)
                }
            }
            "file" if value => FlagEffect::Given,
            "file" => FlagEffect::Next(Role::Plain, true),
            "files" | "type-list" => FlagEffect::Given,
            "field-separator" if !value => FlagEffect::Next(Role::Delimiter, false),
            "assign" | "max-count" | "context" | "after-context" | "before-context" | "glob" | "iglob" | "type"
            | "type-not" | "max-depth" | "threads" | "encoding" | "replace" | "include" | "exclude"
            | "exclude-dir" | "ignore-file" | "max-filesize" | "sort" | "sortr" | "line-length"
                if !value =>
            {
                FlagEffect::Next(Role::Plain, false)
            }
            _ => FlagEffect::None,
        };
    }
    let letters: Vec<char> = flag.chars().skip(1).collect();
    if command == "sed" && letters.first() == Some(&'i') {
        // `-i.bak`: the rest is a suffix.
        return FlagEffect::None;
    }
    let valued: &str = match command {
        "sed" => "l",
        "awk" | "gawk" | "mawk" | "nawk" => "",
        "grep" | "egrep" | "fgrep" => "ABCmdD",
        _ => "ABCmgGtTjMEdr",
    };
    for (at, letter) in letters.iter().enumerate() {
        let last = at + 1 == letters.len();
        let effect = match letter {
            'e' => Some((FlagEffect::Next(code, true), FlagEffect::Attached(code))),
            'f' => Some((FlagEffect::Next(Role::Plain, true), FlagEffect::Given)),
            'F' if script && command != "sed" => Some((FlagEffect::Next(Role::Delimiter, false), FlagEffect::None)),
            'v' if script && command != "sed" => Some((FlagEffect::Next(Role::Plain, false), FlagEffect::None)),
            letter if valued.contains(*letter) => Some((FlagEffect::Next(Role::Plain, false), FlagEffect::None)),
            _ => None,
        };
        if let Some((separate, attached)) = effect {
            return if last { separate } else { attached };
        }
    }
    FlagEffect::None
}

/// `cut -d /`, `awk -F /`, `sort -t /` …: the next word is a delimiter.
fn delimiter_flag(command: &str, flag: &str) -> bool {
    match command {
        "cut" | "paste" => matches!(flag, "-d" | "--delimiter"),
        "sort" | "join" => matches!(flag, "-t" | "--field-separator"),
        "column" => matches!(flag, "-s" | "-o" | "--separator" | "--output-separator"),
        _ => false,
    }
}

fn is_flag(word: &ShellWord) -> bool {
    !word.quoted && word.text.starts_with('-') && word.text.len() > 1
}

fn is_name(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
        && !text.starts_with(|c: char| c.is_ascii_digit())
}

/// A word for a message: the lexer's marks spelled back.
fn shown(text: &str) -> String {
    text.replace(EXPAND, "$").replace(UNKNOWN, "$(…)").replace(OPAQUE, "$…")
}

/// The parts of a word worth checking on their own, with their byte offset.
fn split_parts(text: &str) -> Vec<(usize, &str)> {
    let mut parts = Vec::new();
    let mut start = 0;
    for (at, c) in text.char_indices() {
        if c.is_whitespace() || matches!(c, '\'' | '"' | '`' | ';' | '|' | '&' | '(' | ')' | '<' | '>' | '=' | ':' | ',') {
            if at > start {
                parts.push((start, &text[start..at]));
            }
            start = at + c.len_utf8();
        }
    }
    if start < text.len() {
        parts.push((start, &text[start..]));
    }
    parts
}

/// Bash brace expansion (`/{a,b}/x` → `/a/x`, `/b/x`); a brace without a
/// top-level comma, or with whitespace in it, stays literal. `None` = more
/// alternatives than worth checking.
fn brace_expand(text: &str) -> Option<Vec<String>> {
    const LIMIT: usize = 256;
    let mut done = Vec::new();
    let mut pending = vec![text.to_string()];
    while let Some(text) = pending.pop() {
        match first_brace(&text) {
            Some((open, close, commas)) => {
                let mut cuts = vec![open];
                cuts.extend(commas);
                cuts.push(close);
                for pair in cuts.windows(2) {
                    pending.push(format!("{}{}{}", &text[..open], &text[pair[0] + 1..pair[1]], &text[close + 1..]));
                }
            }
            None => done.push(text),
        }
        if done.len() + pending.len() > LIMIT {
            return None;
        }
    }
    Some(done)
}

/// The first `{…,…}` in `text`: its `{`, its `}` and its top-level commas.
fn first_brace(text: &str) -> Option<(usize, usize, Vec<usize>)> {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(open) = text[from..].find('{').map(|at| at + from) {
        let mut depth = 0usize;
        let mut commas = Vec::new();
        let mut close = None;
        for (at, byte) in bytes.iter().enumerate().skip(open) {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(at);
                        break;
                    }
                }
                b',' if depth == 1 => commas.push(at),
                byte if byte.is_ascii_whitespace() => break,
                _ => {}
            }
        }
        match close {
            Some(close) if !commas.is_empty() => return Some((open, close, commas)),
            _ => from = open + 1,
        }
    }
    None
}

fn basename(word: &str) -> &str {
    word.rsplit('/').next().unwrap_or(word)
}

/// `NAME=value` before the command word.
fn is_assignment(word: &ShellWord) -> bool {
    match word.text.split_once('=') {
        Some((name, _)) => is_name(name),
        None => false,
    }
}

/// One shell word: its text with the quotes removed (expansions marked, see
/// [`EXPAND`]), whether any of it was quoted (quoted text is data unless a
/// shell runs it), whether it is a redirection target, and whether it is a
/// here-doc body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellWord {
    pub text: String,
    pub quoted: bool,
    pub redirect: bool,
    /// A here-doc body: data, or code when a shell reads it.
    pub data: bool,
}

/// One simple command: its words, whether it pipes into the next one, and
/// whether it ran inside a command substitution.
#[derive(Debug, Default)]
pub struct ShellCommand {
    pub words: Vec<ShellWord>,
    pub feeds_next: bool,
    pub nested: bool,
}

struct Heredoc {
    delimiter: String,
    strip: bool,
    /// An unquoted delimiter: the body's `$` and `$(…)` expand.
    expand: bool,
    command: usize,
}

struct Lexer {
    commands: Vec<ShellCommand>,
    word: ShellWord,
    started: bool,
    redirect_next: bool,
    heredocs: Vec<Heredoc>,
    depth: usize,
}

const UNTERMINATED: &str = "an unterminated quote or substitution";

impl Lexer {
    fn flush(&mut self) {
        if self.started {
            let mut word = std::mem::take(&mut self.word);
            word.redirect = std::mem::take(&mut self.redirect_next);
            self.commands.last_mut().expect("one command").words.push(word);
        }
        self.word = ShellWord::default();
        self.started = false;
    }

    fn split(&mut self) {
        self.flush();
        self.redirect_next = false;
        if !self.commands.last().expect("one command").words.is_empty() {
            self.commands.push(ShellCommand::default());
        }
    }

    /// A command substitution: its commands run before the current one, its
    /// output is [`UNKNOWN`].
    fn substitute(&mut self, inner: &str) -> Result<(), String> {
        let mut commands = lex(inner, self.depth + 1)?;
        for command in &mut commands {
            command.nested = true;
        }
        let at = self.commands.len() - 1;
        for heredoc in &mut self.heredocs {
            if heredoc.command >= at {
                heredoc.command += commands.len();
            }
        }
        self.commands.splice(at..at, commands);
        self.word.text.push(UNKNOWN);
        self.started = true;
        Ok(())
    }

    /// A `$` (or an [`EXPAND`] from an outer lexer) at `at`; returns where
    /// lexing resumes.
    fn dollar(&mut self, chars: &[char], at: usize) -> Result<usize, String> {
        self.started = true;
        match chars.get(at + 1) {
            Some('(') if chars.get(at + 2) == Some(&'(') => {
                // `$((…))` is arithmetic: a number.
                let mut depth = 0usize;
                for end in at + 1..chars.len() {
                    match chars[end] {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                if chars[at + 3..end].contains(&'`') || chars[at + 3..end].windows(2).any(|pair| pair[0] == '$' && pair[1] == '(') {
                                    return Err("a command substitution inside `$((…))`".to_string());
                                }
                                self.word.text.push('0');
                                return Ok(end + 1);
                            }
                        }
                        _ => {}
                    }
                }
                Err(UNTERMINATED.to_string())
            }
            Some('(') => {
                let end = closing_paren(chars, at + 2).ok_or(UNTERMINATED)?;
                let inner: String = chars[at + 2..end].iter().collect();
                self.substitute(&inner)?;
                Ok(end + 1)
            }
            Some('{') => {
                let mut depth = 0usize;
                for end in at + 1..chars.len() {
                    match chars[end] {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                self.word.text.push(EXPAND);
                                self.word.text.extend(&chars[at + 1..=end]);
                                return Ok(end + 1);
                            }
                        }
                        _ => {}
                    }
                }
                Err(UNTERMINATED.to_string())
            }
            Some('\'') => {
                // `$'…'`: only the plain escapes are followed.
                let mut end = at + 2;
                loop {
                    match chars.get(end) {
                        None => return Err(UNTERMINATED.to_string()),
                        Some('\'') => break,
                        Some('\\') => {
                            match chars.get(end + 1) {
                                Some('n' | 't' | 'r') => self.word.text.push(' '),
                                Some(c @ ('\\' | '\'' | '"')) => self.word.text.push(*c),
                                _ => return Err("an escape inside `$'…'`".to_string()),
                            }
                            end += 2;
                        }
                        Some(c) => {
                            self.word.text.push(*c);
                            end += 1;
                        }
                    }
                }
                self.word.quoted = true;
                Ok(end + 1)
            }
            Some(c) if *c == '_' || c.is_ascii_alphanumeric() || matches!(c, '@' | '*' | '#' | '?' | '$' | '!' | '-') => {
                self.word.text.push(EXPAND);
                Ok(at + 1)
            }
            _ => {
                self.word.text.push('$');
                Ok(at + 1)
            }
        }
    }

    /// A backtick substitution at `at`; returns where lexing resumes.
    fn backtick(&mut self, chars: &[char], at: usize) -> Result<usize, String> {
        let end = closing_backtick(chars, at + 1).ok_or(UNTERMINATED)?;
        let mut inner = String::new();
        let mut i = at + 1;
        while i < end {
            if chars[i] == '\\' && matches!(chars.get(i + 1), Some('`' | '\\' | '$')) {
                i += 1;
            }
            inner.push(chars[i]);
            i += 1;
        }
        self.substitute(&inner)?;
        Ok(end + 1)
    }

    /// Double-quoted text from `start` (to the closing `"` when `quoted`,
    /// else to the end: a here-doc body); returns where lexing resumes.
    fn double_quoted(&mut self, chars: &[char], start: usize, quoted: bool) -> Result<usize, String> {
        let mut at = start;
        loop {
            let Some(&c) = chars.get(at) else {
                return if quoted { Err(UNTERMINATED.to_string()) } else { Ok(at) };
            };
            match c {
                '"' if quoted => return Ok(at + 1),
                '\\' if chars.get(at + 1) == Some(&'\n') => at += 2,
                '\\' if matches!(chars.get(at + 1), Some('"' | '\\' | '$' | '`')) => {
                    self.word.text.push(chars[at + 1]);
                    at += 2;
                }
                '$' | EXPAND => at = self.dollar(chars, at)?,
                '`' => at = self.backtick(chars, at)?,
                c => {
                    self.word.text.push(c);
                    at += 1;
                }
            }
        }
    }

    /// An unquoted here-doc body with its expansions marked.
    fn expand_body(&mut self, body: &str) -> Result<String, String> {
        let saved = std::mem::take(&mut self.word);
        let started = self.started;
        let chars: Vec<char> = body.chars().collect();
        let result = self.double_quoted(&chars, 0, false);
        let out = std::mem::replace(&mut self.word, saved).text;
        self.started = started;
        result.map(|_| out)
    }
}

/// The `)` closing a `$(` whose text starts at `start`.
fn closing_paren(chars: &[char], start: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = start;
    while at < chars.len() {
        match chars[at] {
            '\\' => at += 1,
            '\'' => at = (at + 1..chars.len()).find(|end| chars[*end] == '\'')?,
            '`' => at = closing_backtick(chars, at + 1)?,
            '"' => {
                at += 1;
                while chars.get(at)? != &'"' {
                    match chars[at] {
                        '\\' => at += 1,
                        '`' => at = closing_backtick(chars, at + 1)?,
                        '$' if chars.get(at + 1) == Some(&'(') => at = closing_paren(chars, at + 2)?,
                        _ => {}
                    }
                    at += 1;
                }
            }
            '(' => depth += 1,
            ')' if depth == 0 => return Some(at),
            ')' => depth -= 1,
            _ => {}
        }
        at += 1;
    }
    None
}

fn closing_backtick(chars: &[char], start: usize) -> Option<usize> {
    let mut at = start;
    while at < chars.len() {
        match chars[at] {
            '\\' => at += 1,
            '`' => return Some(at),
            _ => {}
        }
        at += 1;
    }
    None
}

/// The simple commands of a command line (see [`bash_offending`]); `Err`
/// = it cannot be lexed, which fails closed.
pub fn lex(command: &str, depth: usize) -> Result<Vec<ShellCommand>, String> {
    if depth > 8 {
        return Err("command substitutions nested this deep".to_string());
    }
    let chars: Vec<char> = command.chars().collect();
    let mut lexer = Lexer {
        commands: vec![ShellCommand::default()],
        word: ShellWord::default(),
        started: false,
        redirect_next: false,
        heredocs: Vec::new(),
        depth,
    };
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
                let end = (i + 1..chars.len()).find(|at| chars[*at] == '\'').ok_or(UNTERMINATED)?;
                lexer.word.text.extend(&chars[i + 1..end]);
                lexer.word.quoted = true;
                lexer.started = true;
                i = end + 1;
                continue;
            }
            '"' => {
                lexer.word.quoted = true;
                lexer.started = true;
                i = lexer.double_quoted(&chars, i + 1, true)?;
                continue;
            }
            '$' | EXPAND => {
                i = lexer.dollar(&chars, i)?;
                continue;
            }
            '`' => {
                i = lexer.backtick(&chars, i)?;
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
                let mut expand = true;
                while at < chars.len() && !chars[at].is_whitespace() && !matches!(chars[at], ';' | '|' | '&' | '<' | '>' | '(' | ')') {
                    if matches!(chars[at], '\'' | '"' | '\\') {
                        expand = false;
                    } else {
                        delimiter.push(chars[at]);
                    }
                    at += 1;
                }
                let command = lexer.commands.len() - 1;
                lexer.heredocs.push(Heredoc { delimiter, strip, expand, command });
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
                for heredoc in std::mem::take(&mut lexer.heredocs) {
                    let mut body = String::new();
                    while i < chars.len() {
                        let end = (i..chars.len()).find(|at| chars[*at] == '\n').unwrap_or(chars.len());
                        let line: String = chars[i..end].iter().collect();
                        i = end + 1;
                        let line_text = if heredoc.strip { line.trim_start_matches('\t') } else { line.as_str() };
                        if line_text == heredoc.delimiter {
                            break;
                        }
                        body.push_str(&line);
                        body.push('\n');
                    }
                    let text = if heredoc.expand { lexer.expand_body(&body)? } else { body };
                    lexer.commands[heredoc.command].words.push(ShellWord { text, quoted: true, redirect: false, data: true });
                }
                continue;
            }
            '|' if next == Some('|') => {
                lexer.split();
                i += 2;
                continue;
            }
            '|' => {
                lexer.flush();
                lexer.commands.last_mut().expect("one command").feeds_next = true;
                lexer.split();
            }
            ';' | '&' | '(' | ')' => lexer.split(),
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
    Ok(lexer.commands.into_iter().filter(|command| !command.words.is_empty()).collect())
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

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
}

/// Absolute, `..`-free, symlinks resolved as far as the path exists.
///
/// Resolved COMPONENT BY COMPONENT, never folded lexically first: with
/// `scratch/link -> ~/.ssh`, `link/../.zshrc` is what the kernel opens —
/// `~/.zshrc` — so the link is followed BEFORE the `..` pops the real
/// directory it points into (a cloned repo or an extracted tar can plant
/// such a link). Past the deepest existing ancestor (a file about to be
/// written) the remainder stays lexical, `..` popping the lexical tail.
fn resolve_path(path: &Path, cwd: &Path) -> PathBuf {
    let absolute = if path.is_absolute() { path.to_path_buf() } else { cwd.join(path) };
    let mut out = PathBuf::new();
    // `out` is canonical (symlinks followed) while it exists; once a
    // component is missing the rest is appended as named.
    let mut exists = true;
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
                if !exists {
                    exists = out.exists();
                }
            }
            other => {
                out.push(other.as_os_str());
                if exists {
                    match std::fs::canonicalize(&out) {
                        Ok(canonical) => out = canonical,
                        Err(_) => exists = false,
                    }
                }
            }
        }
    }
    out
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

/// `/a/b/**/*.rs` → `/a/b`: the literal directory a glob pattern starts in
/// (an expansion the scanner does not know counts as a wildcard).
pub fn glob_base(pattern: &str) -> &str {
    let Some(cut) = pattern.find(['*', '?', '[', '{', UNKNOWN, OPAQUE]) else { return pattern };
    match pattern[..cut].rfind('/') {
        Some(0) => "/",
        Some(index) => &pattern[..index],
        None => "",
    }
}

/// [`glob_base`], or `None` when the pattern climbs with `..` after its
/// wildcard (`/tmp/*/../../home`), where the base says nothing.
fn literal_base(pattern: &str) -> Option<&str> {
    let base = glob_base(pattern);
    let rest = pattern.get(base.len()..).unwrap_or_default();
    if base.len() < pattern.len() && rest.split('/').any(|component| component == "..") {
        return None;
    }
    Some(base)
}

/// `//x`, `///x` → `/x` (the kernel reads them alike).
fn collapse_slashes(raw: &str) -> String {
    if raw.starts_with("//") {
        format!("/{}", raw.trim_start_matches('/'))
    } else {
        raw.to_string()
    }
}

fn first_component(path: &str) -> &str {
    path.trim_start_matches('/').split('/').next().unwrap_or_default()
}

/// Whether `/<first>` exists: an absolute-looking word under a top-level
/// directory nobody has is a pattern or a URL path, not a file.
fn top_level_exists(word: &str) -> bool {
    let first = first_component(word);
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

    /// Wave review of #1254..#1285: `link/../secret` opens `<link
    /// target>/../secret`, so the link is followed BEFORE the `..` — a
    /// lexical fold would score it inside the scratch dir.
    #[cfg(unix)]
    #[test]
    fn a_dot_dot_after_a_symlink_pops_the_link_target() {
        let dir = temp("symlink-dotdot");
        let scratch = dir.join("scratch");
        std::fs::create_dir_all(scratch.join("sub")).unwrap();
        let home = home_dir().expect("HOME");
        std::os::unix::fs::symlink(&home, scratch.join("link")).unwrap();
        let denied = |tool: &str, input: Value| denial_in(tool, input, &scratch, &[]).is_some();
        assert_eq!(resolve_path(Path::new("link/../secret"), &scratch), resolve_path(&home, Path::new("/")).parent().unwrap().join("secret"));
        assert!(denied("Read", json!({ "file_path": "link/../secret" })));
        assert!(denied("Write", json!({ "file_path": scratch.join("link/../secret").display().to_string(), "content": "x" })));
        assert!(denied("Bash", json!({ "command": "cat link/../secret" })));
        // A real dir, then `..`: still inside.
        assert!(!denied("Read", json!({ "file_path": "sub/../inside.txt" })));
        assert!(!denied("Bash", json!({ "command": "cat sub/../inside.txt" })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The write case: a file (and dirs) that do not exist yet inside the
    /// scratch dir resolve under it, `..` on the missing tail popping lexically.
    #[test]
    fn a_missing_target_inside_the_scratch_dir_is_allowed() {
        let dir = temp("missing-inside");
        let scratch = dir.join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        let denied = |tool: &str, input: Value| denial_in(tool, input, &scratch, &[]).is_some();
        assert!(!denied("Write", json!({ "file_path": "new/deep/report.md", "content": "x" })));
        assert!(!denied("Write", json!({ "file_path": "new/deep/../report.md", "content": "x" })));
        assert_eq!(
            resolve_path(Path::new("new/deep/../report.md"), &scratch),
            resolve_path(&scratch, Path::new("/")).join("new/report.md")
        );
        // Climbing out through the missing tail lands outside (the temp dir
        // around this scratch dir is open, so only the resolution is checked).
        assert_eq!(
            resolve_path(Path::new("new/../../outside.md"), &scratch),
            resolve_path(&dir, Path::new("/")).join("outside.md")
        );
        assert!(denied("Write", json!({ "file_path": "new/../../../../../../../../../../etc/outside.md", "content": "x" })));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Fail closed: a file tool that always names its file, with no path
    /// key at all (or an empty one), is refused; `Glob`/`Grep` default to
    /// the cwd and stay allowed.
    #[test]
    fn a_file_tool_without_a_path_is_refused() {
        for tool in PATH_REQUIRED_TOOLS {
            assert!(denied(tool, json!({})), "{tool}");
            assert!(denied(tool, json!({ "file_path": "  " })), "{tool} blank");
            assert!(denied(tool, json!({ "content": "x" })), "{tool} content only");
        }
        let reason = denial_in("Read", json!({}), &scratch(), &[]).unwrap();
        assert!(reason.contains("names no file path"), "{reason}");
        assert!(!denied("Glob", json!({ "pattern": "*.md" })));
        assert!(!denied("Grep", json!({ "pattern": "todo" })));
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
    /// a hash; the harness names that EXACT dir (wave review of
    /// #1254..#1285: a prefix match opened a sibling cwd's memory dir).
    #[test]
    fn a_long_scratch_dir_matches_its_hash_suffixed_project_dir() {
        let base = temp("harness");
        let config = base.join("config");
        let dir = PathBuf::from(format!("/nonexistent-exp-scratch/{}/root/session", "x".repeat(220)));
        let sibling = PathBuf::from(format!("/nonexistent-exp-scratch/{}/root/sibling", "x".repeat(220)));
        let munged = munge_project_name(&dir.display().to_string());
        assert!(munged.len() > 200);
        assert_eq!(&munged[..200], &munge_project_name(&sibling.display().to_string())[..200]);
        // The suffixes claude 2.1.308 writes for these two paths.
        let capped = config.join("projects").join(format!("{}-2n2ii0", &munged[..200]));
        let other = config.join("projects").join(format!("{}-10czy4", &munged[..200]));
        assert_eq!(capped_project_name(&dir.display().to_string()), capped.file_name().unwrap().to_string_lossy());
        assert_eq!(capped_project_name(&sibling.display().to_string()), other.file_name().unwrap().to_string_lossy());
        std::fs::create_dir_all(capped.join("memory")).unwrap();
        std::fs::create_dir_all(other.join("memory")).unwrap();
        std::fs::create_dir_all(config.join("projects/-other/memory")).unwrap();
        let harness = claude_harness_dirs(Some(&config), &[&dir]);
        assert_eq!(harness, vec![resolve_path(&capped, Path::new("/"))]);
        // The sibling's memory dir stays shut: only the cwd's project dir is
        // an allowed root (the config dir sits under the open temp dir, so
        // the roots are checked, not a denial).
        let allowed = Check::new(&dir, &Confinement { root: &dir, harness: &harness }).allowed;
        assert!(allowed.contains(&resolve_path(&capped, Path::new("/"))));
        assert!(!allowed.contains(&resolve_path(&other, Path::new("/"))));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The cap's hash is Java's `String.hashCode` over UTF-16 units, `abs`,
    /// base36 (claude 2.1.308 `J0`/`Bue`); pinned against the CLI's output.
    #[test]
    fn a_capped_project_name_hashes_like_claude() {
        assert_eq!(capped_project_name("/home/a"), "-home-a");
        let long = format!("/{}", "y".repeat(230));
        let capped = capped_project_name(&long);
        assert_eq!(capped.len(), 200 + 1 + capped.rsplit('-').next().unwrap().len());
        assert!(capped.starts_with(&format!("-{}-", "y".repeat(199))));
        let suffix = |path: &str| capped_project_name(&format!("{}{path}", "z".repeat(210))).rsplit('-').next().unwrap().to_string();
        // `Math.abs(Bue(t)).toString(36)` for `z`×210 + the tail, computed with bun.
        assert_eq!(suffix("/ä😀"), "719vco");
        assert_ne!(suffix("/a"), suffix("/b"));
    }

    #[test]
    fn a_project_name_munges_like_claude() {
        assert_eq!(munge_project_name("/home/a/.local/x_y"), "-home-a--local-x-y");
        // One `-` per UTF-16 unit, like the JS regex.
        assert_eq!(munge_project_name("/ä😀"), "----");
    }

    #[test]
    fn the_lexer_splits_simple_commands_and_keeps_quotes() {
        let commands = lex("cd a && cat 'b c' > out; echo \"x\\\"y\" | wc # note\nls", 0).expect("lexes");
        let texts: Vec<Vec<&str>> =
            commands.iter().map(|command| command.words.iter().map(|word| word.text.as_str()).collect()).collect();
        assert_eq!(texts, vec![vec!["cd", "a"], vec!["cat", "b c", "out"], vec!["echo", "x\"y"], vec!["wc"], vec!["ls"]]);
        assert!(commands[1].words[1].quoted);
        assert!(commands[1].words[2].redirect);
        assert!(commands[2].feeds_next && !commands[3].feeds_next);
        // A substitution inside double quotes is code: its command comes first.
        let commands = lex("cd \"$(dirname \"$PWD\")\" && ls", 0).expect("lexes");
        assert_eq!(commands[0].words[0].text, "dirname");
        assert!(commands[0].nested);
        assert_eq!(commands[1].words[1].text, UNKNOWN.to_string());
        // Unparseable quoting fails closed.
        assert!(lex("cat 'x", 0).is_err());
        assert!(lex("echo \"$(ls", 0).is_err());
    }

    /// N-5 review: commands the first scanner let through.
    #[test]
    fn the_reviewed_escapes_are_denied() {
        let home = home();
        for command in [
            // 1. Patterns still name paths; `--files`, `-e<x>`, `--regexp=` give the pattern.
            "rg --files ~".to_string(),
            format!("rg --files {home}"),
            format!("grep -rn -epassword {home}"),
            format!("grep -rn --regexp=password {home}"),
            format!("grep -rn '{home}/x' ."),
            "grep -rn x ~/code".to_string(),
            "grep -rn \"$HOME\" .".to_string(),
            // 2. `${…}` modifiers, computed `cd` targets, substitutions in double quotes.
            "cat ${HOME%/}/CLAUDE.md".to_string(),
            "cat ${HOME:-x}/x".to_string(),
            "cat \"${HOME:+$HOME}/x\"".to_string(),
            "cd \"${PWD%/*/*/*}\" && cat CLAUDE.md".to_string(),
            "cd \"$(dirname \"$PWD\")\" && ls -R".to_string(),
            "cd `dirname $PWD` && ls -R".to_string(),
            "cd \"`dirname $PWD`\" && cat CLAUDE.md".to_string(),
            "cat \"$(dirname \"$PWD\")/CLAUDE.md\"".to_string(),
            "ls -R \"$(dirname \"$PWD\")\"".to_string(),
            "d=$(dirname \"$PWD\"); cat $d/CLAUDE.md".to_string(),
            "cat ${!x}".to_string(),
            "cat $XDG_CONFIG_HOME/x".to_string(),
            "bash <<EOF\ncat ${HOME%/}/CLAUDE.md\nEOF".to_string(),
            "bash <<'EOF'\ncat ${HOME%/}/CLAUDE.md\nEOF".to_string(),
            "bash <<EOF\ncd \"$(dirname \"$PWD\")\" && ls -R\nEOF".to_string(),
            "sh <<'EOF'\ncd `dirname $PWD` && cat CLAUDE.md\nEOF".to_string(),
            // 3. Literal-root spellings.
            format!("cat /{home}/CLAUDE.md"),
            format!("cat ///{home}/CLAUDE.md"),
            "cat /{home,x}/someone/CLAUDE.md".to_string(),
            "cat /tmp/{a,../../home}/x".to_string(),
            "cat /tmp/*/../../home/x".to_string(),
            "curl file:///etc/../home/x".to_string(),
            // 4. Shells behind wrappers, `xargs`.
            "timeout 60 sh -c 'find / -name exponential'".to_string(),
            "nice -n 5 bash -c 'ls /'".to_string(),
            "ionice -c 3 sh -c 'ls ~'".to_string(),
            "stdbuf -oL bash -c 'cat ~/x'".to_string(),
            "flock /tmp/lock -c 'ls /'".to_string(),
            "su - me -c 'ls /'".to_string(),
            "script -q -c 'ls /' /dev/null".to_string(),
            "watch -n 1 'ls /'".to_string(),
            "watch ls /".to_string(),
            "env -S 'ls /'".to_string(),
            "env -C / ls".to_string(),
            "echo / | xargs ls".to_string(),
            format!("printf '%s' {home} | xargs -I{{}} ls {{}}"),
            "sudo -u root timeout 5 xargs -n 1 ls /".to_string(),
            // 5. A `/re/`-looking path inside a script.
            format!("awk 'BEGIN{{system(\"cd {home}/ && cat CLAUDE.md\")}}'"),
            format!("sed -n 'w {home}/out/' x"),
            // Unparseable quoting fails closed.
            "cat 'x".to_string(),
            "echo \"$(cat x\"".to_string(),
        ] {
            assert!(bash(&command), "{command}");
        }
        // macOS: the Data volume spells home again (`/home` links into it).
        if cfg!(target_os = "macos") {
            for command in [format!("cat /System/Volumes/Data{home}/CLAUDE.md"), "ls /System/Volumes/Data/home".to_string()] {
                assert!(bash(&command), "{command}");
            }
        }
        // The unresolved ones say so.
        let message = denial_in("Bash", json!({ "command": "cat ${HOME%/}/x" }), &scratch(), &[]).expect("denied");
        assert!(message.contains("cannot check `${HOME%/}`"), "{message}");
    }

    #[test]
    fn a_relative_glob_climbing_out_is_denied() {
        assert!(denied("Glob", json!({ "pattern": "../../../../**/*.md" })));
        assert!(denied("Glob", json!({ "pattern": "**/../../x" })));
        assert!(denied("Glob", json!({ "pattern": "*.md", "path": "../.." })));
        assert!(!denied("Glob", json!({ "pattern": "sub/**/*.md" })));
        assert!(!denied("Glob", json!({ "pattern": "../session/*.md" })));
    }

    /// What stays allowed: tracked variables, loops, plain substitutions.
    #[test]
    fn resolvable_expansions_stay_allowed() {
        for command in [
            "d=sub; cat $d/x",
            "cat \"$PWD/notes.md\"",
            "cd /tmp && cat $PWD/x",
            "for f in *.md; do wc -l \"$f\"; done",
            "echo \"$(date)\" > out.txt",
            "n=$(wc -l < x); echo $n",
            "echo ${#x} ${x:-default} ${f%.md}",
            "IFS=$'\\n' read -r a",
            "cat \"$TMPDIR/x\"",
            "echo {a,b}/c",
            "timeout 60 bash -c 'ls sub'",
            "find . -name '*.rs' | xargs grep -n foo",
            "cat <<EOF > notes.md\nbuilt $(date) in $PWD\nEOF",
        ] {
            assert!(!bash(command), "{command}");
        }
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
