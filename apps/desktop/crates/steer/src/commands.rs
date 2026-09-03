//! EXP-724: the curated steer slash-command catalog, read straight off the
//! generated domain contract (`packages/domain-contract/contract.json`
//! `steerCommands`) — the SAME rows every viewer's `/` menu offers, so what
//! a client can pick, the publisher can execute.
//!
//! A remote message is a command iff its first whitespace token is `/<name>`
//! for a catalog row applicable to the session's agent; everything else is
//! prose and rides the ordinary composer path. The publisher owns HOW a
//! command runs per agent (`publisher::handle_input` → the emitter's
//! command link); this module only answers "is it one, and which".

use crate::activity::SessionAgent;
use domain::contract::{
    STEER_COMMAND_AGENTS, STEER_COMMAND_ARG_HINTS, STEER_COMMAND_CONFIRM,
    STEER_COMMAND_DESCRIPTIONS, STEER_COMMAND_NAMES,
};

/// One catalog row (a zipped view over the contract's parallel arrays).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SteerCommand {
    pub name: &'static str,
    pub description: &'static str,
    /// Empty = the command takes no argument.
    pub arg_hint: &'static str,
    /// The client confirms before sending (context is discarded).
    pub confirm: bool,
}

/// A remote message recognised as a catalog command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedCommand {
    pub command: SteerCommand,
    /// Everything after the name, trimmed; empty when none was given.
    pub args: String,
}

impl ParsedCommand {
    /// The canonical text form — what the feed echoes and what gets typed.
    pub fn text(&self) -> String {
        if self.args.is_empty() {
            format!("/{}", self.command.name)
        } else {
            format!("/{} {}", self.command.name, self.args)
        }
    }
}

fn agent_id(agent: SessionAgent) -> &'static str {
    match agent {
        SessionAgent::Claude => "claude",
        SessionAgent::Codex => "codex",
        SessionAgent::Pi => "pi",
    }
}

/// Every catalog row, agent-agnostic, in contract order.
pub fn catalog() -> impl Iterator<Item = (SteerCommand, &'static str)> {
    STEER_COMMAND_NAMES
        .iter()
        .enumerate()
        .map(|(i, name)| {
            (
                SteerCommand {
                    name,
                    description: STEER_COMMAND_DESCRIPTIONS[i],
                    arg_hint: STEER_COMMAND_ARG_HINTS[i],
                    confirm: STEER_COMMAND_CONFIRM[i],
                },
                STEER_COMMAND_AGENTS[i],
            )
        })
}

/// The rows applicable to `agent`, in contract order.
pub fn catalog_for(agent: SessionAgent) -> Vec<SteerCommand> {
    let id = agent_id(agent);
    catalog()
        .filter(|(_, agents)| agents.split(',').any(|a| a == id))
        .map(|(command, _)| command)
        .collect()
}

/// Recognise `text` as a command for `agent`: `/name` or `/name <args>`,
/// case-insensitive on the name, whitespace-tolerant around it. `None` for
/// prose, an unknown or agent-inapplicable name, and paths like `/api/x`.
pub fn parse_command(text: &str, agent: SessionAgent) -> Option<ParsedCommand> {
    let trimmed = text.trim();
    let rest = trimmed.strip_prefix('/')?;
    let (head, tail) = match rest.find(char::is_whitespace) {
        Some(at) => (&rest[..at], rest[at..].trim()),
        None => (rest, ""),
    };
    if head.is_empty() {
        return None;
    }
    let command = catalog_for(agent)
        .into_iter()
        .find(|command| command.name.eq_ignore_ascii_case(head))?;
    Some(ParsedCommand {
        command,
        args: tail.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_is_the_contract_and_every_agent_id_is_known() {
        assert_eq!(STEER_COMMAND_NAMES.len(), STEER_COMMAND_DESCRIPTIONS.len());
        assert_eq!(STEER_COMMAND_NAMES.len(), STEER_COMMAND_ARG_HINTS.len());
        assert_eq!(STEER_COMMAND_NAMES.len(), STEER_COMMAND_AGENTS.len());
        assert_eq!(STEER_COMMAND_NAMES.len(), STEER_COMMAND_CONFIRM.len());
        for (_, agents) in catalog() {
            for agent in agents.split(',') {
                assert!(
                    domain::contract::CODING_AGENT_VALUES.contains(&agent),
                    "unknown agent {agent:?} in steerCommands"
                );
            }
        }
        assert!(catalog_for(SessionAgent::Claude)
            .iter()
            .any(|c| c.name == "compact"));
        // `/clear` is every agent's (pi runs it natively) and confirms.
        for agent in [SessionAgent::Claude, SessionAgent::Codex, SessionAgent::Pi] {
            let rows = catalog_for(agent);
            assert_eq!(
                rows.iter().map(|c| c.name).collect::<Vec<_>>(),
                vec!["compact", "clear"]
            );
            assert!(rows.iter().any(|c| c.name == "clear" && c.confirm));
            assert!(rows.iter().any(|c| c.name == "compact" && !c.confirm));
        }
    }

    #[test]
    fn parse_command_recognises_only_catalog_names_for_the_agent() {
        let parsed = parse_command("/compact keep the diff", SessionAgent::Claude).unwrap();
        assert_eq!(parsed.command.name, "compact");
        assert_eq!(parsed.args, "keep the diff");
        assert_eq!(parsed.text(), "/compact keep the diff");
        assert_eq!(
            parse_command("  /Compact  ", SessionAgent::Pi).unwrap().text(),
            "/compact"
        );
        // Not in the catalog at all.
        assert_eq!(parse_command("/cost", SessionAgent::Claude), None);
        assert_eq!(parse_command("/model opus", SessionAgent::Claude), None);
        assert_eq!(parse_command("/new", SessionAgent::Codex), None);
        assert!(parse_command("/clear", SessionAgent::Codex).is_some());
        assert!(parse_command("/clear", SessionAgent::Pi).is_some());
        // Prose, paths and a bare slash are never commands.
        assert_eq!(parse_command("fix /compact later", SessionAgent::Claude), None);
        assert_eq!(parse_command("/api/foo", SessionAgent::Claude), None);
        assert_eq!(parse_command("/", SessionAgent::Claude), None);
        assert_eq!(parse_command("//compact", SessionAgent::Claude), None);
    }
}
