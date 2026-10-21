//! `exponential run <prompt>` — run a team's saved prompt as an agent
//! session.
//!
//! NEW-2: prompts replaced actions. The start is
//! `exponential_sessions_send({promptId, text})`: a root whose first message
//! is the prompt's body plus `--prompt`. Locally that is a queued row naming
//! THIS device, claimed and run here through the ONE pickup
//! (`launch::start_foreground`); with `--device` another machine's daemon
//! picks it up. Prompts take no inputs.

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context as _};

use super::{reject_unknown_flags, take_flag, take_value, CommandResult};
use crate::launch::{self, AgentFlags, ForegroundStart};
use crate::{context, term};

pub fn run(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let flags = AgentFlags {
        agent: take_value(&mut args, "--agent"),
        model: take_value(&mut args, "--model"),
        effort: take_value(&mut args, "--effort"),
        plan: take_flag(&mut args, "--plan"),
    };
    let team_flag = take_value(&mut args, "--team");
    // EXP-825: the composer's free text, appended to the prompt's body in
    // the session's first message.
    let prompt = take_value(&mut args, "--prompt").filter(|text| !text.trim().is_empty());
    let detach = take_flag(&mut args, "--detach");
    // EXP-1110: `--device` runs it on another registered machine.
    let remote = super::remote::RemoteFlags::take(&mut args);
    reject_unknown_flags(&args)?;
    let Some(prompt_ref) = args.first() else {
        bail!("usage: exponential run <prompt-id-or-name> [--device <label|id> [--account <profile>] [--follow]] [--team <team-id>] [--prompt <text>] [--agent ...] [--detach]");
    };

    if remote.device.is_none() {
        remote.reject_local()?;
    }
    let ctx = Arc::new(context::load()?);
    let saved = resolve_prompt_ref(&ctx, prompt_ref, team_flag.as_deref())?;
    // The prompt's team — a plain terminal has no calling session for
    // `sessions.send` to infer it from.
    let subject = |input: &mut api::mcp_tools::SessionsSendInput| {
        input.team_id = Some(saved.team_id.clone());
        input.prompt_id = Some(saved.id.clone());
        input.text = prompt.clone();
    };
    if remote.device.is_some() {
        return super::remote::start(&ctx, &flags, &remote, subject);
    }
    let interactive = !detach && term::stdin_is_tty() && term::stdout_is_tty();
    let options = launch::agent_options(&ctx.settings, &flags, interactive)?;
    println!("Running prompt: {}", saved.name);

    let mut input = api::mcp_tools::SessionsSendInput {
        model: flags.model.clone(),
        effort: flags.effort.clone(),
        ..Default::default()
    };
    subject(&mut input);
    let started = match launch::start_foreground(Arc::clone(&ctx), input, options)? {
        ForegroundStart::Running(started) => started,
        ForegroundStart::Refused(reason) => {
            eprintln!("Can't run the prompt: {reason}");
            return Ok(ExitCode::FAILURE);
        }
    };
    let session = Arc::clone(&started.session);
    let outcome = if interactive {
        super::code::attend_acp(&session)
    } else {
        println!("Session {} running — steer it from the web.", session.session_id);
        super::code::wait_with_signals(&session)
    };
    super::code::purge_scratch(&ctx, &session);
    outcome
}

/// Resolve a prompt reference: a UUID, or a case-insensitive prompt name
/// within `--team` (default: the caller's default team).
fn resolve_prompt_ref(
    ctx: &crate::context::Ctx,
    prompt_ref: &str,
    team: Option<&str>,
) -> anyhow::Result<api::prompts::Prompt> {
    if uuid::Uuid::parse_str(prompt_ref).is_ok() {
        return api::prompts::get(&ctx.trpc, prompt_ref).context("load the prompt");
    }
    let team_id = match team {
        Some(team) if !team.is_empty() => team.to_string(),
        _ => launch::default_team_id(&ctx.trpc)?,
    };
    let prompts = api::prompts::list(&ctx.trpc, &team_id).context("list the team's prompts")?;
    find_by_name(prompts, prompt_ref).ok_or_else(|| anyhow!("no prompt named `{prompt_ref}` in that team"))
}

fn find_by_name(prompts: Vec<api::prompts::Prompt>, name: &str) -> Option<api::prompts::Prompt> {
    let wanted = name.trim().to_lowercase();
    prompts.into_iter().find(|prompt| prompt.name.to_lowercase() == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_name_matches_case_insensitively() {
        let prompt = |id: &str, name: &str| -> api::prompts::Prompt {
            serde_json::from_value(serde_json::json!({
                "id": id, "teamId": "t1", "name": name, "body": ""
            }))
            .unwrap()
        };
        let found = find_by_name(vec![prompt("p1", "Triage"), prompt("p2", "Release notes")], " release NOTES ");
        assert_eq!(found.map(|prompt| prompt.id), Some("p2".to_string()));
        assert!(find_by_name(vec![prompt("p1", "Triage")], "deploy").is_none());
    }
}
