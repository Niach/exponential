//! EXP-724: the UI half of the steer slash-command catalog — the `/` menu's
//! matching rules and the confirm copy, hand-mirrored from the web's
//! `lib/steer-commands.ts`, iOS `SlashCommands.swift` and Android
//! `SlashCommands.kt`.
//!
//! The catalog itself is NOT here: it is contract data, read through
//! [`steer::commands`], so the rows this composer offers are exactly the rows
//! the publisher on the other end knows how to execute. What lives here is
//! the part every viewer has to agree on character for character:
//!
//! * **when the menu opens** — only while the WHOLE draft is a bare command
//!   token (`^/[A-Za-z0-9-]*$`). The first space closes it for good: what
//!   follows is the command's argument, or prose that merely began with a
//!   slash;
//! * **what it offers** — a case-insensitive name-PREFIX filter over the
//!   agent's rows, in contract order, so an empty query lists them all;
//! * **what accepting leaves behind** — `/name ` for a command that takes an
//!   argument, bare `/name` for one that does not. Accepting never sends;
//! * **the confirm copy** for a context-discarding row (none in the catalog
//!   since NEW-2 dropped `/clear`; the copy stays the ×4 mirror);
//! * NEW-2 (owner 2026-10-21): the catalog is the contract's `compact`,
//!   `model` and `effort`, and the commands the AGENT CLI injects (skills,
//!   plugins, `config_state.commands`) are NEVER merged into the menu.
//!   `/effort <level>` is a launch-option change ([`effort_level`] →
//!   `sessions.configure`), `/model <alias>` a message the CLI acts on.

use steer::activity::SessionAgent;
use steer::commands::{catalog_for, SteerCommand};

/// Rows the menu will draw at most (web `filterSteerCommands`' cap; the
/// catalog is shorter than this today and the cap is the parity, not a
/// design).
const MENU_LIMIT: usize = 8;

/// Which agent a synced session row runs. A row that names none predates the
/// column — contract order puts claude first, so claude it is.
///
/// EXP-746: unless the run published a `config_state` (`acp`). Only the ACP
/// engine publishes one, and an ACP run for claude/codex always stamps its
/// id, so an agent-less ACP row can only be a LEGACY synced row from an
/// external agent. External agents were retired in EXP-862; the `External`
/// arm survives solely so those old rows keep an empty curated catalog
/// instead of claude's. Mirrored ×4 (`steerAgentId` on web,
/// `SlashCommands.agentId` on iOS and Android).
pub(crate) fn agent_of(row: &domain::rows::SessionRow, acp: bool) -> SessionAgent {
    agent_from_id(row.cli.as_deref(), acp)
}

/// The pure half of [`agent_of`] — `sessions.cli` is a raw wire
/// string, and an unknown one is claude like an absent one.
fn agent_from_id(agent: Option<&str>, acp: bool) -> SessionAgent {
    match agent.map(str::trim) {
        Some(id) if id.eq_ignore_ascii_case("codex") => SessionAgent::Codex,
        Some(id) if !id.is_empty() => SessionAgent::Claude,
        _ if acp => SessionAgent::External,
        _ => SessionAgent::Claude,
    }
}

/// The typed query behind an OPEN `/` menu, or `None` when the draft is not a
/// bare command token (web `matchSlashDraft`). Deliberately untrimmed: the
/// rule is about the whole draft, so a trailing space closes the menu.
pub(crate) fn slash_query(draft: &str) -> Option<&str> {
    let rest = draft.strip_prefix('/')?;
    rest.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-')
        .then_some(rest)
}

/// One row of the `/` menu (owned, so the composer can hold it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MenuCommand {
    pub(crate) name: String,
    pub(crate) description: String,
    /// Empty = the command takes no argument.
    pub(crate) arg_hint: String,
    /// The client confirms before sending (context is discarded).
    pub(crate) confirm: bool,
}

impl From<SteerCommand> for MenuCommand {
    fn from(command: SteerCommand) -> Self {
        Self {
            name: command.name.to_string(),
            description: command.description.to_string(),
            arg_hint: command.arg_hint.to_string(),
            confirm: command.confirm,
        }
    }
}

