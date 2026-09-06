//! EXP-746 — the manual spike driver: run one agent through its wire and
//! print `agent frame → ACP update → ActivityEvent` for every frame. The table
//! it prints becomes the spike report posted on the issue.
//!
//! Owned by lane S1. It drives the WIRE modules directly rather than the
//! engine core, because the core's bodies land after this gate: the ACP and
//! ActivityEvent columns are the DOCUMENTED mapping (the plan's table), which
//! is exactly what the gate has to establish before anyone writes the mapper.
//!
//! ```sh
//! # every checkpoint of one agent, or one by name
//! cargo run -p engine --example exp746_acp_spike -- claude
//! cargo run -p engine --example exp746_acp_spike -- claude plan
//! cargo run -p engine --example exp746_acp_spike -- codex handshake
//! cargo run -p engine --example exp746_acp_spike -- pi handshake
//! ```
//!
//! Env: `EXP_SPIKE_DIR` (scratch worktree, default a temp dir),
//! `EXP_SPIKE_CAPTURE` (raw frames are appended per checkpoint when set),
//! `EXP_MCP_URL` + `EXP_MCP_TOKEN` (checkpoint `mcp`).
//!
//! A `claude` spawned from inside a claude session inherits
//! `CLAUDE_CODE_CHILD_SESSION`; run this under `env -u CLAUDE_CODE_CHILD_SESSION`.

use std::io::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use engine::adapters::claude_wire::{
    self, ClaudeArgs, ClaudeOut, ClaudeProcess, McpConfig, SystemSubtype, TurnEnd,
};
use engine::adapters::codex_wire::{self, AppServer, CodexMode, Incoming};
use engine::adapters::pi_wire::{self, PiArgs, PiOut, PiProcess};
use serde_json::{json, Value};
use terminal::pty::SpawnSpec;

