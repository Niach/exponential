//! #EXP-2079 (agent-settings.json `assistant.chatTeams`): THE TEAMS A CHAT'S
//! LISTS READ, on the desktop. A session of a team the viewer syncs reads
//! that one team. The Assistant's conversation sits in its member's personal
//! team, which never syncs (no `teams` row, no members, boards, issues,
//! prompts or MCP servers of its own), so nothing in it may be looked up by
//! the session's team: its `#` refs, `@` members, MCP pick, `/` prompts,
//! Link / New issue and links read the turn's teams
//! (`domain::agent_settings::assistant_chat_teams`).
//!
//! The teams travel as ONE key (`domain::agent_settings::chat_teams_key`):
//! a single team's key is its id, so every reader of a normal chat is
//! unchanged; the readers a chat reaches (`RefResolver::from_store`,
//! `store_completion_source`, `issue_refs_in_team`, `boards_in_team`,
//! `team_prompts`, `team_statuses`, the link opener) take a key. Whatever
//! needs ONE real team (a tRPC `teamId`, a membership check, an upload)
//! keeps the session's own team or takes [`first`].

use domain::agent_settings::{assistant_chat_teams, assistant_own_devices, chat_teams_first, chat_teams_key, chat_teams_of, cross_team_order};
use domain::rows::SessionRow;
use gpui::App;
use sync::Store;

/// The viewer's synced teams in the cross-team order.
pub(crate) fn synced_team_ids(cx: &App) -> Vec<String> {
    let Some(store) = Store::try_global(cx) else {
        return Vec::new();
    };
    let teams = store.collections().teams.read(cx);
    cross_team_order(teams.iter().map(|team| (team.id.as_str(), team.name.as_str())))
}

/// The chat teams of a session / agent of `own_team_id` whose conversation
/// root's Teams pick is `picked`.
pub(crate) fn team_ids(own_team_id: Option<&str>, picked: Option<&[String]>, cx: &App) -> Vec<String> {
    // The common case, asked per frame: a synced own team is the answer.
    if let Some(own) = own_team_id.filter(|id| !id.is_empty()) {
        if Store::try_global(cx).is_some_and(|store| store.collections().teams.read(cx).get(own).is_some()) {
            return vec![own.to_string()];
        }
    }
    assistant_chat_teams(own_team_id.filter(|id| !id.is_empty()), &synced_team_ids(cx), picked)
}

/// [`team_ids`] as the key the chat's readers take; `None` = no team at all.
pub(crate) fn key(own_team_id: Option<&str>, picked: Option<&[String]>, cx: &App) -> Option<String> {
    chat_teams_key(&team_ids(own_team_id, picked, cx))
}

/// The conversation root's stored Teams pick of `session` (its own while it
/// is the root).
fn root_pick(session: &SessionRow, cx: &App) -> Option<Vec<String>> {
    let root = session
        .root_id
        .as_deref()
        .filter(|root_id| *root_id != session.id)
        .and_then(|root_id| crate::session_data::session(root_id, cx));
    root.map_or_else(|| session.scope_team_ids.clone(), |root| root.scope_team_ids)
}

/// The chat teams key of a synced session row.
pub(crate) fn session_key(session: &SessionRow, cx: &App) -> Option<String> {
    key(session.team_id.as_deref(), root_pick(session, cx).as_deref(), cx)
}

/// The chat teams of a synced session row, in order.
pub(crate) fn session_team_ids(session: &SessionRow, cx: &App) -> Vec<String> {
    team_ids(session.team_id.as_deref(), root_pick(session, cx).as_deref(), cx)
}

/// A key's teams, owned.
pub(crate) fn teams_of(key: &str) -> Vec<String> {
    chat_teams_of(key).map(str::to_string).collect()
}

/// A key's first team (`chatTeams` (7)).
pub(crate) fn first(key: &str) -> &str {
    chat_teams_first(key)
}