/// The `/` menu's rows for `draft`: the contract catalog for `agent`,
/// prefix-filtered. NEW-2: NEVER merged with the commands the agent CLI
/// advertises (`config_state.commands`): those stay hidden on every
/// platform.
pub(crate) fn menu_matches(draft: &str, agent: SessionAgent) -> Vec<MenuCommand> {
    let Some(query) = slash_query(draft) else {
        return Vec::new();
    };
    let needle = query.to_ascii_lowercase();
    let mut rows: Vec<MenuCommand> = catalog_for(agent)
        .into_iter()
        .map(MenuCommand::from)
        .filter(|row| row.name.to_ascii_lowercase().starts_with(&needle))
        .collect();
    rows.truncate(MENU_LIMIT);
    rows
}

/// NEW-2: a team prompt as the `/` menu lists it (the synced, body-less
/// `prompts` row): its icon and name, after the builtins (web `SlashPrompt`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SlashPrompt {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) icon: Option<String>,
}

impl From<&api::prompts::Prompt> for SlashPrompt {
    fn from(prompt: &api::prompts::Prompt) -> Self {
        Self {
            id: prompt.id.clone(),
            name: prompt.name.clone(),
            icon: prompt.icon.clone(),
        }
    }
}

/// Rows of prompts the menu draws at most (web `filterSlashPrompts`' cap).
const PROMPT_LIMIT: usize = 50;

/// The token a prompt answers to after the slash (web `promptSlashName`):
/// its name, lowercased, whitespace runs as `-` (`Fix flaky tests` →
/// `fix-flaky-tests`), so `/fix` and `/fix-fl` both find it.
pub(crate) fn prompt_slash_name(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// Case-insensitive PREFIX filter over the prompts' slash names, the team's
/// order preserved; an empty query offers every prompt (web
/// `filterSlashPrompts`).
pub(crate) fn filter_slash_prompts(prompts: &[SlashPrompt], query: &str) -> Vec<SlashPrompt> {
    let needle = query.trim().to_lowercase();
    prompts
        .iter()
        .filter(|prompt| prompt_slash_name(&prompt.name).starts_with(&needle))
        .take(PROMPT_LIMIT)
        .cloned()
        .collect()
}

/// One row of THE `/` menu: a contract builtin or a team prompt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SlashItem {
    Command(MenuCommand),
    Prompt(SlashPrompt),
}

/// NEW-2: THE `/` menu's rows for `draft` (web `useSlashCommandMenu`): the
/// builtins of `agent` (`None` = no session yet, the launch composer: no
/// builtins), then the team's `prompts`, each prefix-filtered.
pub(crate) fn slash_items(
    draft: &str,
    agent: Option<SessionAgent>,
    prompts: &[SlashPrompt],
) -> Vec<SlashItem> {
    let Some(query) = slash_query(draft) else {
        return Vec::new();
    };
    let commands = agent
        .map(|agent| menu_matches(draft, agent))
        .unwrap_or_default()
        .into_iter()
        .map(SlashItem::Command);
    commands
        .chain(filter_slash_prompts(prompts, query).into_iter().map(SlashItem::Prompt))
        .collect()
}

/// The contract's `/effort` row.
pub(crate) const EFFORT: &str = "effort";

/// NEW-2: the level of an `/effort <level>` draft for `agent`, lowercased —
/// sent as `sessions.configure({effort})` like the "+" menu's Effort pick,
/// never as a message; `None` for any other draft or a bare `/effort`.
pub(crate) fn effort_level(draft: &str, agent: SessionAgent) -> Option<String> {
    let parsed = steer::commands::parse_command(draft, agent)?;
    let level = parsed.args.trim();
    (parsed.command.name == EFFORT && !level.is_empty() && !level.contains(char::is_whitespace))
        .then(|| level.to_ascii_lowercase())
}

/// The refusal when a slash command is sent with attachments staged — a
/// command is the WHOLE message (web `session-composer`'s toast, same words).
pub(crate) const ATTACHMENTS_BLOCK_COMMAND: &str = "Remove the attachments to send a command";

/// What accepting `command` puts in the composer — with the trailing space
/// when there is an argument to type, without when there is not.
pub(crate) fn insertion(command: &MenuCommand) -> String {
    if command.arg_hint.is_empty() {
        format!("/{}", command.name)
    } else {
        format!("/{} ", command.name)
    }
}

// ── Composer chip copy (byte-identical ×4) ──────────────────────────────────
// Kept as the ×4 mirror of web `agent-feed.ts`; EXP-790 retired the
// mid-session mode chip that drew them, so nothing renders them here.