fn main() {
    let mut args = std::env::args().skip(1);
    let agent = args.next().unwrap_or_default();
    let checkpoint = args.next().unwrap_or_else(|| "all".to_string());
    let outcome = match agent.as_str() {
        "claude" => run_claude(&checkpoint),
        "codex" => run_codex(&checkpoint),
        "pi" => run_pi(&checkpoint),
        _ => {
            eprintln!(
                "usage: cargo run -p engine --example exp746_acp_spike -- claude|codex|pi [checkpoint]"
            );
            std::process::exit(2);
        }
    };
    if !outcome {
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Printing: the three columns the gate exists to produce
// ---------------------------------------------------------------------------

/// One printed row. `acp` names the `SessionUpdate` variant (or the client
/// method) the frame becomes; `wire` names the relay `ActivityEvent` kind, or
/// `local:<item>` for a desktop-only feed item, or `-` for a frame that
/// produces neither.
struct Row {
    frame: String,
    acp: &'static str,
    wire: &'static str,
}

struct Capture {
    file: Option<std::fs::File>,
    rows: Vec<Row>,
    started: Instant,
    first_init: Option<Duration>,
    first_token: Option<Duration>,
}

impl Capture {
    fn new(agent: &str, checkpoint: &str) -> Capture {
        let file = std::env::var("EXP_SPIKE_CAPTURE").ok().and_then(|dir| {
            let dir = PathBuf::from(dir).join(agent);
            std::fs::create_dir_all(&dir).ok()?;
            std::fs::File::create(dir.join(format!("{checkpoint}.jsonl"))).ok()
        });
        Capture { file, rows: Vec::new(), started: Instant::now(), first_init: None, first_token: None }
    }

    /// Record one frame: the raw line (scrubbed) into the capture file, the
    /// mapping row into the table.
    fn record(&mut self, raw: &str, row: Row) {
        if let Some(file) = self.file.as_mut() {
            let _ = writeln!(file, "{}", scrub(raw));
        }
        println!("  {:<34} → {:<26} → {}", truncate(&row.frame, 34), row.acp, row.wire);
        self.rows.push(row);
    }

    fn note_init(&mut self) {
        self.first_init.get_or_insert(self.started.elapsed());
    }

    fn note_token(&mut self) {
        self.first_token.get_or_insert(self.started.elapsed());
    }

    fn summary(&self) {
        println!("  ── {} frames", self.rows.len());
        if let Some(init) = self.first_init {
            println!("  ── spawn → init: {} ms", init.as_millis());
        }
        if let Some(token) = self.first_token {
            println!("  ── spawn → first token: {} ms", token.as_millis());
        }
    }
}

/// Never let a credential reach a committed capture. The three shapes that
/// can appear in these streams: our own `expu_` personal keys, an
/// `Authorization: Bearer …` header echoed back in an MCP config, and a
/// provider `sk-…` key.
fn scrub(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    loop {
        let hit = ["expu_", "Bearer ", "sk-ant-", "sk-"]
            .iter()
            .filter_map(|needle| rest.find(needle).map(|at| (at, *needle)))
            .min_by_key(|(at, _)| *at);
        let Some((at, needle)) = hit else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..at]);
        out.push_str(needle);
        out.push_str("<redacted>");
        let tail = &rest[at + needle.len()..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(tail.len());
        rest = &tail[end..];
    }
}

fn truncate(text: &str, max: usize) -> String {
    let one_line = text.replace('\n', " ");
    if one_line.chars().count() <= max {
        return one_line;
    }
    one_line.chars().take(max - 1).collect::<String>() + "…"
}

fn header(agent: &str, checkpoint: &str, note: &str) {
    println!("\n=== {agent} · {checkpoint} — {note}");
}

fn scratch_dir(name: &str) -> PathBuf {
    let root = std::env::var("EXP_SPIKE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("exp746-spike"));
    let dir = root.join(name);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

// ---------------------------------------------------------------------------
// claude
// ---------------------------------------------------------------------------

/// The documented claude mapping (the plan's table, §"Mapping table"), one row
/// per frame. This function IS the gate's deliverable for claude.
fn claude_row(frame: &ClaudeOut, capture: &mut Capture) -> Row {
    let label = frame.label();
    let (acp, wire) = match frame {
        ClaudeOut::System(system) => match SystemSubtype::classify(&system.subtype) {
            SystemSubtype::Init => {
                capture.note_init();
                ("AvailableCommandsUpdate", "config_state")
            }
            SystemSubtype::Status => match system.status.as_deref() {
                Some("compacting") => ("CompactionUpdate{started}", "compaction{started}"),
                _ => ("(busy notice)", "-"),
            },
            SystemSubtype::CompactBoundary => ("CompactionUpdate", "compaction{ended} + usage"),
            SystemSubtype::SessionStateChanged => ("(turn settlement)", "-"),
            SystemSubtype::TaskStarted => ("(subagent edge)", "subagent{status:started}"),
            SystemSubtype::TaskNotification | SystemSubtype::TaskUpdated => {
                ("(subagent edge)", "subagent{status}")
            }
            SystemSubtype::CommandsChanged => ("AvailableCommandsUpdate", "config_state"),
            SystemSubtype::PermissionDenied => ("ToolCallUpdate{failed}", "-"),
            SystemSubtype::ModelRefusalFallback => ("ConfigOptionUpdate", "config_state"),
            SystemSubtype::Other => ("(none)", "-"),
        },
        ClaudeOut::Assistant(_) => {
            capture.note_token();
            ("AgentMessageChunk", "narration")
        }
        ClaudeOut::User(user) => {
            if user.tool_use_result.is_null() {
                ("UserMessageChunk", "user_message")
            } else {
                ("ToolCallUpdate", "local:ToolCard")
            }
        }
        ClaudeOut::StreamEvent(event) => match (event.event_type(), event.delta_type()) {
            ("content_block_delta", Some("text_delta")) => {
                capture.note_token();
                ("AgentMessageChunk", "narration (coalesced)")
            }
            ("content_block_delta", Some("thinking_delta")) => {
                ("AgentThoughtChunk", "narration (thought buffer)")
            }
            ("content_block_delta", Some("input_json_delta")) => {
                ("ToolCallUpdate", "local:ToolCard")
            }
            ("content_block_start", _) => {
                match event.event["content_block"]["type"].as_str().unwrap_or_default() {
                    "tool_use" => ("ToolCall", "tool"),
                    "thinking" => ("AgentThoughtChunk", "narration (thought buffer)"),
                    _ => {
                        capture.note_token();
                        ("AgentMessageChunk", "narration (coalesced)")
                    }
                }
            }
            ("message_start" | "message_delta", _) => ("UsageUpdate", "usage"),
            _ => ("(none)", "-"),
        },
        ClaudeOut::Result(_) => ("PromptResponse{StopReason}", "usage (+ idle)"),
        ClaudeOut::ControlRequest(request) => {
            if request.request.is_ask_user_question() {
                ("elicitation/create", "question (stepper)")
            } else if request.request.is_exit_plan_mode() {
                ("session/request_permission", "question{planMode}")
            } else if request.request.subtype() == "can_use_tool" {
                ("session/request_permission", "question")
            } else if request.request.subtype() == "hook_callback" {
                ("(internal: diff/plan)", "local:EditDiff")
            } else {
                ("(none)", "-")
            }
        }
        ClaudeOut::ControlResponse(_) => ("(request completion)", "-"),
        ClaudeOut::ControlCancelRequest(_) => ("(abort in flight)", "-"),
        ClaudeOut::KeepAlive => ("(dropped, never answered)", "-"),
        ClaudeOut::Unknown => ("(dropped)", "-"),
    };
    Row { frame: label, acp, wire }
}

struct ClaudeRun {
    args: Vec<String>,
    cwd: PathBuf,
}

fn claude_spec(run: &ClaudeRun) -> SpawnSpec {
    let mut spec = SpawnSpec::new("claude").args(run.args.clone()).cwd(run.cwd.clone());
    for (key, value) in claude_wire::extra_env() {
        spec = spec.env(key, value);
    }
    spec
}

/// Drive one claude turn, printing the mapping rows. Returns the `session_id`
/// seen on `system/init` and whether the turn produced a `result`.
fn claude_turn(
    capture: &mut Capture,
    process: &ClaudeProcess,
    prompt: &str,
    answer: Option<Value>,
    budget: Duration,
) -> (Option<String>, bool) {
    process.send_user(prompt, None).expect("stdin accepts the prompt");
    let mut session_id = None;
    let mut answered = 0usize;
    let end = process.pump_turn(Instant::now() + budget, |frame, raw| {
        let row = claude_row(frame, capture);
        capture.record(raw, row);
        if let ClaudeOut::System(system) = frame {
            if system.subtype == "init" && !system.session_id.is_empty() {
                session_id = Some(system.session_id.clone());
            }
        }
        if let ClaudeOut::ControlRequest(request) = frame {
            if request.request.subtype() == "can_use_tool" {
                let tool_use_id = request.request.tool_use_id.clone().unwrap_or_default();
                let payload = answer.clone().unwrap_or_else(|| {
                    claude_wire::permission_allow(&tool_use_id, request.request.input.clone())
                });
                answered += 1;
                println!(
                    "    ↳ answering can_use_tool for {} with {}",
                    request.request.tool_name,
                    payload.get("behavior").and_then(Value::as_str).unwrap_or("?")
                );
                return vec![claude_wire::control_response_success(&request.request_id, payload)];
            }
            if request.request.subtype() == "hook_callback" {
                return vec![claude_wire::control_response_success(
                    &request.request_id,
                    json!({}),
                )];
            }
        }
        Vec::new()
    });
    println!("  ── turn end: {end:?} ({answered} permission answers)");
    (session_id, end == TurnEnd::Result)
}

fn run_claude(checkpoint: &str) -> bool {
    let all = checkpoint == "all";
    let mut ok = true;
    if all || checkpoint == "hello" {
        ok &= claude_hello(true);
    }
    if all || checkpoint == "noprint" {
        ok &= claude_hello(false);
    }
    if all || checkpoint == "perm" {
        ok &= claude_permission();
    }
    if all || checkpoint == "plan" {
        ok &= claude_plan();
    }
    if all || checkpoint == "ask" {
        ok &= claude_ask();
    }
    if all || checkpoint == "effort" {
        ok &= claude_effort();
    }
    if all || checkpoint == "subagent" {
        ok &= claude_subagent();
    }
    if all || checkpoint == "mcp" {
        ok &= claude_mcp();
    }
    if all || checkpoint == "resume" {
        ok &= claude_resume();
    }
    ok
}

/// Checkpoints 1, 2, 9, 11: subscription login, `-p` vs no `-p` across TWO
/// prompts, cumulative usage, startup latency.
fn claude_hello(print_mode: bool) -> bool {
    let name = if print_mode { "hello (-p)" } else { "hello (no -p)" };
    header("claude", name, "two prompts on one stdin");
    let cwd = scratch_dir("claude-hello");
    let run = ClaudeRun {
        args: claude_argv_for(ClaudeArgs { print_mode, ..ClaudeArgs::default() }),
        cwd,
    };
    let mut capture = Capture::new("claude", if print_mode { "hello" } else { "hello-noprint" });
    let Ok(process) = ClaudeProcess::spawn(&claude_spec(&run)) else {
        println!("  !! claude did not spawn");
        return false;
    };
    let (session_id, first) =
        claude_turn(&mut capture, &process, "Reply with the single word ok.", None, Duration::from_secs(120));
    println!("  ── session_id: {}", session_id.clone().unwrap_or_else(|| "(none)".into()));
    // The whole question: does stdin survive the first turn?
    let (_, second) = claude_turn(
        &mut capture,
        &process,
        "Reply with the single word two.",
        None,
        Duration::from_secs(120),
    );
    capture.summary();
    println!("  ── first turn: {first}, second turn on the SAME stdin: {second}");
    first && second
}

/// Checkpoint 4: a permission request reaches the client over the
/// `--permission-prompt-tool stdio` lane and an allow resolves it — first in
/// the default mode (which always asks), then under `bypassPermissions`
/// (which is what a coding run uses) to record what still gets through.
fn claude_permission() -> bool {
    header("claude", "perm", "default mode vs bypassPermissions");
    let asked_default = claude_permission_run("perm-default", false);
    let asked_bypass = claude_permission_run("perm-bypass", true);
    println!("  ── can_use_tool in default mode: {asked_default}; under bypassPermissions: {asked_bypass}");
    asked_default
}

fn claude_permission_run(name: &str, bypass: bool) -> bool {
    let cwd = scratch_dir(name);
    let victim = cwd.join("x");
    let _ = std::fs::write(&victim, "spike\n");
    let args = if bypass {
        claude_argv_for(ClaudeArgs {
            permission_mode: Some("bypassPermissions"),
            allow_dangerous: true,
            ..ClaudeArgs::default()
        })
    } else {
        // `manual` IS the argv spelling of the control protocol's `default`
        // mode, and pinning it is load-bearing: with no flag the CLI takes the
        // user's own settings default, which on this machine is `auto` and
        // never asks at all.
        claude_argv_for(ClaudeArgs { permission_mode: Some("manual"), ..ClaudeArgs::default() })
    };
    let run = ClaudeRun { args, cwd };
    let mut capture = Capture::new("claude", name);
    let Ok(process) = ClaudeProcess::spawn(&claude_spec(&run)) else {
        return false;
    };
    let prompt = format!(
        "Run exactly this shell command and nothing else: rm -f {}",
        victim.display()
    );
    let (_, done) = claude_turn(&mut capture, &process, &prompt, None, Duration::from_secs(180));
    capture.summary();
    let asked = capture
        .rows
        .iter()
        .any(|row| row.frame.starts_with("control_request/can_use_tool"));
    println!(
        "  ── [{name}] turn completed: {done}; can_use_tool: {asked}; file gone: {}",
        !victim.exists()
    );
    asked
}

/// Checkpoint 3: plan mode, the ExitPlanMode 4-option menu, and what an allow
/// does to the turn.
fn claude_plan() -> bool {
    header("claude", "plan", "--permission-mode plan + ExitPlanMode");
    let cwd = scratch_dir("claude-plan");
    let _ = std::fs::write(cwd.join("note.txt"), "hello\n");
    let run = ClaudeRun {
        args: claude_argv_for(ClaudeArgs {
            permission_mode: Some("plan"),
            ..ClaudeArgs::default()
        }),
        cwd,
    };
    let mut capture = Capture::new("claude", "plan");
    let Ok(process) = ClaudeProcess::spawn(&claude_spec(&run)) else {
        return false;
    };
    let mut seen_exit_plan = false;
    process.send_user(
        "Plan (do not implement) a one-line change to note.txt that appends the word world. Keep the plan to one sentence, then call ExitPlanMode.",
        None,
    )
    .expect("stdin accepts the prompt");
    let end = process.pump_turn(Instant::now() + Duration::from_secs(240), |frame, raw| {
        let row = claude_row(frame, &mut capture);
        capture.record(raw, row);
        if let ClaudeOut::ControlRequest(request) = frame {
            if request.request.is_exit_plan_mode() {
                seen_exit_plan = true;
                println!(
                    "    ↳ ExitPlanMode input: {}",
                    truncate(&request.request.input.to_string(), 160)
                );
                let tool_use_id = request.request.tool_use_id.clone().unwrap_or_default();
                // "Yes, and use auto mode" = allow + a session setMode. The
                // clear-context options are a DENY with interrupt:true, which
                // the engine turns into a fresh-context re-prompt.
                let allow = json!({
                    "behavior": "allow",
                    "updatedInput": request.request.input.clone(),
                    "updatedPermissions": [
                        { "type": "setMode", "mode": "acceptEdits", "destination": "session" }
                    ],
                    "toolUseID": tool_use_id,
                    "decisionClassification": "user_permanent",
                });
                return vec![claude_wire::control_response_success(&request.request_id, allow)];
            }
            if request.request.subtype() == "can_use_tool" {
                let tool_use_id = request.request.tool_use_id.clone().unwrap_or_default();
                return vec![claude_wire::control_response_success(
                    &request.request_id,
                    claude_wire::permission_allow(&tool_use_id, request.request.input.clone()),
                )];
            }
        }
        Vec::new()
    });
    capture.summary();
    println!("  ── turn end: {end:?}; ExitPlanMode arrived: {seen_exit_plan}");
    seen_exit_plan
}

/// Checkpoint 5: AskUserQuestion under the chosen print mode.
fn claude_ask() -> bool {
    header("claude", "ask", "AskUserQuestion → elicitation shape");
    let cwd = scratch_dir("claude-ask");
    let run = ClaudeRun { args: claude_argv_for(ClaudeArgs::default()), cwd };
    let mut capture = Capture::new("claude", "ask");
    let Ok(process) = ClaudeProcess::spawn(&claude_spec(&run)) else {
        return false;
    };
    let mut asked = false;
    process
        .send_user(
            "Use the AskUserQuestion tool to ask me whether to use tabs or spaces. Ask exactly one question with those two options, then stop.",
            None,
        )
        .expect("stdin accepts the prompt");
    let end = process.pump_turn(Instant::now() + Duration::from_secs(240), |frame, raw| {
        let row = claude_row(frame, &mut capture);
        capture.record(raw, row);
        if let ClaudeOut::ControlRequest(request) = frame {
            if request.request.is_ask_user_question() {
                asked = true;
                println!(
                    "    ↳ AskUserQuestion input: {}",
                    truncate(&request.request.input.to_string(), 300)
                );
                // The answer rides `updatedInput.answers`, keyed by the
                // question TEXT (not the field key) — that is what the tool's
                // own call() reads.
                let mut updated = request.request.input.clone();
                let question = updated
                    .get("questions")
                    .and_then(Value::as_array)
                    .and_then(|questions| questions.first())
                    .and_then(|question| question.get("question"))
                    .and_then(Value::as_str)
                    .unwrap_or("question")
                    .to_string();
                if let Some(object) = updated.as_object_mut() {
                    object.insert("answers".to_string(), json!({ question: "spaces" }));
                }
                let tool_use_id = request.request.tool_use_id.clone().unwrap_or_default();
                return vec![claude_wire::control_response_success(
                    &request.request_id,
                    claude_wire::permission_allow(&tool_use_id, updated),
                )];
            }
            if request.request.subtype() == "can_use_tool" {
                let tool_use_id = request.request.tool_use_id.clone().unwrap_or_default();
                return vec![claude_wire::control_response_success(
                    &request.request_id,
                    claude_wire::permission_allow(&tool_use_id, request.request.input.clone()),
                )];
            }
        }
        Vec::new()
    });
    capture.summary();
    println!("  ── turn end: {end:?}; AskUserQuestion arrived: {asked}");
    asked
}

/// Checkpoint 7: `--effort ultracode` on argv, then a live switch.
fn claude_effort() -> bool {
    header("claude", "effort", "--effort ultracode + apply_flag_settings");
    let cwd = scratch_dir("claude-effort");
    let run = ClaudeRun {
        args: claude_argv_for(ClaudeArgs { effort: Some("ultracode"), ..ClaudeArgs::default() }),
        cwd,
    };
    let mut capture = Capture::new("claude", "effort");
    let Ok(process) = ClaudeProcess::spawn(&claude_spec(&run)) else {
        println!("  !! claude did not spawn (is --effort ultracode rejected?)");
        return false;
    };
    let mut effort_on_init = None;
    // A first prompt so the CLI emits system/init.
    process.send_user("Reply with the single word ok.", None).expect("stdin");
    let end = process.pump_turn(Instant::now() + Duration::from_secs(180), |frame, raw| {
        let row = claude_row(frame, &mut capture);
        capture.record(raw, row);
        if let ClaudeOut::System(system) = frame {
            if system.subtype == "init" {
                effort_on_init = system.effort.clone();
            }
        }
        Vec::new()
    });
    println!("  ── system/init.effort = {effort_on_init:?} (turn {end:?})");
    let request_id = claude_wire::new_request_id();
    process
        .send(&claude_wire::control_request(
            &request_id,
            claude_wire::apply_flag_settings(json!({ "effortLevel": "high" })),
        ))
        .expect("stdin accepts the control request");
    let switched = process.await_control_response(
        &request_id,
        Instant::now() + Duration::from_secs(60),
        |frame, raw| {
            let row = claude_row(frame, &mut capture);
            capture.record(raw, row);
        },
    );
    capture.summary();
    match switched {
        Some(response) => {
            println!("  ── apply_flag_settings → {} {}", response.subtype, response.response);
            response.is_success()
        }
        None => {
            println!("  !! apply_flag_settings never answered");
            false
        }
    }
}

/// Checkpoint 8: a Task fan-out, and whether `task_started.task_id` equals a
/// later `can_use_tool.agent_id`.
fn claude_subagent() -> bool {
    header("claude", "subagent", "Task fan-out → task_started / agent_id");
    let cwd = scratch_dir("claude-subagent");
    let _ = std::fs::write(cwd.join("a.txt"), "alpha\n");
    let _ = std::fs::write(cwd.join("b.txt"), "beta\n");
    // `manual`, not bypass: the invariant under test is that a permission
    // raised INSIDE the subagent carries that subagent's id, and bypass mode
    // raises none at all.
    let run = ClaudeRun {
        args: claude_argv_for(ClaudeArgs {
            permission_mode: Some("manual"),
            ..ClaudeArgs::default()
        }),
        cwd,
    };
    let mut capture = Capture::new("claude", "subagent");
    let Ok(process) = ClaudeProcess::spawn(&claude_spec(&run)) else {
        return false;
    };
    let mut task_ids: Vec<String> = Vec::new();
    let mut agent_ids: Vec<String> = Vec::new();
    process
        .send_user(
            "Use the Task tool to launch one subagent whose only job is to run the shell command `cat a.txt` with the Bash tool and report the output. Do not read the file yourself.",
            None,
        )
        .expect("stdin");
    let end = process.pump_turn(Instant::now() + Duration::from_secs(300), |frame, raw| {
        let row = claude_row(frame, &mut capture);
        capture.record(raw, row);
        if let ClaudeOut::System(system) = frame {
            if system.subtype == "task_started" {
                if let Some(task_id) = system.task_id.clone() {
                    task_ids.push(task_id);
                }
            }
        }
        if let ClaudeOut::ControlRequest(request) = frame {
            if let Some(agent_id) = request.request.agent_id.clone() {
                agent_ids.push(agent_id);
            }
            if request.request.subtype() == "can_use_tool" {
                let tool_use_id = request.request.tool_use_id.clone().unwrap_or_default();
                return vec![claude_wire::control_response_success(
                    &request.request_id,
                    claude_wire::permission_allow(&tool_use_id, request.request.input.clone()),
                )];
            }
        }
        Vec::new()
    });
    capture.summary();
    println!("  ── task_started ids: {task_ids:?}");
    println!("  ── can_use_tool agent_ids: {agent_ids:?}");
    println!(
        "  ── ids line up: {}",
        agent_ids.iter().all(|agent_id| task_ids.contains(agent_id)) && !agent_ids.is_empty()
    );
    end == TurnEnd::Result
}

/// Checkpoint 6: MCP wiring, both the `.exp-mcp.json` file and inline
/// `--mcp-config` JSON, against `EXP_MCP_URL`/`EXP_MCP_TOKEN`.
fn claude_mcp() -> bool {
    header("claude", "mcp", "--mcp-config file vs inline");
    let (Ok(url), Ok(token)) = (std::env::var("EXP_MCP_URL"), std::env::var("EXP_MCP_TOKEN"))
    else {
        println!("  ~~ skipped: set EXP_MCP_URL and EXP_MCP_TOKEN");
        return true;
    };
    let cwd = scratch_dir("claude-mcp");
    let inline = std::env::var("EXP_SPIKE_MCP_INLINE").is_ok();
    let document = json!({
        "mcpServers": {
            "exponential": {
                "type": "http",
                "url": url,
                "headers": {
                    // The inline variant is checkpoint #2: does the CLI expand
                    // an env ref inside a header value? If not, the file
                    // fallback (the literal key, 0600, git-ignored) stays.
                    "Authorization": if inline { "Bearer ${EXP_MCP_TOKEN}".to_string() }
                                     else { format!("Bearer {token}") },
                    // EXP-637's session header: proof that custom headers ride
                    // along on both variants.
                    "X-Exp-Session-Id": "spike-session",
                },
            },
        },
    });
    let path = cwd.join(".exp-mcp.json");
    let inline_json = document.to_string();
    let args = if inline {
        claude_argv_for(ClaudeArgs {
            mcp_config: Some(McpConfig::Inline(&inline_json)),
            strict_mcp_config: true,
            permission_mode: Some("bypassPermissions"),
            allow_dangerous: true,
            ..ClaudeArgs::default()
        })
    } else {
        let _ = std::fs::write(&path, format!("{document:#}\n"));
        claude_argv_for(ClaudeArgs {
            mcp_config: Some(McpConfig::File(&path)),
            strict_mcp_config: true,
            permission_mode: Some("bypassPermissions"),
            allow_dangerous: true,
            ..ClaudeArgs::default()
        })
    };
    let run = ClaudeRun { args, cwd };
    let mut spec = claude_spec(&run);
    spec = spec.env("EXP_MCP_TOKEN", token);
    let mut capture = Capture::new("claude", if inline { "mcp-inline" } else { "mcp-file" });
    let Ok(process) = ClaudeProcess::spawn(&spec) else {
        return false;
    };
    let mut servers = Value::Null;
    process
        .send_user(
            "List the teams I can see by calling the exponential MCP tool for teams, then reply with their names only.",
            None,
        )
        .expect("stdin");
    let end = process.pump_turn(Instant::now() + Duration::from_secs(240), |frame, raw| {
        let row = claude_row(frame, &mut capture);
        capture.record(raw, row);
        if let ClaudeOut::System(system) = frame {
            if system.subtype == "init" {
                servers = system.extra.get("mcp_servers").cloned().unwrap_or(Value::Null);
            }
        }
        if let ClaudeOut::ControlRequest(request) = frame {
            if request.request.subtype() == "can_use_tool" {
                let tool_use_id = request.request.tool_use_id.clone().unwrap_or_default();
                return vec![claude_wire::control_response_success(
                    &request.request_id,
                    claude_wire::permission_allow(&tool_use_id, request.request.input.clone()),
                )];
            }
        }
        Vec::new()
    });
    capture.summary();
    println!("  ── mcp_servers on init: {servers}");
    println!("  ── turn end: {end:?}");
    servers
        .as_array()
        .is_some_and(|servers| servers.iter().any(|server| server["status"] == json!("connected")))
}

/// Checkpoint 10: `--resume=<id>` reopens the transcript.
fn claude_resume() -> bool {
    header("claude", "resume", "--session-id then --resume");
    let cwd = scratch_dir("claude-resume");
    let session_id = uuid::Uuid::new_v4().to_string();
    let run = ClaudeRun {
        args: claude_argv_for(ClaudeArgs {
            session_id: Some(&session_id),
            ..ClaudeArgs::default()
        }),
        cwd: cwd.clone(),
    };
    let mut capture = Capture::new("claude", "resume");
    let Ok(process) = ClaudeProcess::spawn(&claude_spec(&run)) else {
        return false;
    };
    let (_, first) = claude_turn(
        &mut capture,
        &process,
        "Remember the word pineapple. Reply with the single word ok.",
        None,
        Duration::from_secs(120),
    );
    drop(process);
    let resumed = ClaudeRun {
        args: claude_argv_for(ClaudeArgs { resume: Some(&session_id), ..ClaudeArgs::default() }),
        cwd,
    };
    let Ok(process) = ClaudeProcess::spawn(&claude_spec(&resumed)) else {
        return false;
    };
    let (_, second) = claude_turn(
        &mut capture,
        &process,
        "What word did I ask you to remember? Reply with just that word.",
        None,
        Duration::from_secs(120),
    );
    capture.summary();
    let transcripts = claude_transcript_files(&session_id);
    println!("  ── transcripts under ~/.claude/projects: {transcripts:?}");
    first && second && !transcripts.is_empty()
}

/// Where the CLI persisted this session — the `session/list`/`session/load`
/// source. Every project dir is searched, not only the munged cwd: a worktree
/// can differ from where claude decided to persist.
fn claude_transcript_files(session_id: &str) -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME") else { return Vec::new() };
    let root = PathBuf::from(home).join(".claude").join("projects");
    let mut hits = Vec::new();
    let Ok(projects) = std::fs::read_dir(&root) else { return hits };
    for project in projects.flatten() {
        let candidate = project.path().join(format!("{session_id}.jsonl"));
        if candidate.exists() {
            hits.push(candidate);
        }
    }
    hits
}

fn claude_argv_for(args: ClaudeArgs<'_>) -> Vec<String> {
    claude_wire::claude_argv(&args)
}

// ---------------------------------------------------------------------------
// codex
// ---------------------------------------------------------------------------

fn codex_row(method: &str) -> Row {
    let (acp, wire) = match method {
        "thread/started" => ("(session/new response)", "-"),
        "turn/started" => ("(turn begin)", "-"),
        "turn/completed" | "turn/failed" => ("PromptResponse{StopReason}", "usage (+ idle)"),
        "item/started" => ("ToolCall", "tool"),
        "item/updated" => ("ToolCallUpdate", "local:ToolCard"),
        "item/completed" => ("ToolCallUpdate{completed}", "local:ToolCard"),
        "item/agentMessage/delta" | "item/agentMessage/deltaText" => {
            ("AgentMessageChunk", "narration (coalesced)")
        }
        "item/reasoning/delta" | "item/reasoning/deltaText" | "item/reasoning/deltaSummaryText" => {
            ("AgentThoughtChunk", "narration (thought buffer)")
        }
        "thread/tokenUsage/updated" => ("UsageUpdate", "usage"),
        "turn/plan/updated" => ("Plan", "local:Plan"),
        "thread/compacted" => ("CompactionUpdate", "compaction{ended}"),
        "account/rateLimits/updated" => ("(agent_usage windows)", "-"),
        "error" => ("(adapter error)", "narration"),
        _ => ("(none)", "-"),
    };
    Row { frame: method.to_string(), acp, wire }
}

/// Checkpoint 12: initialize/initialized, thread/start with the config MCP
/// block, a full-access turn, turn/interrupt, model/list, rate limits.
fn run_codex(checkpoint: &str) -> bool {
    if checkpoint == "interrupt" {
        return codex_interrupt();
    }
    header("codex", "handshake", "app-server initialize → thread → turn");
    let cwd = scratch_dir("codex");
    let spec = SpawnSpec::new("codex")
        .args(["app-server", "--listen", "stdio://"])
        .cwd(cwd.clone());
    let Ok((server, notifications, requests, exit)) = AppServer::spawn(&spec) else {
        println!("  !! codex app-server did not spawn");
        return false;
    };
    let mut capture = Capture::new("codex", "handshake");
    let started = Instant::now();
    let initialize = server.request_blocking(
        "initialize",
        codex_wire::initialize_params("0.14.32", codex_wire::OPT_OUT_NOTIFICATIONS),
        Duration::from_secs(60),
    );
    match &initialize {
        Ok(response) => println!("  ── initialize ok in {} ms: {}", started.elapsed().as_millis(), truncate(&response.to_string(), 160)),
        Err(error) => {
            println!("  !! initialize failed: {error}");
            return false;
        }
    }
    capture.note_init();
    let _ = server.notify("initialized", json!({}));

    let config = codex_wire::thread_config(
        std::env::var("EXP_MCP_URL").ok().as_deref(),
        "spike-session",
        std::slice::from_ref(&cwd),
    );
    let thread = server.request_blocking(
        "thread/start",
        codex_wire::thread_start_params(&cwd, config),
        Duration::from_secs(60),
    );
    let thread_id = match &thread {
        Ok(response) => {
            let id = response["thread"]["id"].as_str().unwrap_or_default().to_string();
            println!("  ── thread/start ok: {id}");
            id
        }
        Err(error) => {
            println!("  !! thread/start failed: {error}");
            return false;
        }
    };

    let turn = server.request_blocking(
        "turn/start",
        codex_wire::turn_start_params(
            &thread_id,
            "Reply with the single word ok.",
            None,
            None,
            CodexMode::AgentFullAccess,
            std::slice::from_ref(&cwd),
        ),
        Duration::from_secs(120),
    );
    let turn_id = match &turn {
        Ok(response) => response["turn"]["id"].as_str().unwrap_or_default().to_string(),
        Err(error) => {
            println!("  !! turn/start failed: {error}");
            String::new()
        }
    };
    println!("  ── turn/start → {turn_id}");

    // Drain notifications until the turn completes.
    let deadline = Instant::now() + Duration::from_secs(180);
    let mut completed = false;
    while Instant::now() < deadline && !completed {
        let Ok((method, params)) = notifications.recv_timeout(Duration::from_secs(5)) else {
            if let Ok(exit) = exit.try_recv() {
                println!("  !! app-server exited: {exit:?}");
                break;
            }
            continue;
        };
        if method.contains("delta") {
            capture.note_token();
        }
        let raw = json!({ "method": method, "params": params }).to_string();
        capture.record(&raw, codex_row(&method));
        if method == "turn/completed" || method == "turn/failed" {
            completed = true;
        }
        while let Ok(request) = requests.try_recv() {
            println!("    ↳ server request {} — answering approved", request.method);
            let _ = server.respond(request.id, json!({ "decision": "approved" }));
        }
    }
    capture.summary();

    // turn/interrupt on an already-completed turn should be a clean no-op
    // error, which is itself the check that the method exists and routes.
    if !turn_id.is_empty() {
        let interrupted = server.request_blocking(
            "turn/interrupt",
            codex_wire::turn_interrupt_params(&thread_id, &turn_id),
            Duration::from_secs(30),
        );
        println!("  ── turn/interrupt → {interrupted:?}");
    }
    let models = server.request_blocking(
        "model/list",
        codex_wire::model_list_params(None),
        Duration::from_secs(60),
    );
    match &models {
        Ok(response) => println!(
            "  ── model/list → {} models",
            response["data"].as_array().map(Vec::len).unwrap_or(0)
        ),
        Err(error) => println!("  !! model/list failed: {error}"),
    }
    let limits =
        server.request_blocking("account/rateLimits/read", json!({}), Duration::from_secs(60));
    match &limits {
        Ok(response) => println!("  ── account/rateLimits/read → {}", truncate(&response.to_string(), 200)),
        Err(error) => println!("  !! account/rateLimits/read failed: {error}"),
    }
    completed && models.is_ok()
}

// ---------------------------------------------------------------------------
// pi
// ---------------------------------------------------------------------------

fn pi_row(frame: &PiOut) -> Row {
    let label = frame.label();
    let (acp, wire) = match frame {
        PiOut::Response { .. } => ("(command completion)", "-"),
        PiOut::ExtensionUiRequest { method, .. } => match method.as_str() {
            "confirm" | "select" => ("session/request_permission", "question"),
            "input" | "editor" => ("elicitation/create", "question (stepper)"),
            _ => ("(none)", "-"),
        },
        PiOut::ExtensionError { .. } => ("(adapter error)", "narration"),
        PiOut::Event { kind, event } => match kind.as_str() {
            "message_update" => {
                match event["assistantMessageEvent"]["type"].as_str().unwrap_or_default() {
                    "text_delta" => ("AgentMessageChunk", "narration (coalesced)"),
                    "thinking_delta" => ("AgentThoughtChunk", "narration (thought buffer)"),
                    _ => ("(none)", "-"),
                }
            }
            "tool_execution_start" => ("ToolCall", "tool"),
            "tool_execution_update" => ("ToolCallUpdate", "local:CommandOutput"),
            "tool_execution_end" => ("ToolCallUpdate{status}", "local:ToolCard"),
            "turn_end" => ("UsageUpdate", "usage"),
            "agent_settled" => ("PromptResponse{EndTurn}", "- (idle)"),
            "compaction_start" => ("CompactionUpdate{started}", "compaction{started}"),
            "compaction_end" => ("CompactionUpdate{ended}", "compaction{ended}"),
            "thinking_level_changed" => ("ConfigOptionUpdate", "config_state"),
            "session_info_changed" => ("SessionInfoUpdate", "-"),
            _ => ("(none)", "-"),
        },
        PiOut::Unknown => ("(dropped)", "-"),
    };
    Row { frame: label, acp, wire }
}

/// Checkpoint 13: `--mode rpc` handshake, `get_state`, a prompt through to
/// `agent_settled`, `set_thinking_level`, `compact`.
fn run_pi(checkpoint: &str) -> bool {
    let all = checkpoint == "all";
    let mut ok = true;
    if all || checkpoint == "handshake" {
        ok &= pi_handshake();
    }
    if all || checkpoint == "plan" {
        ok &= pi_plan();
    }
    ok
}

fn pi_handshake() -> bool {
    header("pi", "handshake", "--mode rpc → get_state → prompt → agent_settled");
    let cwd = scratch_dir("pi");
    let args = pi_wire::pi_argv(&PiArgs::default());
    let spec = SpawnSpec::new("pi").args(args).cwd(cwd);
    let mut capture = Capture::new("pi", "handshake");
    let Ok(process) = PiProcess::spawn(&spec) else {
        println!("  !! pi did not spawn");
        return false;
    };

    process
        .send(&pi_wire::command("1", "get_state", json!({})))
        .expect("stdin accepts get_state");
    let mut state = Value::Null;
    let got_state = process.pump(
        Instant::now() + Duration::from_secs(60),
        |frame, raw| {
            capture.record(raw, pi_row(frame));
            if let PiOut::Response { command, data, .. } = frame {
                if command == "get_state" {
                    state = data.clone();
                }
            }
            Vec::new()
        },
        |frame| matches!(frame, PiOut::Response { command, .. } if command == "get_state"),
    );
    capture.note_init();
    println!("  ── get_state answered: {got_state} → {}", truncate(&state.to_string(), 200));

    process
        .send(&pi_wire::command("2", "prompt", json!({ "message": "Reply with the single word ok." })))
        .expect("stdin accepts prompt");
    let mut first_token = false;
    let settled = process.pump(
        Instant::now() + Duration::from_secs(180),
        |frame, raw| {
            if !first_token && matches!(frame, PiOut::Event { kind, .. } if kind == "message_update")
            {
                first_token = true;
                capture.note_token();
            }
            capture.record(raw, pi_row(frame));
            if let PiOut::ExtensionUiRequest { id, method, .. } = frame {
                let payload = match method.as_str() {
                    "confirm" => json!({ "confirmed": true }),
                    "select" => json!({ "value": "" }),
                    _ => json!({ "cancelled": true }),
                };
                return vec![pi_wire::extension_ui_response(id, payload)];
            }
            Vec::new()
        },
        |frame| matches!(frame, PiOut::Event { kind, .. } if kind == "agent_settled"),
    );
    println!("  ── agent_settled: {settled}");

    process
        .send(&pi_wire::command("3", "set_thinking_level", json!({ "level": "high" })))
        .expect("stdin accepts set_thinking_level");
    let thinking = process.pump(
        Instant::now() + Duration::from_secs(60),
        |frame, raw| {
            capture.record(raw, pi_row(frame));
            Vec::new()
        },
        |frame| matches!(frame, PiOut::Response { command, .. } if command == "set_thinking_level"),
    );
    println!("  ── set_thinking_level answered: {thinking}");

    process.send(&pi_wire::command("4", "compact", json!({}))).expect("stdin accepts compact");
    let compacted = process.pump(
        Instant::now() + Duration::from_secs(180),
        |frame, raw| {
            capture.record(raw, pi_row(frame));
            Vec::new()
        },
        |frame| {
            matches!(frame, PiOut::Event { kind, .. } if kind == "compaction_end")
                || matches!(frame, PiOut::Response { command, success: false, .. } if command == "compact")
        },
    );
    println!("  ── compact reached an end: {compacted}");
    capture.summary();
    got_state && settled
}

/// EXP-752 checkpoint: plan mode over `--mode rpc`. pi has no native modes, so
/// the whole mode is the launcher's `.exp-pi-plan.ts` extension: `-e` loads it
/// exactly as the TUI does, `/exp-plan on|off` sent as a PROMPT runs its
/// registered command (pi dispatches extension commands instead of prompting
/// the model), and its `ctx.ui.confirm` reaches us as an
/// `extension_ui_request` titled "Approve plan?".
fn pi_plan() -> bool {
    header(
        "pi",
        "plan",
        "-e .exp-pi-plan.ts → /exp-plan off|on → a write prompt → confirm",
    );
    let cwd = scratch_dir("pi-plan");
    // The same file the launcher writes into a worktree — the extension IS
    // the mode, so the spike drives the shipped source, never a copy.
    let Ok(plan) = coding::pi_bridge::write_pi_plan(&cwd) else {
        println!("  !! the plan extension could not be written into {}", cwd.display());
        return false;
    };
    println!("  ── plan extension at {}", plan.display());
    let extensions = vec![PathBuf::from(format!("./{}", coding::pi_bridge::PI_PLAN_FILE))];
    // `EXP_SPIKE_PI_MODEL` picks the model this checkpoint runs on: pi's own
    // default is whatever the machine last used, and a model the local
    // account cannot serve fails the turn before any tool is offered.
    let model = std::env::var("EXP_SPIKE_PI_MODEL").unwrap_or_default();
    let args = pi_wire::pi_argv(&PiArgs {
        model: (!model.is_empty()).then_some(model.as_str()),
        extensions: &extensions,
        ..PiArgs::default()
    });
    let spec = SpawnSpec::new("pi")
        .args(args)
        .cwd(cwd)
        // The extension's own gate: without it the file returns immediately.
        .env("EXP_PI_PLAN_MODE", "1");
    let mut capture = Capture::new("pi", "plan");
    let Ok(process) = PiProcess::spawn(&spec) else {
        println!("  !! pi did not spawn");
        return false;
    };

    // The command round trip, both ways: `off` is what a client's
    // `session/set_mode` sends, `on` puts the gate back so the write prompt
    // below actually meets it.
    let mut switched = true;
    for (id, argument) in [("1", "off"), ("2", "on")] {
        process
            .send(&pi_wire::command(
                id,
                "prompt",
                json!({ "message": format!("/{} {argument}", coding::pi_bridge::PI_PLAN_COMMAND) }),
            ))
            .expect("stdin accepts the plan command");
        let mut answer = Value::Null;
        let answered = process.pump(
            Instant::now() + Duration::from_secs(60),
            |frame, raw| {
                capture.record(raw, pi_row(frame));
                if let PiOut::Response { command, success, data, .. } = frame {
                    if command == "prompt" {
                        answer = json!({ "success": success, "data": data });
                    }
                }
                Vec::new()
            },
            |frame| matches!(frame, PiOut::Response { command, .. } if command == "prompt"),
        );
        println!("  ── /exp-plan {argument} answered: {answered} → {}", truncate(&answer.to_string(), 200));
        switched &= answered;
    }

    // With the gate active, a write request must reach the exit_plan_mode
    // tool and raise the extension's confirm dialog — the frame the adapter
    // turns into a SwitchMode permission card.
    process
        .send(&pi_wire::command(
            "3",
            "prompt",
            json!({
                "message": "Add a file called plan-check.txt containing the word ok. Do it now."
            }),
        ))
        .expect("stdin accepts prompt");
    let mut confirms = 0usize;
    let settled = process.pump(
        Instant::now() + Duration::from_secs(180),
        |frame, raw| {
            capture.record(raw, pi_row(frame));
            if let PiOut::ExtensionUiRequest { id, method, params } = frame {
                println!(
                    "  ── extension_ui_request {method}: {}",
                    truncate(&params.to_string(), 200)
                );
                if method == "confirm" {
                    confirms += 1;
                    return vec![pi_wire::extension_ui_response(id, json!({ "confirmed": true }))];
                }
                return vec![pi_wire::extension_ui_response(id, json!({ "cancelled": true }))];
            }
            Vec::new()
        },
        |frame| matches!(frame, PiOut::Event { kind, .. } if kind == "agent_settled"),
    );
    println!("  ── agent_settled: {settled}; confirm dialogs seen: {confirms}");
    capture.summary();
    switched && settled
}

// Silence the unused-import warning when a checkpoint set is compiled out.
#[allow(dead_code)]
fn _incoming_is_used(incoming: Incoming) -> bool {
    matches!(incoming, Incoming::Junk)
}

/// Checkpoint 12b: `turn/interrupt` on a LIVE turn, and the stale fence — a
/// completion still arrives, and every notification after the interrupt is
/// what the adapter must drop.
fn codex_interrupt() -> bool {
    header("codex", "interrupt", "interrupt a live turn");
    let cwd = scratch_dir("codex-interrupt");
    let spec = SpawnSpec::new("codex")
        .args(["app-server", "--listen", "stdio://"])
        .cwd(cwd.clone());
    let Ok((server, notifications, _requests, _exit)) = AppServer::spawn(&spec) else {
        println!("  !! codex app-server did not spawn");
        return false;
    };
    let mut capture = Capture::new("codex", "interrupt");
    if server
        .request_blocking(
            "initialize",
            codex_wire::initialize_params("0.14.32", codex_wire::OPT_OUT_NOTIFICATIONS),
            Duration::from_secs(60),
        )
        .is_err()
    {
        return false;
    }
    let _ = server.notify("initialized", json!({}));
    let thread = server.request_blocking(
        "thread/start",
        codex_wire::thread_start_params(&cwd, codex_wire::thread_config(None, "spike", std::slice::from_ref(&cwd))),
        Duration::from_secs(60),
    );
    let Ok(thread) = thread else { return false };
    let thread_id = thread["thread"]["id"].as_str().unwrap_or_default().to_string();
    let turn = server.request_blocking(
        "turn/start",
        codex_wire::turn_start_params(
            &thread_id,
            "Count slowly from 1 to 200, one number per line, with a short comment on each.",
            None,
            None,
            CodexMode::AgentFullAccess,
            std::slice::from_ref(&cwd),
        ),
        Duration::from_secs(120),
    );
    let Ok(turn) = turn else { return false };
    let turn_id = turn["turn"]["id"].as_str().unwrap_or_default().to_string();
    println!("  ── live turn {turn_id}");

    // Let it get going, then cut it off.
    let mut before = 0usize;
    let warmup = Instant::now() + Duration::from_secs(20);
    while Instant::now() < warmup {
        let Ok((method, params)) = notifications.recv_timeout(Duration::from_secs(2)) else {
            continue;
        };
        before += 1;
        capture.record(&json!({ "method": method, "params": params }).to_string(), codex_row(&method));
    }
    let interrupted = server.request_blocking(
        "turn/interrupt",
        codex_wire::turn_interrupt_params(&thread_id, &turn_id),
        Duration::from_secs(30),
    );
    println!("  ── turn/interrupt → {interrupted:?} (after {before} notifications)");

    // Everything from here is what the stale fence drops.
    let mut after = 0usize;
    let mut completed = None;
    let drain = Instant::now() + Duration::from_secs(20);
    while Instant::now() < drain {
        let Ok((method, params)) = notifications.recv_timeout(Duration::from_secs(2)) else {
            continue;
        };
        after += 1;
        if method == "turn/completed" || method == "turn/failed" {
            completed = Some(params["turn"]["status"].as_str().unwrap_or("?").to_string());
        }
        capture.record(&json!({ "method": method, "params": params }).to_string(), codex_row(&method));
    }
    capture.summary();
    println!("  ── notifications after the interrupt: {after}; turn status: {completed:?}");
    interrupted.is_ok()
}
