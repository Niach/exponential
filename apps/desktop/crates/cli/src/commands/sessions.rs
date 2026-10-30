//! `exponential sessions …` (EXP-1110) — the sessions of your teams, on any
//! device (EXP-1292: the `sessions` tree — threads, team agents and their
//! sub-sessions):
//!
//! - `sessions [list] [--device <label|id>] [--status s] [--agents] [--limit n]`
//! - `sessions show <id>`: one session, its device, branch and its thread's issues
//! - `sessions log <id> [--follow]`: the transcript, over eNet from its device
//! - `sessions message <id> <text…>` / `sessions stop <id>` / `sessions end <id>`
//!
//! Reads and the message/stop/end writes go through the instance's own MCP
//! v2 tools (`api::mcp_tools`) — the same server paths an agent's
//! `exponential_sessions_*` calls take. A message to an idle session wakes
//! the SAME row on its device. Transcripts live ONLY on the host
//! device, so `log` reads this machine's own journal when it holds one and
//! is otherwise a steer VIEWER over eNet (`steer::spawn_viewer`, the
//! desktop's own client; VAPP-43: the relay carries no session content): the
//! device answers with the live run or its stored journal, under the
//! session's access rule (`fixtures/steer-enet.json`).
//!
//! Every `<id>` may be a unique prefix of a root session's UUID (the list
//! prints eight characters; the list tool reads roots only).

use std::io::Read as _;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context as _};
use api::mcp_tools::{self, RemoteSession, RemoteSessionTree, SessionsQuery};

use super::{reject_unknown_flags, take_flag, take_value, CommandResult};
use crate::context::{self, Ctx};

const USAGE: &str = "\
Usage: exponential sessions [<command>]

  [list] [--device <label|id>] [--status queued|running|idle|ended] [--agents] [--limit n]
                         Threads you can see, newest first (--agents: the team agents)
  show <id>              One session: status, device, branch, its thread's issues
  log <id> [--follow]    Print the transcript (--follow keeps streaming a live session)
  message <id> <text…>   Message a session; an idle one wakes up (`-` reads stdin)
  stop <id>              Interrupt the session's current turn (it stays, idle)
  end <id>               End the session (and its sub-sessions)

<id> = the session UUID, or a unique prefix of a thread's or agent's.
";

const STATUSES: &[&str] = domain::contract::SESSION_STATUS_VALUES;

pub fn run(args: &[String]) -> CommandResult {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[1.min(args.len())..];
    match sub {
        "" | "list" => list(rest),
        sub if sub.starts_with('-') => list(args),
        "show" | "get" => show(rest),
        "log" | "logs" => log(rest),
        "message" | "msg" => message(rest),
        "stop" => stop(rest),
        "end" => end(rest),
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
    agents: bool,
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
    let agents = take_flag(&mut args, "--agents");
    reject_unknown_flags(&args)?;
    if let Some(extra) = args.first() {
        bail!("unexpected argument `{extra}`");
    }
    if let Some(status) = &status {
        if !STATUSES.contains(&status.as_str()) {
            bail!("--status takes {} (got `{status}`)", STATUSES.join(", "));
        }
    }
    Ok(ListArgs {
        device,
        status,
        agents,
        limit,
    })
}

fn list(args: &[String]) -> CommandResult {
    let parsed = parse_list_args(args)?;
    let ctx = context::load()?;
    // The tool filters on liveness only: a device or status filter narrows
    // AFTER the page, so a filtered list reads the widest page instead.
    let filtered = parsed.device.is_some() || parsed.status.is_some();
    let query = SessionsQuery {
        kind: parsed.agents.then(|| "agent".to_string()),
        live: parsed
            .status
            .as_deref()
            .is_some_and(|status| status != "ended")
            .then_some(true),
        limit: if filtered { mcp_tools::MAX_PAGE } else { parsed.limit },
        ..Default::default()
    };
    let mut rows = mcp_tools::sessions_list(&ctx.trpc, &query)
        .map_err(super::devices::tool_error)
        .context("list sessions")?;
    if let Some(status) = &parsed.status {
        rows.retain(|row| &row.status == status);
    }
    if let Some(selector) = &parsed.device {
        let devices = mcp_tools::devices_list(&ctx.trpc, None)
            .map_err(super::devices::tool_error)
            .context("list devices")?;
        rows = filter_by_device(rows, &devices, selector)?;
    }
    rows.truncate(parsed.limit as usize);
    if rows.is_empty() {
        println!("No sessions.");
        return Ok(ExitCode::SUCCESS);
    }
    print!("{}", render_sessions(&rows));
    Ok(ExitCode::SUCCESS)
}

/// Keep the rows of one device: one of the caller's own (label or id, via
/// the shared resolver), else — a teammate's shared server is not in the
/// caller's own list — a row whose recorded device id matches.
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
    let matched: Vec<RemoteSession> = rows
        .into_iter()
        .filter(|row| row.device_id.as_deref() == Some(selector.trim()))
        .collect();
    if matched.is_empty() {
        // Surface the resolver's own miss (it lists the known devices).
        super::devices::resolve_device(devices, selector)?;
    }
    Ok(matched)
}

