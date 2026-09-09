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
//! * **the confirm copy** for the context-discarding row (`/clear`).
//!   The publisher executes whatever it receives, so asking first is entirely
//!   the client's job — on all four of them, in the same words.

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
/// engine publishes one, and an ACP run for claude/codex/pi always stamps its
/// id, so an agent-less one is an EXTERNAL agent — the synced column takes
/// contract values only and has none for it — and its curated catalog is
/// empty. Mirrored ×4 (`steerAgentId` on web, `SlashCommands.agentId` on iOS
/// and Android).
pub(crate) fn agent_of(row: &domain::rows::CodingSession, acp: bool) -> SessionAgent {
    agent_from_id(row.agent.as_deref(), acp)
}

/// The pure half of [`agent_of`] — `coding_sessions.agent` is a raw wire
/// string, and an unknown one is claude like an absent one.
fn agent_from_id(agent: Option<&str>, acp: bool) -> SessionAgent {
    match agent.map(str::trim) {
        Some(id) if id.eq_ignore_ascii_case("codex") => SessionAgent::Codex,
        Some(id) if id.eq_ignore_ascii_case("pi") => SessionAgent::Pi,
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

/// One row of the `/` menu. Owned, unlike [`SteerCommand`]'s `&'static str`s,
/// because an ACP session's own commands arrive on the wire
/// (`config_state.commands`) and are as real a menu row as a contract one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MenuCommand {
    pub(crate) name: String,
    pub(crate) description: String,
    /// Empty = the command takes no argument.
    pub(crate) arg_hint: String,
    /// The client confirms before sending (context is discarded). Only the
    /// contract knows this — an agent-advertised command never confirms,
    /// because nothing here knows what it does.
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

/// EXP-746 — the same menu over the contract catalog UNION the commands the
/// AGENT advertised for this run (`config_state.commands`, ACP
/// `available_commands_update`).
///
/// Contract rows come FIRST because those are the ones the publisher is known
/// to be able to execute on every transport; an agent row that shadows a
/// contract name is dropped rather than listed twice (`/compact` is one
/// command whoever ends up running it). Mirrored ×4 as `mergeAgentCommands`.
pub(crate) fn menu_matches_with(
    draft: &str,
    agent: SessionAgent,
    extra: &[steer::frames::ConfigCommand],
) -> Vec<MenuCommand> {
    let Some(query) = slash_query(draft) else {
        return Vec::new();
    };
    let needle = query.to_ascii_lowercase();
    let mut rows: Vec<MenuCommand> = catalog_for(agent)
        .into_iter()
        .map(MenuCommand::from)
        .collect();
    for command in extra {
        let name = command.name.trim();
        if name.is_empty()
            || rows
                .iter()
                .any(|row| row.name.eq_ignore_ascii_case(name))
        {
            continue;
        }
        rows.push(MenuCommand {
            name: name.to_string(),
            description: command.description.clone(),
            arg_hint: command.hint.clone().unwrap_or_default(),
            confirm: false,
        });
    }
    rows.retain(|row| row.name.to_ascii_lowercase().starts_with(&needle));
    rows.truncate(MENU_LIMIT);
    rows
}

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

    /// The menu with no agent commands at all — every catalog assertion
    /// below is about the contract half.
    fn names(draft: &str, agent: SessionAgent) -> Vec<String> {
        agent_names(draft, agent, &[])
    }

    fn agent_names(
        draft: &str,
        agent: SessionAgent,
        extra: &[steer::frames::ConfigCommand],
    ) -> Vec<String> {
        menu_matches_with(draft, agent, extra)
            .into_iter()
            .map(|command| command.name)
            .collect()
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

    #[test]
    fn an_empty_query_lists_the_agents_whole_catalog_in_contract_order() {
        let all = names("/", SessionAgent::Claude);
        assert_eq!(all, vec!["compact", "clear"]);
        // Every agent sees the same two rows (the desktop maps `/clear` per
        // agent — pi runs it natively).
        assert_eq!(names("/", SessionAgent::Codex), all);
        assert_eq!(names("/", SessionAgent::Pi), all);
    }

    #[test]
    fn the_filter_is_a_case_insensitive_name_prefix() {
        assert_eq!(names("/co", SessionAgent::Claude), vec!["compact"]);
        assert_eq!(names("/CO", SessionAgent::Claude), vec!["compact"]);
        assert_eq!(names("/compact", SessionAgent::Claude), vec!["compact"]);
        // A prefix, never a substring: `pact` is inside `compact`, not in front.
        assert!(names("/pact", SessionAgent::Claude).is_empty());
        // A closed token draws nothing at all.
        assert!(names("/compact ", SessionAgent::Claude).is_empty());
        assert!(names("hello", SessionAgent::Claude).is_empty());
    }

    #[test]
    fn accepting_adds_a_trailing_space_only_when_there_is_an_argument() {
        let compact = menu_matches_with("/compact", SessionAgent::Claude, &[]).remove(0);
        assert_eq!(compact.arg_hint, "instructions");
        assert_eq!(insertion(&compact), "/compact ");
        let clear = menu_matches_with("/clear", SessionAgent::Claude, &[]).remove(0);
        assert_eq!(clear.arg_hint, "");
        assert_eq!(insertion(&clear), "/clear");
    }

    // ── EXP-746: the contract ∪ agent union (×4 `mergeAgentCommands`) ──────

    fn agent_command(name: &str, hint: Option<&str>) -> steer::frames::ConfigCommand {
        steer::frames::ConfigCommand {
            hint: hint.map(str::to_string),
            ..steer::frames::ConfigCommand::new(name, "an agent command")
        }
    }

    #[test]
    fn contract_rows_come_first() {
        assert_eq!(
            agent_names(
                "/",
                SessionAgent::Claude,
                &[agent_command("agents", None), agent_command("review", None)]
            ),
            vec!["compact", "clear", "agents", "review"]
        );
        // The prefix filter applies to the whole union, not just the contract
        // half.
        assert_eq!(
            agent_names("/a", SessionAgent::Claude, &[agent_command("agents", None)]),
            vec!["agents"]
        );
    }

    #[test]
    fn an_agent_command_that_shadows_a_contract_name_is_dropped() {
        let rows = menu_matches_with(
            "/",
            SessionAgent::Claude,
            &[
                agent_command("COMPACT", Some("only what matters")),
                agent_command("  ", None),
            ],
        );
        assert_eq!(
            rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
            vec!["compact", "clear"]
        );
        // …and the CONTRACT row survives whole: its hint and its confirm are
        // what the publisher acts on.
        assert_eq!(rows[0].arg_hint, "instructions");
    }

    #[test]
    fn the_union_respects_the_menu_limit() {
        let extra: Vec<steer::frames::ConfigCommand> = (0..20)
            .map(|n| agent_command(&format!("cmd{n}"), None))
            .collect();
        assert_eq!(
            menu_matches_with("/", SessionAgent::Claude, &extra).len(),
            MENU_LIMIT
        );
    }

    /// An external ACP agent has no contract catalog at all (`agent_id`
    /// returns an id `steerCommands` cannot name) — its menu is exactly what
    /// the agent advertised.
    #[test]
    fn an_external_agent_offers_only_its_own_commands() {
        assert!(names("/", SessionAgent::External).is_empty());
        assert_eq!(
            agent_names("/", SessionAgent::External, &[agent_command("review", None)]),
            vec!["review"]
        );
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
        assert_eq!(agent_from_id(Some(" pi "), false), SessionAgent::Pi);
        assert_eq!(agent_from_id(Some("claude"), false), SessionAgent::Claude);
        assert_eq!(
            agent_from_id(Some("something-else"), false),
            SessionAgent::Claude
        );
    }

    /// EXP-746, mirrored ×4 under this name (web `steerAgentId`, iOS
    /// `testAnAgentLessAcpRunIsAnExternalAgent`, Android
    /// `an agent-less acp run is an external agent`): a REMOTE viewer cannot
    /// ask the run what it is, so an agent-less row that published a
    /// `config_state` is the external agent it must be — otherwise the menu
    /// offered claude's `/clear`, confirm dialog and all, and the text
    /// reached the agent as an ordinary prompt.
    #[test]
    fn an_agent_less_acp_run_is_an_external_agent() {
        assert_eq!(agent_from_id(None, true), SessionAgent::External);
        assert_eq!(agent_from_id(Some("  "), true), SessionAgent::External);
        // A named agent keeps its own catalog on the ACP path.
        assert_eq!(agent_from_id(Some("codex"), true), SessionAgent::Codex);
        // ...and the menu it leaves is the agent's own commands, nothing else.
        assert!(names("/", agent_from_id(None, true)).is_empty());
        assert_eq!(
            agent_names(
                "/",
                agent_from_id(None, true),
                &[agent_command("review", None)]
            ),
            vec!["review"]
        );
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
