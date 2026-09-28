//! `exponential sessions …` (EXP-1110) — the coding runs of your teams, on
//! any device:
//!
//! - `sessions [list] [--device <label|id>] [--status s] [--all] [--limit n]`
//! - `sessions show <id>`: one run, its PR and its published screenshots
//! - `sessions log <id> [--follow]`: the transcript, over the steer relay
//! - `sessions message <id> <text…>` / `sessions kill <id>`
//!
//! Reads and the message/kill writes go through the instance's own MCP tools
//! (`api::mcp_tools`) — the same server paths an agent's
//! `exponential_sessions_*` calls take. Transcripts live ONLY on the host
//! device, so `log` is a steer VIEWER (`steer::spawn_viewer`, the desktop's
//! own client): the relay replays a live room, or asks the owning device to
//! republish an ended run's journal (`history_request`). Viewing a LIVE run
//! is owner-only (EXP-312) — the mint says no for anyone else.
//!
//! Every `<id>` may be a unique prefix of the session UUID (the list prints
//! eight characters).

use std::io::Read as _;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context as _};
use api::mcp_tools::{self, RemoteSession, SessionsQuery};

use super::{reject_unknown_flags, take_flag, take_value, CommandResult};
use crate::context::{self, Ctx};

const USAGE: &str = "\
Usage: exponential sessions [<command>]

  [list] [--device <label|id>] [--status running|in_review|ended] [--all] [--limit n]
                         Coding sessions, newest first (yours; --all adds teammates')
  show <id>              One session: status, device, branch, PR, screenshots
  log <id> [--follow]    Print the transcript (--follow keeps streaming a live run)
  message <id> <text…>   Send a message to a live session (`-` reads stdin)
  kill <id>              Stop a live session

<id> = the session UUID or a unique prefix of it.
";

const STATUSES: [&str; 3] = ["running", "in_review", "ended"];

pub fn run(args: &[String]) -> CommandResult {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[1.min(args.len())..];
    match sub {
        "" | "list" => list(rest),
        sub if sub.starts_with('-') => list(args),
        "show" | "get" => show(rest),
        "log" | "logs" => log(rest),
        "message" | "msg" => message(rest),
        "kill" | "stop" => kill(rest),
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        other => {
            eprintln!("Unknown sessions command `{other}`.\n");
            print!("{USAGE}");
            Ok(ExitCode::from(2))
        }
    }
}

/// Parsed `sessions list` flags (pure, unit-tested).
#[derive(Debug, Default, PartialEq)]
struct ListArgs {
    device: Option<String>,
    status: Option<String>,
    all: bool,
    limit: u32,
}

fn parse_list_args(args: &[String]) -> anyhow::Result<ListArgs> {
    let mut args = args.to_vec();
    let device = take_value(&mut args, "--device").filter(|value| !value.is_empty());
    let status = take_value(&mut args, "--status").filter(|value| !value.is_empty());
    let limit = match take_value(&mut args, "--limit") {
        Some(raw) => raw
            .parse::<u32>()
            .ok()
            .filter(|limit| (1..=mcp_tools::MAX_PAGE).contains(limit))
            .ok_or_else(|| anyhow!("--limit takes 1-{}", mcp_tools::MAX_PAGE))?,
        None => 50,
    };
    let all = take_flag(&mut args, "--all");
    reject_unknown_flags(&args)?;
    if let Some(extra) = args.first() {
        bail!("unexpected argument `{extra}`");
    }
    if let Some(status) = &status {
        if !STATUSES.contains(&status.as_str()) {
            bail!("--status takes running, in_review or ended (got `{status}`)");
        }
    }
    Ok(ListArgs {
        device,
        status,
        all,
        limit,
    })
}

fn list(args: &[String]) -> CommandResult {
    let parsed = parse_list_args(args)?;
    let ctx = context::load()?;
    // A device filter narrows AFTER the page: the tool has no device filter,
    // so a filtered list reads the widest page instead.
    let query = SessionsQuery {
        status: parsed.status.clone(),
        mine: !parsed.all,
        limit: if parsed.device.is_some() {
            mcp_tools::MAX_PAGE
        } else {
            parsed.limit
        },
        ..Default::default()
    };
    let mut rows = mcp_tools::sessions_list(&ctx.trpc, &query)
        .map_err(super::devices::tool_error)
        .context("list sessions")?;
    if let Some(selector) = &parsed.device {
        let devices = mcp_tools::devices_list(&ctx.trpc, None)
            .map_err(super::devices::tool_error)
            .context("list devices")?;
        rows = filter_by_device(rows, &devices, selector)?;
        rows.truncate(parsed.limit as usize);
    }
    if rows.is_empty() {
        println!("No sessions.");
        return Ok(ExitCode::SUCCESS);
    }
    print!("{}", render_sessions(&rows));
    Ok(ExitCode::SUCCESS)
}

/// Keep the rows of one device: one of the caller's own (label or id, via
/// the shared resolver), else — a teammate's shared server is not in the
/// caller's own list — a row whose recorded device id or label matches.
fn filter_by_device(
    rows: Vec<RemoteSession>,
    devices: &[api::mcp_tools::RemoteDevice],
    selector: &str,
) -> anyhow::Result<Vec<RemoteSession>> {
    if let Ok(device) = super::devices::resolve_device(devices, selector) {
        let id = device.device_id.clone();
        return Ok(rows
            .into_iter()
            .filter(|row| row.device_id.as_deref() == Some(id.as_str()))
            .collect());
    }
    let wanted = selector.trim().to_lowercase();
    let matched: Vec<RemoteSession> = rows
        .into_iter()
        .filter(|row| {
            row.device_id.as_deref() == Some(selector.trim())
                || row
                    .device_label
                    .as_deref()
                    .is_some_and(|label| label.trim().to_lowercase() == wanted)
        })
        .collect();
    if matched.is_empty() {
        // Surface the resolver's own miss (it lists the known devices).
        super::devices::resolve_device(devices, selector)?;
    }
    Ok(matched)
}

/// What a row is ABOUT: its issue, its action, or a batch.
pub fn subject(row: &RemoteSession) -> String {
    match (&row.issue_identifier, &row.action_name) {
        (Some(identifier), _) => match &row.issue_title {
            Some(title) if !title.is_empty() => format!("{identifier} {title}"),
            _ => identifier.clone(),
        },
        (None, Some(action)) => action.clone(),
        (None, None) if row.issue_id.is_none() => "batch run".to_string(),
        _ => "issue".to_string(),
    }
}

/// The status cell: the row status plus the two states a `running` row can
/// hide (EXP-804 usage wall, a question waiting on the person).
pub fn status_cell(row: &RemoteSession) -> String {
    let mut status = row.status.clone();
    if row.blocked.is_some() {
        status.push_str(" (usage wall)");
    } else if row.needs_input {
        status.push_str(" (needs input)");
    }
    status
}

/// ISO timestamp → local `YYYY-MM-DD HH:MM`; unparseable input passes through.
pub fn local_time(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|at| {
            at.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| iso.to_string())
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// The session table (pure, unit-tested). Nested runs indent by depth.
fn render_sessions(rows: &[RemoteSession]) -> String {
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            let indent = "  ".repeat(row.depth.min(4) as usize);
            vec![
                row.id.chars().take(8).collect(),
                status_cell(row),
                format!("{indent}{}", truncate(&subject(row), 48)),
                row.agent.clone().unwrap_or_default(),
                row.device_label.clone().unwrap_or_default(),
                row.started_at
                    .as_deref()
                    .map(local_time)
                    .unwrap_or_default(),
            ]
        })
        .collect();
    super::devices::render_table(
        &["ID", "STATUS", "SUBJECT", "AGENT", "DEVICE", "STARTED"],
        &cells,
    )
}

