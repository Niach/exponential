//! `exponential run <action>` — run a team action (or one of the two
//! virtual builtins) as an interactive agent session, exactly like the
//! desktop's Actions rail: fresh body fetched at run time, per-agent MCP
//! wiring, a `coding_sessions` row steerable from the web.

use std::collections::HashMap;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context as _};
use api::actions::{BUILTIN_CREATE_ACTION_ID, BUILTIN_FIX_CONFLICTS_ID};
use coding::{ActionInputValue, Prepared, PrepareRequest};

use super::{reject_unknown_flags, take_flag, take_value, take_values, CommandResult};
use crate::launch::{self, ActionRepo, AgentFlags};
use crate::session_host::{self, LaunchEnv};
use crate::sidecars::Sidecars;
use crate::{context, term};

pub fn run(args: &[String]) -> CommandResult {
    let mut args = args.to_vec();
    let flags = AgentFlags {
        agent: take_value(&mut args, "--agent"),
        model: take_value(&mut args, "--model"),
        effort: take_value(&mut args, "--effort"),
        plan: take_flag(&mut args, "--plan"),
        skip_permissions: take_flag(&mut args, "--skip-permissions"),
    };
    let team_flag = take_value(&mut args, "--team");
    let raw_inputs = take_values(&mut args, "--input");
    let detach = take_flag(&mut args, "--detach");
    reject_unknown_flags(&args)?;
    let Some(action_ref) = args.first() else {
        bail!("usage: exponential run <action-id-or-name> [--team <team-id>] [--input k=v ...] [--agent ...] [--detach]");
    };

    let ctx = context::load()?;
    let interactive = !detach && term::stdin_is_tty() && term::stdout_is_tty();
    let options = launch::agent_options(&ctx.settings, &flags, interactive)?;

    let team_id = match team_flag {
        Some(team) if !team.is_empty() => team,
        _ => launch::default_team_id(&ctx.trpc)?,
    };

    let (action_id, input_defs) = resolve_action_ref(&ctx, action_ref, &team_id)?;
    let inputs = build_inputs(&raw_inputs, &input_defs, &action_id)?;

    let request = launch::resolve_action_request(
        &ctx,
        &action_id,
        &team_id,
        ActionRepo::Resolve,
        inputs,
        options,
        coding::LaunchOrigin::Local,
        // A hand-typed `run` is never automation-started.
        None,
    )?;
    println!("Running action: {}", request.action_name);

    let deps = launch::coding_deps(&ctx, HashMap::new(), launch::LaunchHost::Foreground);
    let sidecars = Sidecars::start();
    let runtime = steer::SteerRuntime::new().ok();
    let personal_key = context::ensure_personal_key(&ctx).ok();

    let prepared = coding::prepare_with_hooks(
        &PrepareRequest::Action(request),
        &deps,
        sidecars.hook_setup().as_ref(),
        sidecars.observer_setup().as_ref(),
    )
    .map_err(|err| anyhow!("{err}"))?;
    let prepared = match prepared {
        Prepared::Ready(prepared) => prepared,
        Prepared::Disabled(reason) => {
            eprintln!("Can't run the action: {}", reason.message());
            return Ok(ExitCode::FAILURE);
        }
    };

    let env = LaunchEnv {
        ctx: &ctx,
        runtime: runtime.as_ref(),
        sidecars: &sidecars,
        personal_key,
    };
    let session = Arc::new(session_host::launch(&env, prepared, interactive, None)?);

    if interactive {
        super::code::attend(&session)
    } else {
        println!("Session {} running — steer it from the web.", session.session_id);
        super::code::wait_with_signals(&session)
    }
}

/// Resolve an action reference: a builtin id (or its short alias), a UUID,
/// or a case-insensitive action name within the team.
fn resolve_action_ref(
    ctx: &crate::context::Ctx,
    action_ref: &str,
    team_id: &str,
) -> anyhow::Result<(String, Vec<api::actions::ActionInput>)> {
    let builtin = match action_ref {
        BUILTIN_CREATE_ACTION_ID | "create-action" => Some(BUILTIN_CREATE_ACTION_ID),
        BUILTIN_FIX_CONFLICTS_ID | "fix-conflicts" => Some(BUILTIN_FIX_CONFLICTS_ID),
        _ => None,
    };
    if let Some(id) = builtin {
        let action = if id == BUILTIN_FIX_CONFLICTS_ID {
            api::actions::builtin_fix_conflicts_action(team_id)
        } else {
            api::actions::builtin_create_action(team_id)
        };
        return Ok((id.to_string(), action.inputs));
    }
    if uuid::Uuid::parse_str(action_ref).is_ok() {
        let action = api::actions::get(&ctx.trpc, action_ref).context("load the action")?;
        return Ok((action.id, action.inputs));
    }
    // Name lookup within the team (`actions.list` appends the builtins too).
    let actions = api::actions::list(&ctx.trpc, team_id).context("list the team's actions")?;
    let wanted = action_ref.to_lowercase();
    let matched = actions
        .into_iter()
        .find(|action| action.name.to_lowercase() == wanted)
        .ok_or_else(|| anyhow!("no action named `{action_ref}` in that team"))?;
    Ok((matched.id, matched.inputs))
}

/// `--input k=v` values mapped onto the action's input definitions —
/// required ones enforced, unknown keys rejected (typos must not silently
/// drop an input the prompt expects).
fn build_inputs(
    raw: &[String],
    defs: &[api::actions::ActionInput],
    action_id: &str,
) -> anyhow::Result<Vec<ActionInputValue>> {
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
    let mut inputs = Vec::new();
    for def in defs {
        let required = def.required;
        match provided.remove(&def.key) {
            Some(value) => inputs.push(ActionInputValue {
                key: def.key.clone(),
                label: def.label.clone(),
                input_type: def.input_type.clone(),
                value,
                display: None,
            }),
            None if required => bail!("missing required input `{}` (--input {}=...)", def.key, def.key),
            None => {}
        }
    }
    Ok(inputs)
}