/// What a row is ABOUT: its title (a list row's is the server's derived
/// one), else a tree root's derived title, else what it is (an agent's
/// conversation or a session). NEW-8: never the legacy `name`/`actionName`.
pub fn subject(row: &RemoteSession) -> String {
    let named = |value: &Option<String>| value.clone().filter(|value| !value.trim().is_empty());
    named(&row.title).or_else(|| named(&row.display_title)).unwrap_or_else(|| {
        if is_agent(row) { "agent" } else { "session" }.to_string()
    })
}

/// An agent's conversation session: the kind derives from `agent_id`.
fn is_agent(row: &RemoteSession) -> bool {
    domain::session_kind::session_kind_of(row.agent_id.as_deref()) == domain::contract::SESSION_KIND_AGENT
}

/// The status cell: the row status plus the states a live row can hide
/// (EXP-804 usage wall, a question waiting on the person, a turn running).
pub fn status_cell(row: &RemoteSession) -> String {
    let mut status = row.status.clone();
    if row.blocked.is_some() {
        status.push_str(" (usage wall)");
    } else if row.needs_input {
        status.push_str(" (needs input)");
    } else if row.busy && row.status != "ended" {
        status.push_str(" (working)");
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
                row.device_id.clone().unwrap_or_default(),
                row.created_at
                    .as_deref()
                    .map(local_time)
                    .unwrap_or_default(),
            ]
        })
        .collect();
    super::devices::render_table(&["ID", "STATUS", "SUBJECT", "DEVICE", "CREATED"], &cells)
}

/// Resolve `<id>`: a full UUID as-is, else a unique prefix among the roots
/// the caller can see (the newest 200 threads and the team agents).
fn resolve_session_id(ctx: &Ctx, raw: &str) -> anyhow::Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        bail!("missing session id (see `exponential sessions`)");
    }
    if uuid::Uuid::parse_str(raw).is_ok() {
        return Ok(raw.to_string());
    }
    let mut rows = Vec::new();
    for kind in ["thread", "agent"] {
        let query = SessionsQuery {
            kind: Some(kind.to_string()),
            limit: mcp_tools::MAX_PAGE,
            ..Default::default()
        };
        rows.extend(
            mcp_tools::sessions_list(&ctx.trpc, &query)
                .map_err(super::devices::tool_error)
                .context("list sessions")?,
        );
    }
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
        [] => bail!("no session matches `{prefix}` (see `exponential sessions`)"),
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
    let tree = mcp_tools::sessions_get(&ctx.trpc, &id)
        .map_err(super::devices::tool_error)
        .context("load the session")?;
    print!("{}", render_detail(&tree, &id));
    Ok(ExitCode::SUCCESS)
}