/// Resolve `<id>`: a full UUID as-is, else a unique prefix among the
/// sessions the caller can see (newest 200, teammates' included).
fn resolve_session_id(ctx: &Ctx, raw: &str) -> anyhow::Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("missing session id (see `exponential sessions`)");
    }
    if uuid::Uuid::parse_str(raw).is_ok() {
        return Ok(raw.to_string());
    }
    let query = SessionsQuery {
        mine: false,
        limit: mcp_tools::MAX_PAGE,
        ..Default::default()
    };
    let rows = mcp_tools::sessions_list(&ctx.trpc, &query)
        .map_err(super::devices::tool_error)
        .context("list sessions")?;
    match_prefix(&rows, raw)
}

fn match_prefix(rows: &[RemoteSession], prefix: &str) -> anyhow::Result<String> {
    if prefix.chars().count() < 4 {
        bail!("session id prefix `{prefix}` is too short (4+ characters)");
    }
    let hits: Vec<&RemoteSession> = rows
        .iter()
        .filter(|row| row.id.starts_with(prefix))
        .collect();
    match hits.as_slice() {
        [one] => Ok(one.id.clone()),
        [] => bail!("no session matches `{prefix}` (see `exponential sessions --all`)"),
        _ => bail!("session id prefix `{prefix}` is ambiguous — type more of it"),
    }
}

