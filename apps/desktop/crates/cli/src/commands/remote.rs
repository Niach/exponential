//! EXP-1110: `code … --device` / `run … --device` — start the run on ANOTHER
//! registered machine instead of this one, the way the web composer does.
//!
//! The start rides `exponential_sessions_start` (`api::mcp_tools`): the
//! server's `steer.startSession` (subject, agent vocabulary, device caps,
//! repo resolution) plus its wait for the device to register the row, so the
//! CLI can print the run's id. Absent agent options mean the TARGET device's
//! own launch defaults — this machine's settings never leak into it.

use std::process::ExitCode;

use anyhow::Context as _;
use api::mcp_tools::{self, RemoteStartInput};

use super::CommandResult;
use crate::context::Ctx;
use crate::launch::AgentFlags;

/// The remote-only flags `code` and `run` accept beside the agent flags.
#[derive(Debug, Default, PartialEq)]
pub struct RemoteFlags {
    /// `--device <label|id>`: set = remote start.
    pub device: Option<String>,
    /// `--account <profile>`: the agent account profile ON the target.
    pub account: Option<String>,
    /// `--follow`: stream the transcript after the start.
    pub follow: bool,
}

impl RemoteFlags {
    pub fn take(args: &mut Vec<String>) -> Self {
        Self {
            device: super::take_value(args, "--device").filter(|value| !value.is_empty()),
            account: super::take_value(args, "--account").filter(|value| !value.is_empty()),
            follow: super::take_flag(args, "--follow"),
        }
    }

    /// The flags that only mean something on a remote start.
    pub fn reject_local(&self) -> anyhow::Result<()> {
        if self.account.is_some() {
            anyhow::bail!("--account picks a profile on the TARGET device; it needs --device");
        }
        if self.follow {
            anyhow::bail!(
                "--follow tails a remote start; it needs --device (a local run attaches by itself)"
            );
        }
        Ok(())
    }
}

/// Fold the agent flags into a start input. Only what was typed rides:
/// `--plan` = plan mode on, nothing = the device's own default.
pub fn start_input(device_id: &str, flags: &AgentFlags, remote: &RemoteFlags) -> RemoteStartInput {
    RemoteStartInput {
        device_id: device_id.to_string(),
        agent: flags.agent.clone(),
        model: flags.model.clone(),
        effort: flags.effort.clone(),
        plan_mode: flags.plan.then_some(true),
        account: remote.account.clone(),
        ..Default::default()
    }
}

/// Resolve the target, start, report — and with `--follow`, tail it.
/// `fill` adds the subject (issue / action + inputs + prompt).
pub fn start(
    ctx: &Ctx,
    flags: &AgentFlags,
    remote: &RemoteFlags,
    fill: impl FnOnce(&mut RemoteStartInput),
) -> CommandResult {
    let selector = remote.device.as_deref().unwrap_or_default();
    if let Some(agent) = &flags.agent {
        if coding::CodingAgent::parse(agent).is_none() {
            anyhow::bail!("unknown agent `{agent}` (claude or codex)");
        }
    }
    let devices = mcp_tools::devices_list(&ctx.trpc, None)
        .map_err(super::devices::tool_error)
        .context("list devices")?;
    let device = super::devices::resolve_start_target(&devices, selector, flags.agent.as_deref())?;
    let mut input = start_input(&device.device_id, flags, remote);
    fill(&mut input);
    let label = if device.label.is_empty() {
        device.device_id.clone()
    } else {
        device.label.clone()
    };
    println!("Starting on {label}...");
    let started = mcp_tools::sessions_start(&ctx.trpc, &input)
        .map_err(super::devices::tool_error)
        .context("remote start")?;
    println!("Session {} running on {label}.", started.session_id);
    if !remote.follow {
        println!(
            "Follow it:  exponential sessions log {} --follow",
            started.session_id
        );
        println!(
            "Steer it:   exponential sessions message {} <text>",
            started.session_id
        );
        println!(
            "Stop it:    exponential sessions kill {}",
            started.session_id
        );
        return Ok(ExitCode::SUCCESS);
    }
    let row = match started.session {
        Some(row) => row,
        None => mcp_tools::sessions_get(&ctx.trpc, &started.session_id)
            .map_err(super::devices::tool_error)
            .context("load the session")?,
    };
    super::sessions::stream_transcript(ctx, &row, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_flags_strip_from_the_args() {
        let mut args: Vec<String> = [
            "EXP-1",
            "--device",
            "box",
            "--account=work",
            "--follow",
            "--plan",
        ]
        .iter()
        .map(|arg| arg.to_string())
        .collect();
        let remote = RemoteFlags::take(&mut args);
        assert_eq!(
            remote,
            RemoteFlags {
                device: Some("box".into()),
                account: Some("work".into()),
                follow: true,
            }
        );
        assert_eq!(args, vec!["EXP-1".to_string(), "--plan".to_string()]);
        assert!(remote.reject_local().is_err());
        assert!(RemoteFlags::default().reject_local().is_ok());
    }

    #[test]
    fn only_typed_options_ride_the_start() {
        let flags = AgentFlags {
            agent: Some("codex".into()),
            plan: true,
            ..Default::default()
        };
        let input = start_input("d1", &flags, &RemoteFlags::default());
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"deviceId":"d1","agent":"codex","planMode":true}"#
        );
        let bare = start_input("d1", &AgentFlags::default(), &RemoteFlags::default());
        assert_eq!(
            serde_json::to_string(&bare).unwrap(),
            r#"{"deviceId":"d1"}"#
        );
    }
}