/// EXP-746: a chip whose value is BLANK — the agent CLI's own default, which
/// is a real choice (the relay's `set_config` deliberately allows an empty
/// value) and not a missing one.
#[allow(dead_code)]
pub(crate) const CONFIG_DEFAULT_VALUE_LABEL: &str = "CLI default";

/// The mode chip's leading label. Every other chip's label arrives live on
/// `config_state.options[].label`, so this is the only one a client owns.
#[allow(dead_code)]
pub(crate) const CONFIG_MODE_LABEL: &str = "Mode";

// ── Confirm copy (byte-identical ×4) ────────────────────────────────────────

pub(crate) fn confirm_title(name: &str) -> String {
    format!("Run /{name}?")
}

pub(crate) const CONFIRM_BODY: &str =
    "The agent forgets everything in this session so far. Files in the worktree are kept.";

pub(crate) fn confirm_button(name: &str) -> String {
    format!("Run /{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(draft: &str, agent: SessionAgent) -> Vec<String> {
        menu_matches(draft, agent).into_iter().map(|command| command.name).collect()
    }

    #[test]
    fn the_menu_opens_only_on_a_bare_command_token() {
        assert_eq!(slash_query("/"), Some(""));
        assert_eq!(slash_query("/co"), Some("co"));
        assert_eq!(slash_query("/security-review"), Some("security-review"));
        // The first space closes it for good, and prose never opens it.
        assert_eq!(slash_query("/compact "), None);
        assert_eq!(slash_query("/compact keep the diff"), None);
        assert_eq!(slash_query("x /c"), None);
        assert_eq!(slash_query(" /c"), None);
        assert_eq!(slash_query("compact"), None);
        assert_eq!(slash_query(""), None);
    }

    /// NEW-2: the catalog is the contract's compact/model/effort for both
    /// agents (`clear` and `btw` are gone).
    #[test]
    fn an_empty_query_lists_the_agents_whole_catalog_in_contract_order() {
        assert_eq!(names("/", SessionAgent::Claude), vec!["compact", "model", "effort"]);
        assert_eq!(names("/", SessionAgent::Codex), vec!["compact", "model", "effort"]);
        assert!(names("/clear", SessionAgent::Claude).is_empty());
        assert!(names("/btw", SessionAgent::Claude).is_empty());
    }

    #[test]
    fn the_filter_is_a_case_insensitive_name_prefix() {
        assert_eq!(names("/co", SessionAgent::Claude), vec!["compact"]);
        assert_eq!(names("/CO", SessionAgent::Claude), vec!["compact"]);
        assert_eq!(names("/compact", SessionAgent::Claude), vec!["compact"]);
        assert_eq!(names("/m", SessionAgent::Codex), vec!["model"]);
        // A prefix, never a substring: `pact` is inside `compact`, not in front.
        assert!(names("/pact", SessionAgent::Claude).is_empty());
        // A closed token draws nothing at all.
        assert!(names("/compact ", SessionAgent::Claude).is_empty());
        assert!(names("hello", SessionAgent::Claude).is_empty());
    }

    #[test]
    fn accepting_adds_a_trailing_space_when_there_is_an_argument() {
        for (name, hint) in [("compact", "instructions"), ("model", "model"), ("effort", "level")] {
            let row = menu_matches(&format!("/{name}"), SessionAgent::Claude).remove(0);
            assert_eq!(row.arg_hint, hint);
            assert!(!row.confirm);
            assert_eq!(insertion(&row), format!("/{name} "));
        }
        let bare = MenuCommand {
            name: "bare".into(),
            description: String::new(),
            arg_hint: String::new(),
            confirm: false,
        };
        assert_eq!(insertion(&bare), "/bare");
    }

    fn prompt(id: &str, name: &str) -> SlashPrompt {
        SlashPrompt {
            id: id.into(),
            name: name.into(),
            icon: None,
        }
    }

    /// NEW-2 (web `promptSlashName`): lowercased, whitespace runs as `-`.
    #[test]
    fn a_prompt_answers_to_its_dashed_name() {
        assert_eq!(prompt_slash_name("Fix flaky tests"), "fix-flaky-tests");
        assert_eq!(prompt_slash_name("  Release   train "), "release-train");
        assert_eq!(prompt_slash_name("Triage"), "triage");
    }

    /// NEW-2: the builtins first, then the team's prompts in team order,
    /// each a case-insensitive prefix filter; the launch composer (no
    /// session) lists prompts alone.
    #[test]
    fn the_menu_lists_builtins_then_prompts() {
        let prompts = vec![
            prompt("p1", "Fix flaky tests"),
            prompt("p2", "Release train"),
            prompt("p3", "Compact the docs"),
        ];
        let ids = |draft: &str, agent: Option<SessionAgent>| -> Vec<String> {
            slash_items(draft, agent, &prompts)
                .into_iter()
                .map(|item| match item {
                    SlashItem::Command(command) => format!("/{}", command.name),
                    SlashItem::Prompt(prompt) => prompt.id,
                })
                .collect()
        };
        assert_eq!(
            ids("/", Some(SessionAgent::Claude)),
            vec!["/compact", "/model", "/effort", "p1", "p2", "p3"]
        );
        assert_eq!(ids("/co", Some(SessionAgent::Claude)), vec!["/compact", "p3"]);
        assert_eq!(ids("/FIX-FL", Some(SessionAgent::Codex)), vec!["p1"]);
        assert_eq!(ids("/", None), vec!["p1", "p2", "p3"]);
        assert_eq!(ids("/re", None), vec!["p2"]);
        // A substring is not a prefix; a closed token draws nothing.
        assert!(ids("/train", None).is_empty());
        assert!(ids("/release ", None).is_empty());
    }

    /// NEW-2: `/effort <level>` is a configure, never a message; `/model`
    /// and anything else is not.
    #[test]
    fn effort_carries_its_level() {
        assert_eq!(effort_level("/effort High", SessionAgent::Claude).as_deref(), Some("high"));
        assert_eq!(effort_level(" /EFFORT  low ", SessionAgent::Codex).as_deref(), Some("low"));
        assert_eq!(effort_level("/effort", SessionAgent::Claude), None);
        assert_eq!(effort_level("/effort very high", SessionAgent::Claude), None);
        assert_eq!(effort_level("/model opus", SessionAgent::Claude), None);
        assert_eq!(effort_level("effort high", SessionAgent::Claude), None);
    }

    /// An external ACP agent has no contract catalog at all, and NEW-2
    /// never merges the agent's own commands: its menu is empty.
    #[test]
    fn an_external_agent_offers_nothing() {
        assert!(names("/", SessionAgent::External).is_empty());
    }

    /// The two chip strings, spelled out where the ×4 mirror can see them.
    #[test]
    fn the_chip_copy_is_the_shared_wording() {
        assert_eq!(CONFIG_DEFAULT_VALUE_LABEL, "CLI default");
        assert_eq!(CONFIG_MODE_LABEL, "Mode");
    }

    #[test]
    fn a_row_without_a_known_agent_steers_a_claude_session() {
        assert_eq!(agent_from_id(None, false), SessionAgent::Claude);
        assert_eq!(agent_from_id(Some("codex"), false), SessionAgent::Codex);
        // EXP-849: a retired agent id reads like any other unknown one.
        assert_eq!(agent_from_id(Some(" pi "), false), SessionAgent::Claude);
        assert_eq!(agent_from_id(Some("claude"), false), SessionAgent::Claude);
        assert_eq!(agent_from_id(Some("something-else"), false), SessionAgent::Claude);
    }

    /// EXP-746, mirrored ×4: an agent-less row that published a
    /// `config_state` is the external agent it must be.
    #[test]
    fn an_agent_less_acp_run_is_an_external_agent() {
        assert_eq!(agent_from_id(None, true), SessionAgent::External);
        assert_eq!(agent_from_id(Some("  "), true), SessionAgent::External);
        // A named agent keeps its own catalog on the ACP path.
        assert_eq!(agent_from_id(Some("codex"), true), SessionAgent::Codex);
        assert!(names("/", agent_from_id(None, true)).is_empty());
    }

    #[test]
    fn the_confirm_copy_names_the_command_in_the_title_and_the_button() {
        assert_eq!(confirm_title("clear"), "Run /clear?");
        assert_eq!(confirm_button("clear"), "Run /clear");
        assert_eq!(
            CONFIRM_BODY,
            "The agent forgets everything in this session so far. \
             Files in the worktree are kept."
        );
    }
}