fn one_id(args: &[String], usage: &str) -> anyhow::Result<String> {
    reject_unknown_flags(args)?;
    match args {
        [id] => Ok(id.clone()),
        _ => bail!("usage: {usage}"),
    }
}

fn show(args: &[String]) -> CommandResult {
    let raw = one_id(args, "exponential sessions show <id>")?;
    let ctx = context::load()?;
    let id = resolve_session_id(&ctx, &raw)?;
    let row = mcp_tools::sessions_get(&ctx.trpc, &id)
        .map_err(super::devices::tool_error)
        .context("load the session")?;
    print!("{}", render_detail(&row));
    Ok(ExitCode::SUCCESS)
}

/// The `show` block, also the header `log` prints (pure, unit-tested).
fn render_detail(row: &RemoteSession) -> String {
    let mut out = String::new();
    let mut line = |key: &str, value: &str| out.push_str(&format!("{key:<10}{value}\n"));
    line("Session", &row.id);
    line("Subject", &subject(row));
    line("Status", &status_cell(row));
    if let Some(ended_by) = &row.ended_by {
        line("Ended by", ended_by);
    }
    if let Some(agent) = &row.agent {
        line("Agent", agent);
    }
    if let Some(device) = row.device_label.as_ref().or(row.device_id.as_ref()) {
        line("Device", device);
    }
    if let Some(branch) = &row.branch {
        line("Branch", branch);
    }
    if let Some(url) = &row.pr_url {
        let state = row
            .pr_state
            .as_deref()
            .map(|state| format!(" ({state})"))
            .unwrap_or_default();
        line("PR", &format!("{url}{state}"));
    }
    if let Some(blocked) = &row.blocked {
        let window = blocked
            .get("window")
            .and_then(|value| value.as_str())
            .unwrap_or("usage");
        let resets = blocked
            .get("resetsAt")
            .and_then(|value| value.as_str())
            .map(|at| format!(" until {}", local_time(at)))
            .unwrap_or_default();
        line("Blocked", &format!("{window} limit{resets}"));
    }
    if let Some(started) = &row.started_at {
        line("Started", &local_time(started));
    }
    if let Some(ended) = &row.ended_at {
        line("Ended", &local_time(ended));
    }
    for result in &row.results {
        line(
            "Result",
            &format!("{} / {}: {}", result.topic, result.label, result.url),
        );
    }
    out
}

fn message(args: &[String]) -> CommandResult {
    // No flag parsing: the text is free-form (a leading `-` is prose).
    let [raw, words @ ..] = args else {
        bail!("usage: exponential sessions message <id> <text…>");
    };
    let text = if words == ["-"] {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        text
    } else {
        words.join(" ")
    };
    let text = text.trim();
    if text.is_empty() {
        bail!("usage: exponential sessions message <id> <text…>");
    }
    let ctx = context::load()?;
    let id = resolve_session_id(&ctx, raw)?;
    mcp_tools::sessions_message(&ctx.trpc, &id, text)
        .map_err(super::devices::tool_error)
        .context("send the message")?;
    println!("Delivered to session {id}.");
    Ok(ExitCode::SUCCESS)
}

fn kill(args: &[String]) -> CommandResult {
    let raw = one_id(args, "exponential sessions kill <id>")?;
    let ctx = context::load()?;
    let id = resolve_session_id(&ctx, &raw)?;
    let killed = mcp_tools::sessions_kill(&ctx.trpc, &id)
        .map_err(super::devices::tool_error)
        .context("stop the session")?;
    println!("Session {id} stopped ({}).", killed.status);
    Ok(ExitCode::SUCCESS)
}

