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
//! * **the confirm copy** for the context-discarding rows (`/clear`, `/new`).
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
pub(crate) fn agent_of(row: &domain::rows::CodingSession) -> SessionAgent {
    agent_from_id(row.agent.as_deref())
}

/// The pure half of [`agent_of`] — `coding_sessions.agent` is a raw wire
/// string, and an unknown one is claude like an absent one.
fn agent_from_id(agent: Option<&str>) -> SessionAgent {
    match agent.map(str::trim) {
        Some(id) if id.eq_ignore_ascii_case("codex") => SessionAgent::Codex,
        Some(id) if id.eq_ignore_ascii_case("pi") => SessionAgent::Pi,
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

/// The rows to draw for `draft`. Empty = no menu (either the draft is not a
/// command token, or nothing in the agent's catalog matches).
pub(crate) fn menu_matches(draft: &str, agent: SessionAgent) -> Vec<SteerCommand> {
    let Some(query) = slash_query(draft) else {
        return Vec::new();
    };
    let needle = query.to_ascii_lowercase();
    catalog_for(agent)
        .into_iter()
        .filter(|command| command.name.to_ascii_lowercase().starts_with(&needle))
        .take(MENU_LIMIT)
        .collect()
}

/// What accepting `command` puts in the composer — with the trailing space
/// when there is an argument to type, without when there is not.
pub(crate) fn insertion(command: &SteerCommand) -> String {
    if command.arg_hint.is_empty() {
        format!("/{}", command.name)
    } else {
        format!("/{} ", command.name)
    }
}

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

    fn names(draft: &str, agent: SessionAgent) -> Vec<&'static str> {
        menu_matches(draft, agent)
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
        assert_eq!(all, vec!["compact", "clear", "model", "init", "review"]);
        // `/new` is codex/pi's twin of `/clear` — neither agent sees both.
        assert!(names("/", SessionAgent::Codex).contains(&"new"));
        assert!(!names("/", SessionAgent::Codex).contains(&"clear"));
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
        let compact = menu_matches("/compact", SessionAgent::Claude)[0];
        assert_eq!(compact.arg_hint, "instructions");
        assert_eq!(insertion(&compact), "/compact ");
        let clear = menu_matches("/clear", SessionAgent::Claude)[0];
        assert_eq!(clear.arg_hint, "");
        assert_eq!(insertion(&clear), "/clear");
    }

    #[test]
    fn a_row_without_a_known_agent_steers_a_claude_session() {
        assert_eq!(agent_from_id(None), SessionAgent::Claude);
        assert_eq!(agent_from_id(Some("codex")), SessionAgent::Codex);
        assert_eq!(agent_from_id(Some(" pi ")), SessionAgent::Pi);
        assert_eq!(agent_from_id(Some("claude")), SessionAgent::Claude);
        assert_eq!(agent_from_id(Some("something-else")), SessionAgent::Claude);
    }

    #[test]
    fn the_confirm_copy_names_the_command_in_the_title_and_the_button() {
        assert_eq!(confirm_title("clear"), "Run /clear?");
        assert_eq!(confirm_button("new"), "Run /new");
        assert_eq!(
            CONFIRM_BODY,
            "The agent forgets everything in this session so far. \
             Files in the worktree are kept."
        );
    }
}
