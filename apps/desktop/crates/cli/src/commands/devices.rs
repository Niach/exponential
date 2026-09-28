//! `exponential devices` (EXP-1110) — the machines this account can start
//! runs on: every registered desktop app and CLI daemon of yours (plus the
//! servers teammates shared with `--team`), with the SAME online rule every
//! client applies (`last_seen_at` freshness, server-computed).
//!
//! Also the shared `--device <label|id>` resolver behind `code`, `run` and
//! `sessions`.

use std::collections::HashMap;
use std::process::ExitCode;

use anyhow::{bail, Context as _};
use api::mcp_tools::{self, RemoteDevice, RemoteSession, SessionsQuery};

use super::{reject_unknown_flags, take_value, CommandResult};
use crate::context::{self, Ctx};

pub fn run(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let team = take_value(&mut args, "--team").filter(|team| !team.is_empty());
    reject_unknown_flags(&args)?;
    if let Some(extra) = args.first() {
        bail!("unexpected argument `{extra}` (usage: exponential devices [--team <team-id>])");
    }
    let ctx = context::load()?;
    let devices = mcp_tools::devices_list(&ctx.trpc, team.as_deref())
        .map_err(tool_error)
        .context("list devices")?;
    // Running counts come off the session rows (teammates' runs on a shared
    // server included), not a device field: the list tool reports none.
    let running = running_sessions(&ctx)
        .map(|rows| running_counts(&rows))
        .unwrap_or_default();
    if devices.is_empty() {
        println!("No devices registered yet.");
    } else {
        print!("{}", render_devices(&devices, &running, &ctx.device_id()));
    }
    if !devices
        .iter()
        .any(|device| device.device_id == ctx.device_id())
    {
        println!();
        println!("{}", super::account::NOT_REGISTERED_HINT);
    }
    Ok(ExitCode::SUCCESS)
}

/// An MCP tool's refusal reads as its own sentence ("Session is not live"),
/// never `HTTP 400: …`; the 426/401 variants stay typed so `main` still
/// recognises the upgrade gate.
pub fn tool_error(err: api::ApiError) -> anyhow::Error {
    match err {
        api::ApiError::Http { .. } | api::ApiError::Transport { .. } => {
            anyhow::anyhow!(err.user_message())
        }
        other => other.into(),
    }
}

/// Every live run the caller can see (own + teammates'), newest first.
pub fn running_sessions(ctx: &Ctx) -> anyhow::Result<Vec<RemoteSession>> {
    let query = SessionsQuery {
        status: Some("running".to_string()),
        mine: false,
        limit: mcp_tools::MAX_PAGE,
        ..Default::default()
    };
    mcp_tools::sessions_list(&ctx.trpc, &query).map_err(tool_error)
}

/// Live runs per device id.
pub fn running_counts(rows: &[RemoteSession]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for row in rows {
        if let Some(device) = &row.device_id {
            *counts.entry(device.clone()).or_insert(0) += 1;
        }
    }
    counts
}

/// The device table (pure, unit-tested). `*` marks the default device.
pub fn render_devices(
    devices: &[RemoteDevice],
    running: &HashMap<String, usize>,
    this_device_id: &str,
) -> String {
    let rows: Vec<Vec<String>> = devices
        .iter()
        .map(|device| {
            let mut label = display_label(device);
            if device.is_default {
                label.push_str(" *");
            }
            if device.device_id == this_device_id {
                label.push_str(" (this machine)");
            }
            if let Some(owner) = &device.owner {
                label.push_str(&format!(" [{}]", owner.name));
            }
            let kind = match device.kind.as_str() {
                "server" => "cli",
                _ => "desktop",
            };
            vec![
                label,
                kind.to_string(),
                if device.online { "online" } else { "offline" }.to_string(),
                agents_cell(device),
                running
                    .get(&device.device_id)
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
                device.device_id.clone(),
            ]
        })
        .collect();
    render_table(
        &["DEVICE", "KIND", "STATUS", "AGENTS", "RUNNING", "ID"],
        &rows,
    )
}

