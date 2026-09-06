//! `exponential code <ISSUE>` — the desktop's Start-coding flow, headless:
//! resolve the issue over tRPC, run the ONE shared launcher
//! (`coding::prepare_with_hooks`), spawn the agent TUI on a real PTY. With
//! a tty the PTY attaches raw (use it exactly like the desktop terminal);
//! detached it runs to completion, steerable from the web.

use std::collections::HashMap;
use std::io::BufRead as _;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Context as _;
use coding::{Prepared, PrepareRequest};

use super::{reject_unknown_flags, take_flag, take_value, CommandResult};
use crate::launch::{self, AgentFlags};
use crate::session_host::{self, LaunchEnv, RunningSession};
use crate::sidecars::Sidecars;
use crate::{context, term};

pub fn run(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let flags = AgentFlags {
        agent: take_value(&mut args, "--agent"),
        model: take_value(&mut args, "--model"),
        effort: take_value(&mut args, "--effort"),
        plan: take_flag(&mut args, "--plan"),
    };
    let detach = take_flag(&mut args, "--detach");
    reject_unknown_flags(&args)?;
    let Some(issue_ref) = args.first() else {
        anyhow::bail!("usage: exponential code <ISSUE> [--agent claude|codex|pi] [--model m] [--effort e] [--plan] [--detach]");
    };

    let ctx = context::load()?;
    let interactive = !detach && term::stdin_is_tty() && term::stdout_is_tty();
    let options = launch::agent_options(&ctx.settings, &flags, interactive)?;
    // EXP-746: the shared registry decision for every end this process
    // issues (daemon parity — `registry::install_end_observer`).
    crate::registry::install_end_observer(ctx.data_dir.clone());

    let fetched = api::issues::issues_get(&ctx.trpc, issue_ref)
        .with_context(|| format!("resolve issue `{issue_ref}`"))?;
    let issue = fetched.issue;
    println!("Starting {} — {}", issue.identifier, issue.title);

    let request = launch::issue_launch_request(&issue, options, coding::LaunchOrigin::Local, false);
    let mut seeds = HashMap::new();
    seeds.insert(issue.id.clone(), launch::issue_seed(&issue));
    // EXP-746: the runtime is resolved BEFORE `prepare` — it decides the
    // launch's transport (`CodingDeps::acp_available`), and the two
    // transports compose different argv.
    let runtime = steer::SteerRuntime::new().ok();
    let deps = launch::coding_deps(&ctx, seeds, launch::LaunchHost::Foreground, runtime.as_ref());

    let sidecars = Sidecars::start();
    let personal_key = context::ensure_personal_key(&ctx).ok();

    let prepared = coding::prepare_with_hooks(
        &PrepareRequest::Issue(request),
        &deps,
        sidecars.hook_setup().as_ref(),
        sidecars.observer_setup().as_ref(),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;
    let prepared = match prepared {
        Prepared::Ready(prepared) => prepared,
        Prepared::Disabled(reason) => {
            eprintln!("Can't start coding: {}", reason.message());
            return Ok(ExitCode::FAILURE);
        }
    };

    let env = LaunchEnv {
        ctx: &ctx,
        runtime: runtime.as_ref(),
        sidecars: &sidecars,
        personal_key,
    };
    let session = session_host::launch(&env, prepared, interactive, Some(issue.id.clone()))?;
    let session = Arc::new(session);
    println!(
        "Session {} on branch {} (worktree {})",
        session.session_id,
        session.branch,
        session.worktree.display()
    );

    match (interactive, session.attaches_by_line()) {
        // EXP-746: an ACP run has no PTY to tee, so its attach is a line
        // transcript plus a line composer.
        (true, true) => attend_acp(&session),
        (true, false) => attend(&session),
        (false, _) => {
            println!("Detached — steer it from the web.");
            wait_with_signals(&session)
        }
    }
}

/// Interactive attach: raw-mode stdin straight into the PTY (the same
/// shared writer remote steer input uses), output mirrored by the session
/// host's tee, local resizes forwarded. Shared with `run`.
pub fn attend(session: &Arc<RunningSession>) -> CommandResult {
    let raw = term::RawMode::enter();

    // stdin pump — reads stay blocking; the thread dies with the process.
    {
        let session = Arc::clone(session);
        std::thread::spawn(move || {
            use std::io::Read as _;
            let mut stdin = std::io::stdin().lock();
            let mut buf = [0u8; 1024];
            loop {
                match stdin.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => session.write_stdin(&buf[..n]),
                }
            }
        });
    }
    // Resize watcher — polling beats signal plumbing here and reacts fast
    // enough for a human dragging a window.
    if session.supports_resize() {
        let session = Arc::clone(session);
        let mut last = term::window_size();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(400));
            let now = term::window_size();
            if now != last {
                if let Some((cols, rows)) = now {
                    session.resize(cols, rows);
                }
                last = now;
            }
        });
    }

    let exit = session.wait();
    drop(raw);
    println!();
    println!("Session ended (exit {}).", exit.code);
    Ok(if exit.success { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

// ---------------------------------------------------------------------------
// EXP-746: the ACP attach — a line transcript plus a line composer
// ---------------------------------------------------------------------------

/// How many output lines ONE tool call may print before the transcript stops
/// following it. A runaway build log must not bury the conversation; the full
/// output is still on the session screen and in the agent's own context.
const OUTPUT_LINE_CAP: usize = 200;

/// The ACP attach. There is no PTY here, so no raw mode, no byte tee and no
/// resize watcher: a printer thread renders the engine's local feed in the
/// SAME vocabulary web, iOS and Android render (a run reads identically
/// wherever it is watched), and a composer thread reads whole LINES from
/// stdin. Shared with `run`.
///
/// The composer routes exactly like a remote steerer: `/name args` through
/// the command sink, a bare number as the answer to the pending question
/// card, anything else as a message.
pub fn attend_acp(session: &Arc<RunningSession>) -> CommandResult {
    let Some(feed) = session.feed() else {
        // Not an ACP session after all — the raw byte tee is its attach.
        return attend(session);
    };
    let state = Arc::new(Mutex::new(AttachState::default()));
    println!("Type a message and press Enter. `/name` runs a command; a bare number answers a question.");

    {
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            for event in feed.iter() {
                print_event(&event, &state);
            }
        });
    }
    {
        let session = Arc::clone(session);
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                let Ok(line) = line else { return };
                compose(&session, &state, line);
            }
        });
    }

    let exit = session.wait();
    println!();
    println!("Session ended (exit {}).", exit.code);
    Ok(if exit.success { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// The attach's shared memory: what the composer needs from the printer, plus
/// the latest-wins state whose re-emission must NOT scroll the transcript.
#[derive(Default)]
struct AttachState {
    /// The newest answerable question card, numbered exactly as printed.
    pending: Option<PendingQuestion>,
    /// The `/` names the AGENT advertised (`config_state.commands`) — the
    /// contract catalog is the other half of the composer's vocabulary.
    agent_commands: Vec<String>,
    /// `config_state`, `usage` and `diff` are LATEST-WINS state (D4), re-sent
    /// on a timer or on every change: print each only when it actually
    /// differs, or a 3 s diff tick alone would fill the screen.
    config_line: Option<String>,
    usage_line: Option<String>,
    diff_lines: Vec<String>,
    plan_lines: Vec<String>,
    /// Output lines already printed, per tool call ([`OUTPUT_LINE_CAP`]).
    output_lines: HashMap<String, usize>,
}

/// A question card the local attach can answer, with its options in the order
/// the printer numbered them. `keys` are the ACP option ids, never keystrokes.
struct PendingQuestion {
    id: String,
    ask_id: Option<String>,
    keys: Vec<String>,
}

fn print_event(event: &engine::LocalFeedEvent, state: &Mutex<AttachState>) {
    match event {
        engine::LocalFeedEvent::Activity { event, .. } => print_activity(event, state),
        engine::LocalFeedEvent::Output { tool_call_id, chunk, exit_code } => {
            print_output(tool_call_id, chunk, *exit_code, state)
        }
        engine::LocalFeedEvent::Plan { entries } => print_plan(entries, state),
        engine::LocalFeedEvent::Phase(phase) => match phase {
            engine::EnginePhase::Connecting => println!("Connecting to the agent..."),
            engine::EnginePhase::Live => println!("Connected."),
            // `attend_acp` prints the exit line itself once `wait` returns.
            engine::EnginePhase::Ended => {}
        },
        // Deliberately unrendered on a line printer: `ToolCall` repeats the
        // wire `tool` event this feed already carries, `EditDiff` is covered
        // by the worktree diff summary, and `Thought` is exactly the noise a
        // scrolling transcript should not carry (the session screen shows all
        // three as collapsible cards instead).
        engine::LocalFeedEvent::ToolCall { .. }
        | engine::LocalFeedEvent::EditDiff { .. }
        | engine::LocalFeedEvent::Thought { .. } => {}
    }
}

fn print_activity(event: &steer::ActivityEvent, state: &Mutex<AttachState>) {
    match event {
        steer::ActivityEvent::Narration { text, .. } => println!("{}", text.trim_end()),
        steer::ActivityEvent::Tool { name, detail, .. } => match detail {
            Some(detail) => println!("  · {name} {detail}"),
            None => println!("  · {name}"),
        },
        steer::ActivityEvent::UserMessage { text, .. } => println!("> {}", text.trim_end()),
        steer::ActivityEvent::Permission { tool, detail, .. } => match detail {
            Some(detail) => println!("  · waiting on permission: {tool} {detail}"),
            None => println!("  · waiting on permission: {tool}"),
        },
        steer::ActivityEvent::Subagent { agent_type, status, .. } => {
            let status = match status {
                steer::SubagentStatus::Started => "started",
                steer::SubagentStatus::Completed => "completed",
            };
            println!("  ⤷ {agent_type} {status}");
        }
        steer::ActivityEvent::Compaction { phase, .. } => match phase {
            steer::frames::CompactionPhase::Started => println!("── compacting context ──"),
            steer::frames::CompactionPhase::Ended => println!("── context compacted ──"),
        },
        steer::ActivityEvent::Diff { diff, .. } => {
            let lines = summarize_diff(diff);
            let mut state = lock(state);
            if state.diff_lines != lines {
                state.diff_lines = lines.clone();
                drop(state);
                for line in lines {
                    println!("  ± {line}");
                }
            }
        }
        steer::ActivityEvent::ConfigState { options, current_mode, commands, .. } => {
            let line = config_line(options, current_mode.as_deref());
            let mut state = lock(state);
            if let Some(commands) = commands {
                state.agent_commands = commands.iter().map(|c| c.name.clone()).collect();
            }
            if state.config_line.as_deref() != Some(line.as_str()) {
                state.config_line = Some(line.clone());
                drop(state);
                println!("[{line}]");
            }
        }
        steer::ActivityEvent::Usage { context_used, context_size, cost_usd, .. } => {
            let line = usage_line(*context_used, *context_size, *cost_usd);
            let mut state = lock(state);
            if state.usage_line.as_deref() != Some(line.as_str()) {
                state.usage_line = Some(line.clone());
                drop(state);
                println!("[{line}]");
            }
        }
        steer::ActivityEvent::Question { text, options, id, ask_id, .. } => {
            println!();
            println!("? {}", text.trim_end());
            // One option per line rather than a packed row: an ACP option
            // label is a whole sentence often enough that columns wrap into
            // an unreadable mess.
            for (index, option) in options.iter().enumerate() {
                println!("  {}) {}", index + 1, option.label);
            }
            println!("  (answer with a number)");
            let Some(id) = id.clone() else {
                // EXP-730: an id-less card is read-only everywhere.
                return;
            };
            lock(state).pending = Some(PendingQuestion {
                id,
                ask_id: ask_id.clone(),
                keys: options.iter().map(|option| option.key.clone()).collect(),
            });
        }
        steer::ActivityEvent::AnswerAck { .. } => println!("  ✓ answer sent"),
        steer::ActivityEvent::QuestionResolved { id, answers, dismissed, .. } => {
            match (answers, dismissed) {
                (Some(answers), _) if !answers.is_empty() => {
                    println!("  ✓ answered: {}", answers.join(", "));
                }
                (_, Some(true)) => println!("  · question dismissed"),
                _ => println!("  ✓ answered"),
            }
            let mut state = lock(state);
            let resolved = state
                .pending
                .as_ref()
                // No id resolves EVERY card of the ask (frames.rs).
                .is_some_and(|pending| id.is_none() || id.as_deref() == Some(&pending.id));
            if resolved {
                state.pending = None;
            }
        }
    }
}

fn print_output(
    tool_call_id: &str,
    chunk: &str,
    exit_code: Option<i32>,
    state: &Mutex<AttachState>,
) {
    let mut printed = Vec::new();
    {
        let mut state = lock(state);
        let seen = state.output_lines.entry(tool_call_id.to_string()).or_insert(0);
        for line in chunk.lines() {
            if *seen >= OUTPUT_LINE_CAP {
                if *seen == OUTPUT_LINE_CAP {
                    *seen += 1;
                    printed.push(format!("    ... output truncated after {OUTPUT_LINE_CAP} lines"));
                }
                break;
            }
            *seen += 1;
            printed.push(format!("    {line}"));
        }
    }
    for line in printed {
        println!("{line}");
    }
    if let Some(code) = exit_code {
        println!("    (exit {code})");
    }
}

fn print_plan(entries: &[engine::PlanEntryView], state: &Mutex<AttachState>) {
    let lines: Vec<String> = entries
        .iter()
        .map(|entry| {
            let mark = match entry.status {
                engine::PlanEntryStatusView::Completed => "x",
                engine::PlanEntryStatusView::InProgress => ">",
                engine::PlanEntryStatusView::Pending => " ",
            };
            format!("  [{mark}] {}", entry.content.trim_end())
        })
        .collect();
    let mut state = lock(state);
    if state.plan_lines == lines {
        return;
    }
    state.plan_lines = lines.clone();
    drop(state);
    println!("Plan:");
    for line in lines {
        println!("{line}");
    }
}

/// Route ONE composed line: an answer, a command, or a message. Mirrors what
/// a remote steerer's frames do, so the two paths cannot drift.
fn compose(session: &Arc<RunningSession>, state: &Mutex<AttachState>, line: String) {
    if line.trim().is_empty() {
        return;
    }
    {
        let mut state = lock(state);
        let answer = state
            .pending
            .as_ref()
            .and_then(|pending| {
                parse_answer_line(&line, &pending.keys).map(|key| steer::RemoteAnswer {
                    question_id: pending.id.clone(),
                    ask_id: pending.ask_id.clone(),
                    keys: vec![key],
                    text: None,
                })
            });
        if let Some(answer) = answer {
            // Locked until the engine acks (`answer_ack`) or resolves it.
            state.pending = None;
            drop(state);
            session.answer(answer);
            return;
        }
    }
    let agent_commands = lock(state).agent_commands.clone();
    if let Some((name, args)) = parse_slash_line(&line, session.agent, &agent_commands) {
        session.run_command(&name, &args);
        return;
    }
    session.send_prompt(line);
}

/// A bare number picks option N (1-based) of the pending card and returns its
/// ACP option id. Anything else — a number out of range, `2.` , prose — is not
/// an answer and falls through to the composer's other rules.
fn parse_answer_line(line: &str, keys: &[String]) -> Option<String> {
    let picked: usize = line.trim().parse().ok()?;
    keys.get(picked.checked_sub(1)?).cloned()
}

/// `/name` or `/name args` → the command sink, for a name the CONTRACT
/// catalog carries for this agent or one the agent advertised itself
/// (`config_state.commands`). Everything else — an unknown name, a path like
/// `/tmp/x` — is prose and rides the message path, exactly as
/// `steer::commands::parse_command` decides it for a remote steerer.
fn parse_slash_line(
    line: &str,
    agent: steer::SessionAgent,
    agent_commands: &[String],
) -> Option<(String, String)> {
    if let Some(parsed) = steer::commands::parse_command(line, agent) {
        return Some((parsed.command.name.to_string(), parsed.args));
    }
    let rest = line.trim().strip_prefix('/')?;
    let (head, tail) = match rest.find(char::is_whitespace) {
        Some(at) => (&rest[..at], rest[at..].trim()),
        None => (rest, ""),
    };
    let name = agent_commands
        .iter()
        .find(|command| command.eq_ignore_ascii_case(head))?;
    Some((name.clone(), tail.to_string()))
}

/// `model opus · effort high · mode plan` — the chip row as one line, in the
/// order the agent declared its options (the mode leads, like the composer
/// chips on every client).
fn config_line(options: &[steer::ConfigOption], current_mode: Option<&str>) -> String {
    let mut parts = Vec::new();
    if let Some(mode) = current_mode.filter(|mode| !mode.is_empty()) {
        parts.push(format!("mode {mode}"));
    }
    for option in options {
        let Some(value) = option.value.as_deref().filter(|value| !value.is_empty()) else {
            continue;
        };
        let label = option
            .values
            .as_ref()
            .and_then(|values| values.iter().find(|candidate| candidate.id == value))
            .map(|candidate| candidate.label.as_str())
            .unwrap_or(value);
        parts.push(format!("{} {label}", option.id));
    }
    if parts.is_empty() {
        return "no options".to_string();
    }
    parts.join(" · ")
}

/// `context 124k / 200k (62%)`, with ` · $1.24` when the run has cost half a
/// cent or more. A zero window means the agent has not reported one yet.
fn usage_line(context_used: i64, context_size: i64, cost_usd: Option<f64>) -> String {
    let mut line = if context_size > 0 {
        let percent = (context_used as f64 / context_size as f64 * 100.0).round() as i64;
        format!(
            "context {} / {} ({percent}%)",
            thousands(context_used),
            thousands(context_size)
        )
    } else {
        format!("context {}", thousands(context_used))
    };
    if let Some(cost) = cost_usd.filter(|cost| *cost >= 0.005) {
        line.push_str(&format!(" · ${cost:.2}"));
    }
    line
}

/// `124000` → `124k`. Tokens are counted in the thousands everywhere else in
/// the product; the exact figure is never the point.
fn thousands(value: i64) -> String {
    if value >= 1000 {
        format!("{}k", value / 1000)
    } else {
        value.to_string()
    }
}

/// A unified worktree diff → one `path (+added −removed)` line per file, in
/// the diff's own order. The full patch belongs on the session screen; a
/// scrolling transcript wants the shape of the change.
fn summarize_diff(diff: &str) -> Vec<String> {
    let mut files: Vec<(String, usize, usize)> = Vec::new();
    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ ") {
            let path = path.strip_prefix("b/").unwrap_or(path);
            if path != "/dev/null" {
                files.push((path.to_string(), 0, 0));
            }
            continue;
        }
        let Some(current) = files.last_mut() else { continue };
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if line.starts_with('+') {
            current.1 += 1;
        } else if line.starts_with('-') {
            current.2 += 1;
        }
    }
    files
        .into_iter()
        .map(|(path, added, removed)| format!("{path} (+{added} −{removed})"))
        .collect()
}

