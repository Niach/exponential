//! EXP-1206: the human-readable head of a command line, the detail a Bash
//! row wears when the agent gave no description of its own.
//!
//! `cd apps/web && GW=$(cat x) && bun run typecheck` reads as
//! `bun run typecheck`, never `cd`. Only the PROGRAM (its file name) and, for
//! a short list of tools whose subcommand IS the action (`git status`,
//! `cargo test`), a word-shaped subcommand leave this function: no argument,
//! path, URL or heredoc reaches the relay.

/// Programs whose first word-shaped non-flag argument names the action.
const SUBCOMMAND_PROGRAMS: &[&str] = &[
    "git", "bun", "bunx", "npm", "npx", "pnpm", "yarn", "cargo", "docker", "kubectl", "gradle",
    "gradlew", "xcodebuild", "tuist", "make", "python", "python3", "node", "go", "rustc", "brew",
    "pip", "pip3",
];

/// A segment led by one of these does nothing worth naming (`cd x &&`,
/// `export A=1;`, `set -e;`): the head is the next segment's program.
const NOISE_PROGRAMS: &[&str] = &[
    "cd", "pushd", "popd", "export", "set", "unset", "setopt", "shopt", "source", ".", "true", ":",
    "local", "alias", "trap", "ulimit", "umask", "for", "case", "fi", "done", "esac", "}",
    "function", "[", "[[", "test",
];

/// Leading words that only wrap the real program (`sudo make`, `time cargo`,
/// `if git …`).
const WRAPPERS: &[&str] = &[
    "env", "time", "timeout", "sudo", "nohup", "command", "exec", "if", "then", "else", "elif",
    "do", "while", "until", "!", "{",
];

/// The longest subcommand worth a caption.
const SUBCOMMAND_MAX: usize = 40;

/// The head of `command`: the first real program (by file name) plus its
/// subcommand for [`SUBCOMMAND_PROGRAMS`]. A `<shell> -lc "<command>"`
/// wrapper names the inner command. Empty when nothing is recognisable.
pub fn command_head(command: &str) -> String {
    let segments = segments(command);
    // A `zsh -lc "<command>"` wrapper: the inner command is the one to name.
    if let Some(first) = segments.first() {
        if let [shell, flag, inner, ..] = first.as_slice() {
            let is_shell = matches!(file_name(shell), "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish");
            if is_shell && matches!(flag.as_str(), "-lc" | "-c" | "-ic" | "-lic") {
                return command_head(&unquote(inner));
            }
        }
    }
    let mut noise: Option<String> = None;
    for segment in &segments {
        let mut words = segment.iter().map(String::as_str).peekable();
        // `timeout 30 bun test`: the wrapper's duration is still to skip.
        let mut duration_pending = false;
        // Assignments (`GW=$(cat x)`), wrappers and their flags.
        let program = loop {
            let Some(word) = words.next() else { break None };
            if is_assignment(word) || WRAPPERS.contains(&word) {
                duration_pending |= word == "timeout";
                continue;
            }
            if duration_pending && word.starts_with(|c: char| c.is_ascii_digit()) {
                duration_pending = false;
                continue;
            }
            if word.starts_with('-') {
                // `sudo -u root`, `env -u NAME`: the wrapper's flag value.
                if matches!(word, "-u" | "-g") {
                    words.next();
                }
                continue;
            }
            break Some(word);
        };
        let Some(program) = program else { continue };
        // `$(…)` or a backtick substitution run as a command names nothing.
        if program.starts_with('$') || program.starts_with('`') || program.starts_with('#') {
            continue;
        }
        let program = unquote(program);
        let name = file_name(&program);
        if name.is_empty() {
            continue;
        }
        if NOISE_PROGRAMS.contains(&name) {
            noise.get_or_insert_with(|| name.to_string());
            continue;
        }
        return match subcommand(name, &mut words) {
            Some(sub) => format!("{name} {sub}"),
            None => name.to_string(),
        };
    }
    noise.unwrap_or_default()
}

/// The subcommand of a [`SUBCOMMAND_PROGRAMS`] program, past its flags (and
/// the values of the flags that take one).
fn subcommand<'a>(program: &str, words: &mut impl Iterator<Item = &'a str>) -> Option<String> {
    if !SUBCOMMAND_PROGRAMS.contains(&program) {
        return None;
    }
    // xcodebuild's flags mostly take values; its action is a known word.
    if program == "xcodebuild" {
        const ACTIONS: &[&str] = &[
            "build", "test", "archive", "clean", "analyze", "install", "build-for-testing",
            "test-without-building", "docbuild",
        ];
        return words.find(|word| ACTIONS.contains(word)).map(str::to_string);
    }
    let python = program.starts_with("python");
    let mut words = words.peekable();
    while let Some(word) = words.next() {
        if python && word == "-m" {
            return words.next().filter(|module| is_word(module)).map(|module| format!("-m {module}"));
        }
        if python && word == "-c" || program == "node" && matches!(word, "-e" | "-p" | "--eval") {
            return None;
        }
        if word.starts_with('-') || (program == "cargo" && word.starts_with('+')) {
            if !word.contains('=') && takes_value(program, word) {
                words.next();
            }
            continue;
        }
        if !is_word(word) {
            return None;
        }
        // `bun run typecheck`: a script runner names the script too.
        let runs_script = matches!(program, "bun" | "npm" | "pnpm" | "yarn") && word == "run";
        if runs_script {
            while let Some(next) = words.next() {
                if next.starts_with('-') {
                    if !next.contains('=') && takes_value(program, next) {
                        words.next();
                    }
                    continue;
                }
                if is_word(next) {
                    return Some(format!("run {next}"));
                }
                break;
            }
        }
        return Some(word.to_string());
    }
    None
}

