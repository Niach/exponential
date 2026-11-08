//! EXP-724 / NEW-2: THE `/` command menu's rules, gpui-free so every host
//! draws the same menu (the IDE's composer, #EDE-65 Exponential Desktop's
//! chat): the curated catalog off the contract (`steerCommands`), when the
//! menu opens, what it offers and what accepting leaves in the composer.
//! Mirrored by web `lib/steer-commands.ts`, iOS `SlashCommands.swift` and
//! Android `SlashCommands.kt`.

use crate::contract::{STEER_COMMAND_AGENTS, STEER_COMMAND_ARG_HINTS, STEER_COMMAND_CONFIRM, STEER_COMMAND_DESCRIPTIONS, STEER_COMMAND_NAMES};

/// Rows the menu draws at most (web `filterSteerCommands`' cap).
pub const MENU_LIMIT: usize = 8;

/// One catalog row (a zipped view over the contract's parallel arrays).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
    /// Empty = the command takes no argument.
    pub arg_hint: &'static str,
    /// The client confirms before sending (context is discarded).
    pub confirm: bool,
}

/// Every catalog row with the agents (`claude,codex`) it applies to, in contract order.
pub fn catalog() -> impl Iterator<Item = (SlashCommand, &'static str)> {
    STEER_COMMAND_NAMES.iter().enumerate().map(|(i, name)| {
        (
            SlashCommand { name, description: STEER_COMMAND_DESCRIPTIONS[i], arg_hint: STEER_COMMAND_ARG_HINTS[i], confirm: STEER_COMMAND_CONFIRM[i] },
            STEER_COMMAND_AGENTS[i],
        )
    })
}

/// The rows applicable to the agent CLI `agent` (`claude` | `codex`), in contract order.
pub fn catalog_for(agent: &str) -> Vec<SlashCommand> {
    catalog().filter(|(_, agents)| agents.split(',').any(|a| a == agent)).map(|(command, _)| command).collect()
}

/// The typed query behind an OPEN `/` menu, or `None` when the draft is not a
/// bare command token (`^/[A-Za-z0-9-]*$`, web `matchSlashDraft`). Untrimmed:
/// the first space closes the menu for good.
pub fn slash_query(draft: &str) -> Option<&str> {
    let rest = draft.strip_prefix('/')?;
    rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-').then_some(rest)
}

/// The menu's rows for `draft` and `agent`: a case-insensitive name-PREFIX
/// filter over the catalog (an empty query lists them all), capped. The
/// commands the agent CLI advertises are never merged in (NEW-2).
pub fn menu_matches(draft: &str, agent: &str) -> Vec<SlashCommand> {
    let Some(query) = slash_query(draft) else { return Vec::new() };
    let needle = query.to_ascii_lowercase();
    let mut rows: Vec<SlashCommand> = catalog_for(agent).into_iter().filter(|row| row.name.to_ascii_lowercase().starts_with(&needle)).collect();
    rows.truncate(MENU_LIMIT);
    rows
}

/// What accepting a command leaves in the composer: `/name ` when it takes
/// an argument, bare `/name` when not. Accepting never sends.
pub fn insertion(name: &str, arg_hint: &str) -> String {
    if arg_hint.is_empty() {
        format!("/{name}")
    } else {
        format!("/{name} ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(draft: &str, agent: &str) -> Vec<&'static str> {
        menu_matches(draft, agent).into_iter().map(|c| c.name).collect()
    }

    #[test]
    fn the_menu_opens_only_on_a_bare_command_token() {
        assert_eq!(slash_query("/"), Some(""));
        assert_eq!(slash_query("/co"), Some("co"));
        assert_eq!(slash_query("/co "), None);
        assert_eq!(slash_query("see /co"), None);
        assert_eq!(slash_query("/api/x"), None);
    }

    #[test]
    fn it_prefix_filters_the_contract_catalog_for_the_agent() {
        assert_eq!(names("/", "claude"), STEER_COMMAND_NAMES.to_vec());
        assert_eq!(names("/CO", "codex"), ["compact"]);
        assert!(names("/", "pi").is_empty(), "an unknown agent has no builtins");
        assert!(names("hello", "claude").is_empty());
    }

    #[test]
    fn accepting_leaves_a_space_only_for_an_argument() {
        assert_eq!(insertion("compact", "instructions"), "/compact ");
        assert_eq!(insertion("new", ""), "/new");
    }
}