/// The attach's state lock is only ever held for a few field reads, so a
/// poisoned one is recovered rather than propagated — a panicking printer
/// thread must not take the composer down with it.
fn lock(state: &Mutex<AttachState>) -> std::sync::MutexGuard<'_, AttachState> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Detached wait: Ctrl-C / SIGTERM end the session cleanly (kill the child,
/// end the row) instead of orphaning a `running` badge.
pub fn wait_with_signals(session: &Arc<RunningSession>) -> CommandResult {
    crate::commands::daemon::install_signal_handler();
    loop {
        if crate::commands::daemon::shutdown_requested() {
            eprintln!("Stopping the session...");
            session.kill();
            let exit = session.wait();
            return Ok(if exit.success { ExitCode::SUCCESS } else { ExitCode::FAILURE });
        }
        if let Some(exit) = session.wait_timeout(Duration::from_millis(500)) {
            println!("Session ended (exit {}).", exit.code);
            return Ok(if exit.success { ExitCode::SUCCESS } else { ExitCode::FAILURE });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// EXP-746: the numbered card is the ONLY way to answer from a line
    /// attach, and it answers with the ACP option id the card carried — never
    /// a keystroke and never the number itself. Out-of-range picks, decorated
    /// numbers and prose are not answers: they fall through to the composer's
    /// other rules, so `2. no thanks` is a message, not a silent mis-answer.
    #[test]
    fn answer_line_parses_a_numbered_choice() {
        let keys = vec!["allow".to_string(), "allow_always".to_string(), "deny".to_string()];

        assert_eq!(parse_answer_line("1", &keys).as_deref(), Some("allow"));
        assert_eq!(parse_answer_line("  3  ", &keys).as_deref(), Some("deny"));

        assert_eq!(parse_answer_line("0", &keys), None);
        assert_eq!(parse_answer_line("4", &keys), None);
        assert_eq!(parse_answer_line("2.", &keys), None);
        assert_eq!(parse_answer_line("yes", &keys), None);
        assert_eq!(parse_answer_line("", &keys), None);
        assert_eq!(parse_answer_line("1", &[]), None);
    }

    /// EXP-746: `/` lines route to the command sink for a CONTRACT row of this
    /// agent or a command the agent advertised itself (`config_state.commands`)
    /// — the same union every client's `/` menu offers. Anything else stays
    /// prose, so a path never becomes a command.
    #[test]
    fn slash_line_routes_to_the_command_sink() {
        let agent = steer::SessionAgent::Claude;
        let advertised = vec!["review".to_string()];

        // A contract row, with and without arguments.
        let compact = steer::commands::catalog_for(agent)
            .first()
            .expect("claude has catalog commands")
            .name;
        assert_eq!(
            parse_slash_line(&format!("/{compact}"), agent, &advertised),
            Some((compact.to_string(), String::new()))
        );
        assert_eq!(
            parse_slash_line(&format!("/{compact} now"), agent, &advertised),
            Some((compact.to_string(), "now".to_string()))
        );

        // An agent-advertised command, case-insensitively.
        assert_eq!(
            parse_slash_line("/Review the diff", agent, &advertised),
            Some(("review".to_string(), "the diff".to_string()))
        );

        // Prose and paths are messages.
        assert_eq!(parse_slash_line("/tmp/build.log", agent, &advertised), None);
        assert_eq!(parse_slash_line("just do it", agent, &advertised), None);
        assert_eq!(parse_slash_line("/review", agent, &[]), None);
    }

    /// The chip row and the context line are LATEST-WINS state, so the printer
    /// compares the RENDERED line to decide whether to scroll the transcript —
    /// these two are what it compares.
    #[test]
    fn the_chip_and_context_lines_read_like_the_clients() {
        let options = vec![
            steer::ConfigOption {
                id: "model".to_string(),
                label: "Model".to_string(),
                category: Some("model".to_string()),
                value: Some("opus".to_string()),
                values: Some(vec![steer::ConfigValue {
                    id: "opus".to_string(),
                    label: "Opus".to_string(),
                }]),
            },
            steer::ConfigOption {
                id: "effort".to_string(),
                label: "Effort".to_string(),
                category: None,
                // No `values` to look the label up in — the raw id shows.
                value: Some("high".to_string()),
                values: None,
            },
            // An unset option is the CLI default and carries no chip.
            steer::ConfigOption::new("fast", "Fast"),
        ];
        assert_eq!(
            config_line(&options, Some("plan")),
            "mode plan · model Opus · effort high"
        );
        assert_eq!(config_line(&[], None), "no options");

        assert_eq!(usage_line(124_000, 200_000, None), "context 124k / 200k (62%)");
        assert_eq!(
            usage_line(124_000, 200_000, Some(1.239)),
            "context 124k / 200k (62%) · $1.24"
        );
        // Under half a cent is not worth a line (client parity), and a window
        // the agent has not reported yet is not a percentage of anything.
        assert_eq!(usage_line(10, 200_000, Some(0.004)), "context 10 / 200k (0%)");
        assert_eq!(usage_line(400, 0, None), "context 400");
    }

    /// The transcript shows the SHAPE of the worktree diff, one line per file;
    /// the patch itself belongs on the session screen.
    #[test]
    fn a_worktree_diff_summarizes_per_file() {
        let diff = "\
diff --git a/src/a.rs b/src/a.rs
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,2 +1,3 @@
 keep
-gone
+new
+also new
diff --git a/src/b.rs b/src/b.rs
--- /dev/null
+++ b/src/b.rs
@@ -0,0 +1 @@
+created
";
        assert_eq!(
            summarize_diff(diff),
            vec!["src/a.rs (+2 −1)".to_string(), "src/b.rs (+1 −0)".to_string()]
        );
        assert!(summarize_diff("").is_empty());
    }
}