fn display_label(device: &RemoteDevice) -> String {
    if device.label.trim().is_empty() {
        device.device_id.clone()
    } else {
        device.label.clone()
    }
}

fn agents_cell(device: &RemoteDevice) -> String {
    let mut parts: Vec<String> = device.agents.clone();
    parts.extend(
        device
            .unauthed_agents
            .iter()
            .map(|agent| format!("{agent} (signed out)")),
    );
    if parts.is_empty() {
        "none".to_string()
    } else {
        parts.join(", ")
    }
}

/// Left-aligned columns, two spaces apart, the header first. Widths count
/// chars (labels may carry non-ASCII).
pub fn render_table(headers: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = headers
        .iter()
        .map(|header| header.chars().count())
        .collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if let Some(width) = widths.get_mut(index) {
                *width = (*width).max(cell.chars().count());
            }
        }
    }
    let line = |cells: Vec<&str>| {
        let last = cells.len().saturating_sub(1);
        let mut out = String::new();
        for (index, cell) in cells.iter().enumerate() {
            out.push_str(cell);
            if index < last {
                let pad = widths[index].saturating_sub(cell.chars().count()) + 2;
                out.push_str(&" ".repeat(pad));
            }
        }
        out.push('\n');
        out
    };
    let mut out = line(headers.to_vec());
    for row in rows {
        out.push_str(&line(row.iter().map(String::as_str).collect()));
    }
    out
}