/// `deviceCases`: the device list of `session` is the viewer's OWN machines
/// (no team rule): their own session in a team they do not sync (the
/// Assistant's conversation: the device-move picker, a suggestion card's
/// Start on).
pub(crate) fn own_devices(session: &SessionRow, cx: &App) -> bool {
    let me = crate::queries::active_account(cx).map(|account| account.user_id);
    let synced = session
        .team_id
        .as_deref()
        .is_some_and(|team_id| Store::try_global(cx).is_some_and(|store| store.collections().teams.read(cx).get(team_id).is_some()));
    assistant_own_devices(synced, me.is_some() && session.user_id == me)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    fn seed<T: sync::ShapeRow + serde::de::DeserializeOwned>(collection: &gpui::Entity<sync::Collection<T>>, rows: serde_json::Value, cx: &mut gpui::App) {
        collection.update(cx, |c, cx| {
            for row in rows.as_array().unwrap() {
                let row: T = serde_json::from_value(row.clone()).unwrap();
                c.seed(row.key(), row);
            }
            cx.notify();
        });
    }

    /// The state of the Assistant move: the member syncs two teams (each with
    /// a board, an issue of the SAME identifier, a prompt); the Assistant's
    /// conversation sits in their personal team, which has no `teams` row.
    fn world(cx: &mut gpui::App) {
        let store = sync::Store::open(cx, None, None);
        cx.set_global(store);
        let c = sync::Store::global(cx).collections().clone();
        seed(&c.teams, json!([{"id": "t-b", "name": "Beta", "slug": "beta"}, {"id": "t-a", "name": "alpha", "slug": "alpha"}]), cx);
        seed(
            &c.boards,
            json!([
                {"id": "b-b", "team_id": "t-b", "name": "Beta board", "slug": "bb", "prefix": "EXP"},
                {"id": "b-a", "team_id": "t-a", "name": "Alpha board", "slug": "ab", "prefix": "EXP"},
            ]),
            cx,
        );
        seed(
            &c.issues,
            json!([
                {"id": "i-b1", "board_id": "b-b", "number": 1, "identifier": "EXP-1", "title": "Beta's", "status": "backlog"},
                {"id": "i-a1", "board_id": "b-a", "number": 1, "identifier": "EXP-1", "title": "Alpha's", "status": "backlog"},
                {"id": "i-b2", "board_id": "b-b", "number": 2, "identifier": "EXP-2", "title": "Only Beta's", "status": "backlog"},
            ]),
            cx,
        );
        seed(
            &c.prompts,
            json!([
                {"id": "p-b", "team_id": "t-b", "name": "beta-review", "sort_order": 0},
                {"id": "p-a", "team_id": "t-a", "name": "alpha-review", "sort_order": 5},
            ]),
            cx,
        );
        seed(
            &c.sessions,
            json!([
                {"id": "assistant", "team_id": "p-me", "root_id": "assistant", "agent_id": "ag", "user_id": "me", "status": "idle"},
                {"id": "picked", "team_id": "p-me", "root_id": "picked", "agent_id": "ag", "user_id": "me", "status": "idle", "scope_team_ids": ["t-b", "t-gone"]},
                {"id": "child", "team_id": "p-me", "root_id": "picked", "parent_id": "picked", "user_id": "me", "status": "idle"},
                {"id": "plain", "team_id": "t-b", "root_id": "plain", "user_id": "me", "status": "idle", "scope_team_ids": ["t-a"]},
            ]),
            cx,
        );
    }

    fn session(id: &str, cx: &gpui::App) -> domain::rows::SessionRow {
        crate::session_data::session(id, cx).unwrap()
    }

    #[gpui::test]
    fn a_chat_reads_its_own_team_or_the_assistants_turn_teams(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            world(cx);
            assert_eq!(super::synced_team_ids(cx), ["t-a", "t-b"], "by name, case-insensitive");
            // A session of a synced team: that one team, its id unchanged (a
            // plain chat's Teams pick never widens its lists).
            assert_eq!(super::session_key(&session("plain", cx), cx).as_deref(), Some("t-b"));
            // The Assistant's conversation: every synced team, in order.
            assert_eq!(super::session_key(&session("assistant", cx), cx).as_deref(), Some("t-a+t-b"));
            // Its Teams pick ∩ the synced teams; a child reads its root's.
            assert_eq!(super::session_key(&session("picked", cx), cx).as_deref(), Some("t-b"));
            assert_eq!(super::session_team_ids(&session("child", cx), cx), ["t-b"]);
        });
    }

    #[gpui::test]
    fn the_readers_of_a_chat_take_the_chat_teams_key(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            world(cx);
            let key = super::session_key(&session("assistant", cx), cx).unwrap();
            let c = sync::Store::global(cx).collections().clone();
            // `#` refs: the issues of both teams; a clash is the first team's.
            assert_eq!(c.issue_refs_in_team(&key, cx).len(), 3);
            assert_eq!(crate::description_editor::issue_id_by_identifier(&key, "exp-1", cx).as_deref(), Some("i-a1"));
            assert_eq!(crate::description_editor::issue_id_by_identifier("t-b+t-a", "EXP-1", cx).as_deref(), Some("i-b1"));
            assert_eq!(crate::description_editor::issue_id_by_identifier(&key, "EXP-2", cx).as_deref(), Some("i-b2"));
            // Boards and `/` prompts: team by team in the key's order.
            assert_eq!(c.boards_in_team(&key, cx).iter().map(|board| board.id.as_str()).collect::<Vec<_>>(), ["b-a", "b-b"]);
            let prompts = crate::queries::team_prompts(cx, &key).0;
            assert_eq!(prompts.iter().map(|prompt| prompt.name.as_str()).collect::<Vec<_>>(), ["alpha-review", "beta-review"]);
            // One team's key reads exactly that team.
            assert_eq!(c.issue_refs_in_team("t-b", cx).len(), 2);
            assert_eq!(crate::queries::team_prompts(cx, "t-b").0.len(), 1);
            // Start's `#IDENT` refs of a card in the conversation.
            let idents = crate::card_taps::team_idents_for_test("assistant", cx);
            assert_eq!(idents.get("EXP-1").map(String::as_str), Some("i-a1"));
            assert_eq!(idents.get("EXP-2").map(String::as_str), Some("i-b2"));
        });
    }
}
