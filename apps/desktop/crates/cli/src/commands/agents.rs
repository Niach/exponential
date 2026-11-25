//! `exponential agents …` (#FEED-311): your teams' agents from a terminal or
//! a script. A message is sealed by the key holder on this machine, like
//! `sessions message` ([`super::sessions::send`]).

use std::io::Read as _;
use std::process::ExitCode;

use anyhow::{bail, Context as _};
use api::mcp_tools::{self, RemoteAgent, SessionsSendInput};

use super::{reject_unknown_flags, CommandResult};
use crate::context::{self, Ctx};

const USAGE: &str = "\
Usage: exponential agents [<command>]

  [list]                  Your teams' agents
  message <agent> <text…> Message an agent (`-` reads stdin)
  new <agent>             Start a new conversation (changed instructions apply then)

<agent> = the agent's UUID or its name.
";

pub fn run(args: &[String]) -> CommandResult {
    let sub = args.first().map(String::as_str).unwrap_or("");
    let rest = &args[1.min(args.len())..];
    match sub {
        "" | "list" => list(rest),
        "message" | "msg" => message(rest),
        "new" => new(rest),
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        other => {
            eprintln!("Unknown agents command `{other}`.\n");
            print!("{USAGE}");
            Ok(ExitCode::from(2))
        }
    }
}

struct TeamAgent {
    team: String,
    agent: RemoteAgent,
}

fn load(ctx: &Ctx) -> anyhow::Result<Vec<TeamAgent>> {
    let mut rows = Vec::new();
    for team in mcp_tools::teams_list(&ctx.trpc).map_err(super::devices::tool_error).context("list teams")? {
        let agents = mcp_tools::agents_list(&ctx.trpc, &team.id).map_err(super::devices::tool_error).context("list agents")?;
        rows.extend(agents.into_iter().map(|agent| TeamAgent { team: team.name.clone(), agent }));
    }
    Ok(rows)
}

fn list(args: &[String]) -> CommandResult {
    reject_unknown_flags(args)?;
    let ctx = context::load()?;
    for row in load(&ctx)? {
        println!("{}  {}  ({})", row.agent.id, row.agent.name, row.team);
    }
    Ok(ExitCode::SUCCESS)
}

/// An agent by id, or by name (case-insensitive).
fn find<'a>(rows: &'a [TeamAgent], selector: &str) -> anyhow::Result<&'a TeamAgent> {
    let wanted = selector.trim();
    if let Some(exact) = rows.iter().find(|row| row.agent.id == wanted) {
        return Ok(exact);
    }
    let mut matches = rows.iter().filter(|row| row.agent.name.to_lowercase() == wanted.to_lowercase());
    match (matches.next(), matches.next()) {
        (Some(row), None) => Ok(row),
        (None, _) => bail!("no agent `{wanted}` (see `exponential agents`)"),
        _ => bail!("`{wanted}` names several agents; use the id (see `exponential agents`)"),
    }
}

fn send(ctx: &Ctx, selector: &str, text: &str) -> anyhow::Result<String> {
    // A UUID needs no lookup.
    let id = match uuid::Uuid::parse_str(selector.trim()) {
        Ok(_) => selector.trim().to_string(),
        Err(_) => find(&load(ctx)?, selector)?.agent.id.clone(),
    };
    let input = SessionsSendInput { agent_id: Some(id), text: Some(text.to_string()), ..Default::default() };
    Ok(super::sessions::send(ctx, &input)?.session_id)
}

fn message(args: &[String]) -> CommandResult {
    // No flag parsing: the text is free-form (a leading `-` is prose).
    let [agent, words @ ..] = args else {
        bail!("usage: exponential agents message <agent> <text…>");
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
        bail!("usage: exponential agents message <agent> <text…>");
    }
    let session = send(&context::load()?, agent, text)?;
    println!("Queued for session {session}.");
    Ok(ExitCode::SUCCESS)
}

fn new(args: &[String]) -> CommandResult {
    let [agent] = args else {
        bail!("usage: exponential agents new <agent>");
    };
    let session = send(&context::load()?, agent, coding::pickup::NEW_CONVERSATION_COMMAND)?;
    println!("Session {session} starts a new conversation.");
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agent_is_found_by_id_then_by_its_only_name() {
        let row = |id: &str, name: &str| TeamAgent { team: "t".into(), agent: RemoteAgent { id: id.into(), name: name.into() } };
        let rows = vec![row("a", "Koordinator"), row("b", "Ops"), row("c", "ops")];
        assert_eq!(find(&rows, "koordinator").unwrap().agent.id, "a");
        assert_eq!(find(&rows, "b").unwrap().agent.id, "b");
        assert!(find(&rows, "Ops").is_err());
        assert!(find(&rows, "x").is_err());
    }
}