/// The `show` block, also the header `log` prints (pure, unit-tested):
/// the tree member `id` names (the root when it is not in the tree) and
/// its thread's issues.
fn render_detail(tree: &RemoteSessionTree, id: &str) -> String {
    let row = tree.session(id).unwrap_or(&tree.root);
    let mut out = String::new();
    let mut line = |key: &str, value: &str| out.push_str(&format!("{key:<10}{value}\n"));
    line("Session", &row.id);
    line("Subject", &subject(if row.parent_id.is_none() { row } else { &tree.root }));
    line("Status", &status_cell(row));
    if is_agent(row) {
        line("Kind", "team agent");
    }
    if let Some(parent) = &row.parent_id {
        line("Parent", parent);
    }
    if let Some(cli) = &row.cli {
        line("Agent", cli);
    }
    if let Some(account) = &row.account {
        line("Account", account);
    }
    if let Some(model) = &row.model {
        line("Model", model);
    }
    if let Some(device) = &row.device_id {
        line("Device", device);
    }
    if let Some(branch) = &row.branch {
        line("Branch", branch);
    }
    for issue in &tree.issues {
        let identifier = issue.identifier.as_deref().unwrap_or(&issue.issue_id);
        let text = match issue.title.as_deref().filter(|title| !title.is_empty()) {
            Some(title) => format!("{identifier} {title}"),
            None => identifier.to_string(),
        };
        line("Issue", &text);
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
    if let Some(last) = &row.last_turn_at {
        line("Last turn", &local_time(last));
    }
    if let Some(ended) = &row.ended_at {
        line("Ended", &local_time(ended));
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
    let input = mcp_tools::SessionsSendInput {
        session_id: Some(id.clone()),
        text: Some(text.to_string()),
        ..Default::default()
    };
    mcp_tools::sessions_send(&ctx.trpc, &input)
        .map_err(super::devices::tool_error)
        .context("send the message")?;
    println!("Queued for session {id}.");
    Ok(ExitCode::SUCCESS)
}

fn stop(args: &[String]) -> CommandResult {
    let raw = one_id(args, "exponential sessions stop <id>")?;
    let ctx = context::load()?;
    let id = resolve_session_id(&ctx, &raw)?;
    mcp_tools::sessions_stop(&ctx.trpc, &id)
        .map_err(super::devices::tool_error)
        .context("stop the session")?;
    println!("Session {id} stopped — it stays, idle; a message wakes it.");
    Ok(ExitCode::SUCCESS)
}

fn end(args: &[String]) -> CommandResult {
    let raw = one_id(args, "exponential sessions end <id>")?;
    let ctx = context::load()?;
    let id = resolve_session_id(&ctx, &raw)?;
    mcp_tools::sessions_end(&ctx.trpc, &id)
        .map_err(super::devices::tool_error)
        .context("end the session")?;
    println!("Session {id} ended.");
    Ok(ExitCode::SUCCESS)
}

fn log(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let follow = take_flag(&mut args, "--follow") | take_flag(&mut args, "-f");
    let raw = one_id(&args, "exponential sessions log <id> [--follow]")?;
    let ctx = context::load()?;
    let id = resolve_session_id(&ctx, &raw)?;
    let tree = mcp_tools::sessions_get(&ctx.trpc, &id)
        .map_err(super::devices::tool_error)
        .context("load the session")?;
    print!("{}", render_detail(&tree, &id));
    println!();
    stream_transcript(&ctx, tree.session(&id).unwrap_or(&tree.root), follow)
}

/// How long a non-follow `log` waits for the replay to complete.
const REPLAY_BUDGET: Duration = Duration::from_secs(45);

/// How long the viewer's eNet node may take to come up and load its first
/// allowlist.
const ENET_START_BUDGET: Duration = Duration::from_secs(15);

/// This CLI install's eNet node, for the viewer. It shares its identity with
/// the daemon (`cli-enet-identity`), so it refuses while the daemon runs:
/// two processes on one endpoint id would knock each other off the relay.
fn viewer_install(ctx: &Ctx, runtime: &steer::SteerRuntime, row: &RemoteSession) -> anyhow::Result<steer::enet::InstallNode> {
    use steer::enet::{InstallKind, InstallNode, InstallSpec};
    if let Some(pid) = super::daemon::daemon_pid(&ctx.data_dir) {
        bail!(
            "this session's transcript lives on its device and travels over eNet, whose identity the running daemon (pid {pid}) holds here — open the session in the desktop app, or stop the daemon and retry"
        );
    }
    let teams: Vec<String> = row.team_id.iter().cloned().collect();
    let install = InstallNode::start(
        runtime.handle(),
        InstallSpec {
            kind: InstallKind::Daemon,
            user_id: ctx.account.user_id.clone(),
            device_id: Some(ctx.device_id()),
            label: Some(api::users::hostname()),
            identity_path: steer::enet::identity_path(&ctx.data_dir, &ctx.account.id, InstallKind::Daemon),
            net: steer::enet::EnetConfig::default(),
        },
        Arc::clone(&ctx.trpc) as Arc<dyn steer::enet::InstallRegistry>,
        Arc::new(move || teams.clone()),
    );
    if !install.wait_passes_blocking(1, ENET_START_BUDGET) {
        log::debug!("sessions log: the eNet node is not ready yet; joining anyway");
    }
    Ok(install)
}

/// Print a run's transcript. This machine's own journal needs no network
/// (an ended run, or no `follow`); otherwise a steer viewer over eNet:
/// without `follow` it stops once the replay is complete
/// (`activity_synced`); with it, it keeps printing a live run until it ends
/// (Ctrl-C to leave early). Also the tail of `code`/`run --device --follow`.
pub fn stream_transcript(ctx: &Ctx, row: &RemoteSession, follow: bool) -> CommandResult {
    let ended = row.status == "ended";
    if !follow || ended {
        if let Some(events) = steer::read_journal(&ctx.data_dir, &row.id).filter(|events| !events.is_empty()) {
            let state = Mutex::new(super::code::AttachState::default());
            for event in &events {
                super::code::print_activity(event, &state);
            }
            return Ok(ExitCode::SUCCESS);
        }
    }
    let runtime = steer::SteerRuntime::new().context("start the steer runtime")?;
    let install = viewer_install(ctx, &runtime, row)?;
    let (events_tx, events_rx) = flume::unbounded();
    let enet = Arc::new(steer::InstallViewerSource { install: install.clone(), trpc: Arc::clone(&ctx.trpc) });
    let handle = steer::spawn_viewer(&runtime, Some(enet), row.id.clone(), events_tx);
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
                "no transcript arrived within {}s — the session's device may be offline",
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
                    eprintln!("(waiting for the session's device to publish...)");
                }
                steer::ViewerPhase::Ended { outcome } => {
                    break match outcome.as_deref() {
                        Some("device_offline") => Err(anyhow!(
                            "the device that holds this transcript is offline — transcripts live only on the session's device"
                        )),
                        _ => {
                            if follow && !ended {
                                println!("-- session ended --");
                            }
                            Ok(ExitCode::SUCCESS)
                        }
                    };
                }
                steer::ViewerPhase::Unauthorized { detail } => {
                    break Err(anyhow!(
                        "can't show this session's transcript: {}",
                        detail.as_deref().unwrap_or("refused")
                    ));
                }
                _ => {}
            },
            _ => {}
        }
    };
    handle.shutdown();
    install.stop();
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
                "--agents",
                "--limit=5"
            ]))
            .unwrap(),
            ListArgs {
                device: Some("build box".into()),
                status: Some("ended".into()),
                agents: true,
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

    fn issue(identifier: &str, title: &str) -> api::mcp_tools::RemoteSessionIssue {
        api::mcp_tools::RemoteSessionIssue {
            issue_id: format!("id-{identifier}"),
            identifier: Some(identifier.to_string()),
            title: Some(title.to_string()),
        }
    }

    #[test]
    fn subjects_and_status_cells() {
        let mut thread = row("s1");
        thread.title = Some("Fix login".into());
        thread.needs_input = true;
        assert_eq!(subject(&thread), "Fix login");
        assert_eq!(status_cell(&thread), "running (needs input)");

        let mut tree_root = row("s2");
        tree_root.display_title = Some("Cleanup".into());
        tree_root.blocked = Some(serde_json::json!({"window": "session"}));
        assert_eq!(subject(&tree_root), "Cleanup");
        assert_eq!(status_cell(&tree_root), "running (usage wall)");

        let mut agent = row("s3");
        agent.agent_id = Some("ag-1".into());
        agent.title = Some("Triage bot".into());
        agent.status = "idle".into();
        assert_eq!(subject(&agent), "Triage bot");
        assert_eq!(status_cell(&agent), "idle");

        let mut busy = row("s4");
        busy.busy = true;
        assert_eq!(subject(&busy), "session");
        assert_eq!(status_cell(&busy), "running (working)");
    }

    #[test]
    fn the_session_table_indents_children() {
        let mut parent = row("aaaaaaaa-1111");
        parent.title = Some("Chat".into());
        parent.device_id = Some("box".into());
        let mut child = row("bbbbbbbb-2222");
        child.title = Some("EXP-7".into());
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
    fn a_device_filter_falls_back_to_the_rows_device_id() {
        let mut shared = row("s1");
        shared.device_id = Some("teammate-box".into());
        let mine = row("s2");
        let kept = filter_by_device(vec![shared, mine], &[], "teammate-box").unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "s1");
        assert!(filter_by_device(vec![row("s3")], &[], "nope").is_err());
    }

    #[test]
    fn the_detail_block_names_the_runner_and_the_threads_issues() {
        let mut root = row("r1");
        root.display_title = Some("Fix login".into());
        let mut child = row("s1");
        child.cli = Some("codex".into());
        child.branch = Some("exp/EXP-42".into());
        child.parent_id = Some("r1".into());
        let tree = RemoteSessionTree {
            root,
            children: vec![child],
            issues: vec![issue("EXP-42", "Fix login")],
        };
        let text = render_detail(&tree, "s1");
        assert!(text.contains("Session   s1\n"), "{text}");
        assert!(text.contains("Subject   Fix login\n"), "{text}");
        assert!(text.contains("Agent     codex\n"), "{text}");
        assert!(text.contains("Branch    exp/EXP-42\n"), "{text}");
        assert!(text.contains("Parent    r1\n"), "{text}");
        assert!(text.contains("Issue     EXP-42 Fix login\n"), "{text}");
        // An id outside the tree shows the root.
        assert!(render_detail(&tree, "zz").contains("Session   r1\n"));
    }

    /// The `--status` filter takes exactly the contract's session statuses.
    #[test]
    fn the_status_filter_speaks_the_v2_statuses() {
        for status in ["queued", "running", "idle", "ended"] {
            assert!(parse_list_args(&args(&["--status", status])).is_ok(), "{status}");
        }
        assert!(parse_list_args(&args(&["--status", "in_review"])).is_err());
    }
}