/// The flags (of a [`SUBCOMMAND_PROGRAMS`] program) whose value is the
/// next word, so `git -C x status` reads as `git status`.
fn takes_value(program: &str, flag: &str) -> bool {
    let flags: &[&str] = match program {
        "git" => &["-C", "-c", "--git-dir", "--work-tree", "--namespace"],
        "bun" | "bunx" => &["--cwd", "--filter", "-F", "--config"],
        "npm" | "npx" => &["--prefix", "-w", "--workspace", "-p", "--package"],
        "pnpm" => &["-C", "--dir", "--filter", "-F"],
        "yarn" => &["--cwd"],
        "cargo" => &["-C", "-Z", "--manifest-path", "--config"],
        "docker" => &["-H", "--host", "--context", "-c", "--config", "-l", "--log-level"],
        "kubectl" => &["-n", "--namespace", "--context", "--kubeconfig", "--cluster", "--user"],
        "gradle" | "gradlew" => &["-p", "--project-dir", "-b", "--build-file", "-x", "--exclude-task"],
        "make" => &["-C", "-f", "--directory", "--file", "-I"],
        "go" => &["-C"],
        "node" => &["-r", "--require", "--import", "--loader"],
        "rustc" => &["-o", "--edition", "--crate-type", "--crate-name", "--target", "-L"],
        _ => &[],
    };
    flags.contains(&flag)
}

/// A word-shaped token (`status`, `typecheck`, `:app:test`, `build-for-x`):
/// never a path, URL, quoted string or assignment.
fn is_word(token: &str) -> bool {
    token.len() <= SUBCOMMAND_MAX
        && token.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == ':')
        && token.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
}

/// `NAME=value` (the name an identifier).
fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else { return false };
    !name.is_empty()
        && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The last path component (`./gradlew` → `gradlew`, `/bin/zsh` → `zsh`).
fn file_name(token: &str) -> &str {
    token.rsplit('/').next().unwrap_or(token)
}

/// One level of surrounding quotes off a word.
fn unquote(word: &str) -> String {
    let bytes = word.as_bytes();
    if bytes.len() >= 2 && (bytes[0] == b'"' || bytes[0] == b'\'') && bytes[bytes.len() - 1] == bytes[0] {
        let inner = &word[1..word.len() - 1];
        return if bytes[0] == b'"' { inner.replace("\\\"", "\"") } else { inner.to_string() };
    }
    word.to_string()
}

/// The simple commands of a command line: words split on whitespace,
/// grouped between `&&`, `||`, `;`, `|`, `&`, newlines and subshell parens.
/// Quotes, `$(…)` and backticks stay inside their word.
fn segments(command: &str) -> Vec<Vec<String>> {
    let mut segments: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    let mut depth = 0usize;
    let mut backtick = false;
    let mut chars = command.chars().peekable();
    let end_word = |word: &mut String, current: &mut Vec<String>| {
        if !word.is_empty() {
            current.push(std::mem::take(word));
        }
    };
    let end_segment = |current: &mut Vec<String>, segments: &mut Vec<Vec<String>>| {
        if !current.is_empty() {
            segments.push(std::mem::take(current));
        }
    };
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            word.push(c);
            if c == '\\' && q == '"' {
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '\\' if matches!(chars.peek(), Some('\n') | Some('\r')) => {
                // A `\` line continuation is whitespace (one line break only:
                // a blank line after it still ends the command).
                if chars.peek() == Some(&'\r') {
                    chars.next();
                }
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                end_word(&mut word, &mut current);
            }
            '\\' => {
                word.push(c);
                if let Some(next) = chars.next() {
                    word.push(next);
                }
            }
            '\'' | '"' => {
                quote = Some(c);
                word.push(c);
            }
            '`' => {
                backtick = !backtick;
                word.push(c);
            }
            _ if backtick => word.push(c),
            '(' if word.is_empty() && depth == 0 => {
                // A subshell `( … )` opens a new command.
                end_segment(&mut current, &mut segments);
            }
            '(' => {
                depth += 1;
                word.push(c);
            }
            ')' if depth > 0 => {
                depth -= 1;
                word.push(c);
            }
            _ if depth > 0 => word.push(c),
            ')' | ';' | '\n' => {
                end_word(&mut word, &mut current);
                end_segment(&mut current, &mut segments);
            }
            '&' if word.ends_with('>') || word.ends_with('<') || chars.peek() == Some(&'>') => {
                // `2>&1`, `&>file`: a redirection, not a separator.
                word.push(c);
            }
            '&' | '|' => {
                end_word(&mut word, &mut current);
                end_segment(&mut current, &mut segments);
                if chars.peek() == Some(&c) {
                    chars.next();
                }
            }
            '#' if word.is_empty() => {
                // A comment runs to the end of its line.
                for next in chars.by_ref() {
                    if next == '\n' {
                        break;
                    }
                }
                end_segment(&mut current, &mut segments);
            }
            c if c.is_whitespace() => end_word(&mut word, &mut current),
            _ => word.push(c),
        }
    }
    end_word(&mut word, &mut current);
    end_segment(&mut current, &mut segments);
    segments
}