/// Resolve `--device <label|id>`: an exact id, else a case-insensitive exact
/// label, else a unique id prefix (4+ chars). Ambiguity and a miss are errors
/// that list the candidates.
pub fn resolve_device<'a>(
    devices: &'a [RemoteDevice],
    selector: &str,
) -> anyhow::Result<&'a RemoteDevice> {
    let selector = selector.trim();
    if selector.is_empty() {
        bail!("--device needs a device label or id (see `exponential devices`)");
    }
    if let Some(device) = devices.iter().find(|device| device.device_id == selector) {
        return Ok(device);
    }
    let wanted = selector.to_lowercase();
    let by_label: Vec<&RemoteDevice> = devices
        .iter()
        .filter(|device| device.label.trim().to_lowercase() == wanted)
        .collect();
    match by_label.as_slice() {
        [one] => return Ok(one),
        [] => {}
        many => {
            let ids = many
                .iter()
                .map(|device| format!("  {}", device.device_id))
                .collect::<Vec<_>>()
                .join("\n");
            bail!("several devices are labelled `{selector}` — pass the id instead:\n{ids}");
        }
    }
    if selector.chars().count() >= 4 {
        let by_prefix: Vec<&RemoteDevice> = devices
            .iter()
            .filter(|device| device.device_id.starts_with(selector))
            .collect();
        if let [one] = by_prefix.as_slice() {
            return Ok(one);
        }
    }
    let known = if devices.is_empty() {
        "no devices are registered".to_string()
    } else {
        format!(
            "known devices: {}",
            devices
                .iter()
                .map(display_label)
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    bail!("no device matches `{selector}` ({known}; see `exponential devices`)")
}

/// [`resolve_device`] plus the start preconditions a remote start checks
/// first: the device is online and (when asked for) runs that agent. The
/// server re-checks both; this just says it before a round trip.
pub fn resolve_start_target<'a>(
    devices: &'a [RemoteDevice],
    selector: &str,
    agent: Option<&str>,
) -> anyhow::Result<&'a RemoteDevice> {
    let device = resolve_device(devices, selector)?;
    if !device.online {
        let seen = device
            .last_seen_at
            .as_deref()
            .map(|at| format!(", last seen {}", super::sessions::local_time(at)))
            .unwrap_or_default();
        bail!(
            "{} is offline{seen}. A start aimed at an offline device is refused; start its app or `exponential daemon` first.",
            display_label(device)
        );
    }
    if let Some(agent) = agent {
        if !device.agents.iter().any(|installed| installed == agent) {
            let installed = if device.agents.is_empty() {
                "none".to_string()
            } else {
                device.agents.join(", ")
            };
            bail!(
                "{} cannot run {agent} (its agents: {installed}).",
                display_label(device)
            );
        }
    }
    Ok(device)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, label: &str, online: bool) -> RemoteDevice {
        RemoteDevice {
            device_id: id.to_string(),
            label: label.to_string(),
            kind: "server".to_string(),
            online,
            agents: vec!["claude".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn a_device_resolves_by_id_label_or_prefix() {
        let devices = vec![
            device("cli-1111aaaa", "Build Box", true),
            device("cli-2222bbbb", "laptop", false),
        ];
        assert_eq!(
            resolve_device(&devices, "cli-2222bbbb").unwrap().label,
            "laptop"
        );
        assert_eq!(
            resolve_device(&devices, "build box").unwrap().device_id,
            "cli-1111aaaa"
        );
        assert_eq!(
            resolve_device(&devices, " LAPTOP ").unwrap().device_id,
            "cli-2222bbbb"
        );
        assert_eq!(
            resolve_device(&devices, "cli-1").unwrap().device_id,
            "cli-1111aaaa"
        );
        // A prefix shared by both is no match, and a 3-char prefix never is.
        assert!(resolve_device(&devices, "cli-").is_err());
        assert!(resolve_device(&devices, "cli").is_err());
        let miss = resolve_device(&devices, "nas").unwrap_err().to_string();
        assert!(miss.contains("known devices: Build Box, laptop"), "{miss}");
    }

    #[test]
    fn a_shared_label_is_ambiguous() {
        let devices = vec![device("a-1", "box", true), device("b-2", "Box", true)];
        let err = resolve_device(&devices, "box").unwrap_err().to_string();
        assert!(err.contains("several devices are labelled `box`"), "{err}");
        assert!(err.contains("a-1") && err.contains("b-2"), "{err}");
    }

    #[test]
    fn a_start_target_must_be_online_and_run_the_agent() {
        let devices = vec![device("a-1", "box", true), device("b-2", "laptop", false)];
        assert!(resolve_start_target(&devices, "box", Some("claude")).is_ok());
        let offline = resolve_start_target(&devices, "laptop", None)
            .unwrap_err()
            .to_string();
        assert!(offline.starts_with("laptop is offline."), "{offline}");
        let agent = resolve_start_target(&devices, "box", Some("codex"))
            .unwrap_err()
            .to_string();
        assert_eq!(agent, "box cannot run codex (its agents: claude).");
    }

    #[test]
    fn the_device_table_marks_default_and_this_machine() {
        let mut main = device("cli-1", "build box", true);
        main.is_default = true;
        main.unauthed_agents = vec!["codex".to_string()];
        let mut laptop = device("desk-2", "", false);
        laptop.kind = "desktop".to_string();
        laptop.agents.clear();
        let running = HashMap::from([("cli-1".to_string(), 2)]);
        let table = render_devices(&[main, laptop], &running, "cli-1");
        let expected = [
            "DEVICE                      KIND     STATUS   AGENTS                      RUNNING  ID",
            "build box * (this machine)  cli      online   claude, codex (signed out)  2        cli-1",
            "desk-2                      desktop  offline  none                        0        desk-2",
        ];
        assert_eq!(table, format!("{}\n", expected.join("\n")));
    }

    #[test]
    fn running_counts_group_by_device() {
        let rows = vec![
            RemoteSession {
                id: "s1".into(),
                device_id: Some("d1".into()),
                ..Default::default()
            },
            RemoteSession {
                id: "s2".into(),
                device_id: Some("d1".into()),
                ..Default::default()
            },
            RemoteSession {
                id: "s3".into(),
                device_id: None,
                ..Default::default()
            },
        ];
        let counts = running_counts(&rows);
        assert_eq!(counts.get("d1"), Some(&2));
        assert_eq!(counts.len(), 1);
    }
}
