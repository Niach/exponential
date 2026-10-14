//! `exponential run <action>` — run a team action as an agent session,
//! exactly like the desktop's Actions rail.
//!
//! EXP-1292: an action is a SAVED PROMPT — the start is
//! `exponential_sessions_send({actionId, inputs, text})` and the server
//! renders the body + pick inputs into the session's brief. Locally that is
//! a queued row naming THIS device, claimed and run here through the ONE
//! pickup (`launch::start_foreground`); with `--device` another machine's
//! daemon picks it up. The builtins (chat, create-action, fix-conflicts,
//! tidy-up) are gone.

use std::collections::{BTreeMap, HashMap};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context as _};

use super::{reject_unknown_flags, take_flag, take_value, take_values, CommandResult};
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
    let raw_inputs = take_values(&mut args, "--input");
    // EXP-825: the composer's free text — the session's first message,
    // rendered as the brief's additional instructions.
    let prompt = take_value(&mut args, "--prompt").filter(|text| !text.trim().is_empty());
    let detach = take_flag(&mut args, "--detach");
    // EXP-1110: `--device` runs it on another registered machine.
    let remote = super::remote::RemoteFlags::take(&mut args);
    reject_unknown_flags(&args)?;
    let Some(action_ref) = args.first() else {
        bail!("usage: exponential run <action-id-or-name> [--device <label|id> [--account <profile>] [--follow]] [--team <team-id>] [--input k=v ...] [--prompt <text>] [--agent ...] [--detach]");
    };

    if remote.device.is_none() {
        remote.reject_local()?;
    }
    let ctx = Arc::new(context::load()?);
    let action = resolve_action_ref(&ctx, action_ref, team_flag.as_deref())?;
    let inputs = build_inputs(&raw_inputs, &action.inputs, &action.id)?;
    // EXP-1292: the action's team — a plain terminal has no calling session
    // for `sessions.send` to infer it from.
    let subject = |input: &mut api::mcp_tools::SessionsSendInput| {
        input.team_id = Some(action.team_id.clone());
        input.action_id = Some(action.id.clone());
        input.inputs = (!inputs.is_empty()).then(|| inputs.clone());
        input.text = prompt.clone();
    };
    if remote.device.is_some() {
        return super::remote::start(&ctx, &flags, &remote, subject);
    }
    let interactive = !detach && term::stdin_is_tty() && term::stdout_is_tty();
    let options = launch::agent_options(&ctx.settings, &flags, interactive)?;
    println!("Running action: {}", action.name);

    let mut input = api::mcp_tools::SessionsSendInput {
        model: flags.model.clone(),
        effort: flags.effort.clone(),
        ..Default::default()
    };
    subject(&mut input);
    let started = match launch::start_foreground(Arc::clone(&ctx), input, options)? {
        ForegroundStart::Running(started) => started,
        ForegroundStart::Refused(reason) => {
            eprintln!("Can't run the action: {reason}");
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

/// Resolve an action reference: a UUID, or a case-insensitive action name
/// within `--team` (default: the caller's default team).
fn resolve_action_ref(
    ctx: &crate::context::Ctx,
    action_ref: &str,
    team: Option<&str>,
) -> anyhow::Result<api::actions::Action> {
    if uuid::Uuid::parse_str(action_ref).is_ok() {
        return api::actions::get(&ctx.trpc, action_ref).context("load the action");
    }
    let team_id = match team {
        Some(team) if !team.is_empty() => team.to_string(),
        _ => launch::default_team_id(&ctx.trpc)?,
    };
    let actions = api::actions::list(&ctx.trpc, &team_id).context("list the team's actions")?;
    find_by_name(actions, action_ref)
        .ok_or_else(|| anyhow!("no action named `{action_ref}` in that team"))
}

fn find_by_name(actions: Vec<api::actions::Action>, name: &str) -> Option<api::actions::Action> {
    let wanted = name.trim().to_lowercase();
    actions
        .into_iter()
        .find(|action| action.name.to_lowercase() == wanted)
}

/// `--input k=v` values mapped onto the action's PICK input definitions —
/// required ones enforced, unknown keys rejected (typos must not silently
/// drop an input the prompt expects), an input type this build does not know
/// refused (contract `actionInputType`; never a silent text fallback).
fn build_inputs(
    raw: &[String],
    defs: &[api::actions::ActionInput],
    action_id: &str,
) -> anyhow::Result<BTreeMap<String, String>> {
    let mut provided: HashMap<String, String> = HashMap::new();
    for entry in raw {
        let (key, value) = entry
            .split_once('=')
            .ok_or_else(|| anyhow!("--input takes k=v, got `{entry}`"))?;
        provided.insert(key.trim().to_string(), value.to_string());
    }
    for key in provided.keys() {
        if !defs.iter().any(|def| &def.key == key) {
            bail!("action `{action_id}` has no input `{key}`");
        }
    }
    let mut inputs = BTreeMap::new();
    for def in defs {
        if !domain::contract::ACTION_INPUT_TYPE_VALUES.contains(&def.input_type.as_str()) {
            bail!(
                "action `{action_id}` declares the input `{}` of type `{}`, which this version \
of the CLI does not know — run `exponential update`",
                def.key,
                def.input_type
            );
        }
        match provided.remove(&def.key) {
            Some(value) => {
                inputs.insert(def.key.clone(), value);
            }
            None if def.required => {
                bail!("missing required input `{}` (--input {}=...)", def.key, def.key)
            }
            None => {}
        }
    }
    Ok(inputs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(key: &str, input_type: &str, required: bool) -> api::actions::ActionInput {
        api::actions::ActionInput {
            key: key.to_string(),
            label: key.to_string(),
            input_type: input_type.to_string(),
            required,
            placeholder: None,
        }
    }

    fn raw(entries: &[&str]) -> Vec<String> {
        entries.iter().map(|entry| entry.to_string()).collect()
    }

    #[test]
    fn pick_inputs_map_onto_their_definitions() {
        let defs = [def("board", "board", true), def("repo", "repo", false)];
        let inputs = build_inputs(&raw(&["board=b1"]), &defs, "a1").unwrap();
        assert_eq!(inputs, BTreeMap::from([("board".to_string(), "b1".to_string())]));
        assert!(build_inputs(&[], &defs, "a1")
            .unwrap_err()
            .to_string()
            .contains("missing required input `board`"));
        assert!(build_inputs(&raw(&["bord=b1"]), &defs, "a1")
            .unwrap_err()
            .to_string()
            .contains("has no input `bord`"));
        assert!(build_inputs(&raw(&["board"]), &defs, "a1").is_err());
    }

    /// An input type outside the contract refuses the start instead of
    /// guessing at it.
    #[test]
    fn an_unknown_input_type_refuses() {
        let defs = [def("notes", "textarea", false)];
        assert!(build_inputs(&[], &defs, "a1")
            .unwrap_err()
            .to_string()
            .contains("does not know"));
        for kind in domain::contract::ACTION_INPUT_TYPE_VALUES {
            assert!(build_inputs(&[], &[def("x", kind, false)], "a1").is_ok());
        }
    }

    #[test]
    fn an_action_name_matches_case_insensitively() {
        let action = |id: &str, name: &str| -> api::actions::Action {
            serde_json::from_value(serde_json::json!({
                "id": id, "teamId": "t1", "name": name, "body": ""
            }))
            .unwrap()
        };
        let found = find_by_name(vec![action("a1", "Triage"), action("a2", "Release notes")], " release NOTES ");
        assert_eq!(found.map(|action| action.id), Some("a2".to_string()));
        assert!(find_by_name(vec![action("a1", "Triage")], "deploy").is_none());
    }
}