#[cfg(test)]
mod tests {
    use super::command_head;

    #[test]
    fn a_plain_command_is_its_program() {
        assert_eq!(command_head("cat Cargo.toml"), "cat");
        assert_eq!(command_head("curl -s https://example.com/?token=abc"), "curl");
        assert_eq!(command_head("ls -la"), "ls");
        assert_eq!(command_head("/usr/bin/grep -rn foo ."), "grep");
    }

    #[test]
    fn a_cd_prefixed_chain_names_the_real_command() {
        assert_eq!(command_head("cd apps/web && bun run typecheck"), "bun run typecheck");
        assert_eq!(command_head("cd /tmp/worktree; git status"), "git status");
        assert_eq!(command_head("(cd apps/desktop && cargo test -p engine)"), "cargo test");
        assert_eq!(command_head("set -e; cd x && make build"), "make build");
        assert_eq!(command_head("cd apps/web && \\\n  bun run typecheck"), "bun run typecheck");
    }

    #[test]
    fn variable_assignments_and_exports_are_skipped() {
        assert_eq!(command_head(r#"export PATH="/opt/node/bin:$PATH" && bun install"#), "bun install");
        assert_eq!(command_head("DATABASE_URL=postgres://x bun run test"), "bun run test");
        assert_eq!(command_head("GW=$(cat x) && curl -H \"Authorization: $GW\" https://h"), "curl");
        assert_eq!(command_head("FOO=$(git rev-parse HEAD)\necho $FOO"), "echo");
        assert_eq!(command_head("FOO=1 \\\n  bun test"), "bun test");
        assert_eq!(command_head("DATABASE_URL=x \\\r\n  bun test"), "bun test");
    }

    #[test]
    fn flags_between_program_and_subcommand_are_skipped() {
        assert_eq!(command_head("git -C x status"), "git status");
        assert_eq!(command_head("git \\\n  status"), "git status");
        assert_eq!(command_head("git -c core.pager=cat --no-pager log -5"), "git log");
        assert_eq!(command_head("bun run --filter @exp/web build"), "bun run build");
        assert_eq!(command_head("cargo +nightly build --release"), "cargo build");
        assert_eq!(command_head("./gradlew :app:compileProductionDebugKotlin -q"), "gradlew :app:compileProductionDebugKotlin");
        assert_eq!(
            command_head("xcodebuild build -workspace X.xcworkspace -scheme App -destination 'name=iPhone'"),
            "xcodebuild build"
        );
        assert_eq!(command_head("xcodebuild -workspace X.xcworkspace -scheme App test"), "xcodebuild test");
        assert_eq!(command_head("python3 -m pytest -q"), "python3 -m pytest");
        assert_eq!(command_head("python3 -c 'print(1)'"), "python3");
    }

    #[test]
    fn wrappers_are_skipped() {
        assert_eq!(command_head("sudo -u root make install"), "make install");
        assert_eq!(command_head("time cargo test"), "cargo test");
        assert_eq!(command_head("timeout 30 bun test"), "bun test");
        assert_eq!(command_head("timeout 5m cargo test -p engine"), "cargo test");
        assert_eq!(command_head("env FOO=1 npm test"), "npm test");
        assert_eq!(command_head("$(which node) script.js"), "");
    }

    #[test]
    fn an_argument_never_reaches_the_head() {
        // A subcommand is a word, never a path, URL or quoted string.
        assert_eq!(command_head("npm test --token expu_supersecretkey"), "npm test");
        assert_eq!(command_head("node /tmp/secret/script.js"), "node");
        assert_eq!(command_head("git 'https://user:pw@host/x'"), "git");
    }

    #[test]
    fn a_shell_wrapper_names_the_inner_command() {
        assert_eq!(command_head("/bin/zsh -lc \"printf 'smoke %s' one two\""), "printf");
        assert_eq!(command_head("bash -c 'cd x && git push origin HEAD'"), "git push");
    }

    #[test]
    fn a_noise_only_command_keeps_its_program() {
        assert_eq!(command_head("cd apps/web"), "cd");
        assert_eq!(command_head("export FOO=1"), "export");
        assert_eq!(command_head(""), "");
    }
}