fn log(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let follow = take_flag(&mut args, "--follow") | take_flag(&mut args, "-f");
    let raw = one_id(&args, "exponential sessions log <id> [--follow]")?;
    let ctx = context::load()?;
    let id = resolve_session_id(&ctx, &raw)?;
    let row = mcp_tools::sessions_get(&ctx.trpc, &id)
        .map_err(super::devices::tool_error)
        .context("load the session")?;
    print!("{}", render_detail(&row));
    println!();
    stream_transcript(&ctx, &row, follow)
}

/// How long a non-follow `log` waits for the replay to complete. The relay
/// gives a republishing device 20s before it answers `history_unavailable`.
const REPLAY_BUDGET: Duration = Duration::from_secs(45);

/// Print a run's transcript through a steer viewer. Without `follow` it
/// stops once the replay is complete (`activity_synced`); with it, it keeps
/// printing a live run until the room ends (Ctrl-C to leave early). Also
/// the tail of `code`/`run --device --follow`.
pub fn stream_transcript(ctx: &Ctx, row: &RemoteSession, follow: bool) -> CommandResult {
    let runtime = steer::SteerRuntime::new().context("start the steer runtime")?;
    let (events_tx, events_rx) = flume::unbounded();
    let tickets = Arc::new(steer::TrpcViewerTickets {
        trpc: Arc::clone(&ctx.trpc),
        coding_session_id: row.id.clone(),
    });
    let handle = steer::spawn_viewer(&runtime, tickets, row.id.clone(), events_tx);
    let ended = row.status == "ended";
    if ended {
        handle.note_session_ended();
    }
    let state = Mutex::new(super::code::AttachState::default());
    let started = Instant::now();
    let mut max_seq = 0u64;
    let mut live = false;
    let mut said_waiting = false;
    let outcome = loop {
        if !follow && started.elapsed() > REPLAY_BUDGET {
            break Err(anyhow!(
                "no transcript arrived within {}s — the run's device may be offline",
                REPLAY_BUDGET.as_secs()
            ));
        }
        let event = match events_rx.recv_timeout(Duration::from_millis(500)) {
            Ok(event) => event,
            Err(flume::RecvTimeoutError::Timeout) => continue,
            Err(flume::RecvTimeoutError::Disconnected) => break Ok(ExitCode::SUCCESS),
        };
        match event {
            steer::ViewerEvent::Activity(seq, event) => {
                // A redial replays the room from the top: skip what printed.
                if seq > 0 && seq <= max_seq {
                    continue;
                }
                max_seq = max_seq.max(seq);
                super::code::print_activity(&event, &state);
            }
            steer::ViewerEvent::Synced { .. } => {
                if !follow || ended {
                    break Ok(ExitCode::SUCCESS);
                }
            }
            // A markerless republish ends on the next beat (EXP-656).
            steer::ViewerEvent::Keepalive if live && !follow && max_seq > 0 => {
                break Ok(ExitCode::SUCCESS);
            }
            steer::ViewerEvent::Phase(phase) => match phase {
                steer::ViewerPhase::Live => live = true,
                steer::ViewerPhase::Starting if !said_waiting => {
                    said_waiting = true;
                    eprintln!("(waiting for the run's device to publish...)");
                }
                steer::ViewerPhase::Ended { outcome } => {
                    break match outcome.as_deref() {
                        Some("device_offline") => Err(anyhow!(
                            "the device that holds this transcript is offline — transcripts live only on the run's device"
                        )),
                        Some("history_unavailable") => {
                            Err(anyhow!("the run's device had no transcript to send"))
                        }
                        _ => {
                            if follow && !ended {
                                println!("-- session ended --");
                            }
                            Ok(ExitCode::SUCCESS)
                        }
                    };
                }
                steer::ViewerPhase::Unauthorized { detail } => {
                    break Err(match detail {
                        Some(detail) => anyhow!("can't show this run's transcript: {detail}"),
                        None => anyhow!(
                            "the relay refused to show this run — a live run is visible only to its owner"
                        ),
                    });
                }
                _ => {}
            },
            _ => {}
        }
    };
    handle.shutdown();
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|arg| arg.to_string()).collect()
    }

    fn row(id: &str) -> RemoteSession {
        RemoteSession {
            id: id.to_string(),
            status: "running".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn list_flags_parse() {
        assert_eq!(
            parse_list_args(&args(&[
                "--device",
                "build box",
                "--status",
                "ended",
                "--all",
                "--limit=5"
            ]))
            .unwrap(),
            ListArgs {
                device: Some("build box".into()),
                status: Some("ended".into()),
                all: true,
                limit: 5,
            }
        );
        assert_eq!(parse_list_args(&[]).unwrap().limit, 50);
        assert!(parse_list_args(&args(&["--status", "done"])).is_err());
        assert!(parse_list_args(&args(&["--limit", "0"])).is_err());
        assert!(parse_list_args(&args(&["--bogus"])).is_err());
    }

    #[test]
    fn a_session_prefix_resolves_uniquely() {
        let rows = vec![
            row("1234abcd-0000"),
            row("1234ffff-0000"),
            row("9999aaaa-0000"),
        ];
        assert_eq!(match_prefix(&rows, "9999").unwrap(), "9999aaaa-0000");
        assert_eq!(match_prefix(&rows, "1234ab").unwrap(), "1234abcd-0000");
        assert!(match_prefix(&rows, "1234")
            .unwrap_err()
            .to_string()
            .contains("ambiguous"));
        assert!(match_prefix(&rows, "123")
            .unwrap_err()
            .to_string()
            .contains("too short"));
        assert!(match_prefix(&rows, "abcd")
            .unwrap_err()
            .to_string()
            .contains("no session"));
    }

    #[test]
    fn subjects_and_status_cells() {
        let mut issue = row("s1");
        issue.issue_id = Some("i1".into());
        issue.issue_identifier = Some("EXP-42".into());
        issue.issue_title = Some("Fix login".into());
        issue.needs_input = true;
        assert_eq!(subject(&issue), "EXP-42 Fix login");
        assert_eq!(status_cell(&issue), "running (needs input)");

        let mut action = row("s2");
        action.action_name = Some("Chat".into());
        action.blocked = Some(serde_json::json!({"window": "session"}));
        assert_eq!(subject(&action), "Chat");
        assert_eq!(status_cell(&action), "running (usage wall)");

        assert_eq!(subject(&row("s3")), "batch run");
    }

    #[test]
    fn the_session_table_indents_children() {
        let mut parent = row("aaaaaaaa-1111");
        parent.action_name = Some("Chat".into());
        parent.agent = Some("claude".into());
        parent.device_label = Some("box".into());
        let mut child = row("bbbbbbbb-2222");
        child.issue_id = Some("i".into());
        child.issue_identifier = Some("EXP-7".into());
        child.depth = 1;
        let table = render_sessions(&[parent, child]);
        let lines: Vec<&str> = table.lines().collect();
        assert!(
            lines[0].starts_with("ID        STATUS   SUBJECT"),
            "{table}"
        );
        assert!(lines[1].starts_with("aaaaaaaa  running  Chat"), "{table}");
        assert!(
            lines[2].starts_with("bbbbbbbb  running    EXP-7"),
            "{table}"
        );
    }

    #[test]
    fn a_device_filter_falls_back_to_the_rows_label() {
        let mut shared = row("s1");
        shared.device_id = Some("teammate-box".into());
        shared.device_label = Some("Team Box".into());
        let mine = row("s2");
        let kept = filter_by_device(vec![shared, mine], &[], "team box").unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "s1");
        assert!(filter_by_device(vec![row("s3")], &[], "nope").is_err());
    }

    #[test]
    fn the_detail_block_names_pr_and_results() {
        let mut detail = row("s1");
        detail.pr_url = Some("https://github.com/o/r/pull/9".into());
        detail.pr_state = Some("open".into());
        detail.results = vec![api::mcp_tools::SessionResult {
            topic: "Login".into(),
            label: "web".into(),
            url: "https://x/api/attachments/a".into(),
        }];
        let text = render_detail(&detail);
        assert!(
            text.contains("PR        https://github.com/o/r/pull/9 (open)\n"),
            "{text}"
        );
        assert!(
            text.contains("Result    Login / web: https://x/api/attachments/a\n"),
            "{text}"
        );
    }
}
