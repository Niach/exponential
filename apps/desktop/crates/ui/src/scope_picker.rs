//! FEED-76 — the team/board scope picker, the web `@exp/ui` `ScopePicker`
//! twin's data and rules: what a credential may touch. An "Everything"
//! switch (all teams and boards, including ones created later) over the member's teams, each a
//! whole-team checkbox with its boards indented beneath — a board under a
//! ticked team reads checked and disabled because the team covers it. The
//! Create-API-key dialog (`settings::api_keys::MintKeyContent`, an eUI
//! document since #EXP-1629) draws it; the MCP OAuth
//! consent screen is web-only mid-authorize, so the IDE has the one caller.
//!
//! The RAW selection lives with the dialog; what is SENT is [`effective_scope_selection`]
//! — boards a selected whole team already covers drop out, "Everything"
//! clears both lists — applied at submit, so unticking a team restores the
//! boards the user had ticked under it. The wire shape is
//! `api::users::ScopeSelection`, byte for byte the web's `scope` input.

use gpui::App;

pub(crate) use api::users::ScopeSelection;


/// One board under its team.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScopeBoard {
    pub id: String,
    pub name: String,
    pub prefix: String,
}

/// One team with its live boards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScopeTeam {
    pub id: String,
    pub name: String,
    pub boards: Vec<ScopeBoard>,
}

impl ScopeTeam {
    /// The member's synced teams (name-sorted) with their boards in the
    /// sidebar's order — the trash/archive never reach the boards shape, so
    /// this is the same live tree the web's `mcpGrants.scopeTree` serves.
    /// Needs the installed `sync::Store` (never the styleguide).
    pub(crate) fn from_store(cx: &App) -> Vec<Self> {
        let collections = sync::Store::global(cx).collections();
        collections
            .teams_sorted(cx)
            .into_iter()
            .map(|team| Self {
                boards: collections
                    .boards_in_team(&team.id, cx)
                    .into_iter()
                    .map(|board| ScopeBoard {
                        id: board.id,
                        name: board.name,
                        prefix: board.prefix.unwrap_or_default(),
                    })
                    .collect(),
                id: team.id,
                name: team.name,
            })
            .collect()
    }
}

/// What to send: boards covered by a selected whole team are dropped; an
/// all-teams pick carries no ids at all (web `effectiveScopeSelection`).
pub(crate) fn effective_scope_selection(
    tree: &[ScopeTeam],
    value: &ScopeSelection,
) -> ScopeSelection {
    if value.all_teams {
        return ScopeSelection::everything();
    }
    let covered: Vec<&str> = tree
        .iter()
        .filter(|team| value.team_ids.iter().any(|id| id == &team.id))
        .flat_map(|team| team.boards.iter().map(|board| board.id.as_str()))
        .collect();
    ScopeSelection {
        all_teams: false,
        team_ids: value.team_ids.clone(),
        board_ids: value
            .board_ids
            .iter()
            .filter(|id| !covered.contains(&id.as_str()))
            .cloned()
            .collect(),
    }
}

/// The web `toggle`: add once / remove, order kept.
pub(crate) fn toggle(list: &mut Vec<String>, id: &str, on: bool) {
    let present = list.iter().any(|item| item == id);
    if on && !present {
        list.push(id.to_string());
    } else if !on {
        list.retain(|item| item != id);
    }
}

/// The copy, the web's byte for byte.
pub(crate) mod copy {
    pub(crate) const EVERYTHING: &str = "Everything";
    pub(crate) const EVERYTHING_HINT: &str =
        "All teams and boards, including ones created later.";
    pub(crate) const WHOLE_TEAM: &str = "whole team";
    pub(crate) const NO_TEAMS: &str = "You aren't a member of any team yet.";
    pub(crate) const NO_BOARDS: &str = "No boards yet.";
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The web test's tree.
    pub(crate) fn tree() -> Vec<ScopeTeam> {
        vec![
            ScopeTeam {
                id: "t-acme".into(),
                name: "Acme".into(),
                boards: vec![
                    ScopeBoard { id: "b-web".into(), name: "Web".into(), prefix: "WEB".into() },
                    ScopeBoard { id: "b-mob".into(), name: "Mobile".into(), prefix: "MOB".into() },
                ],
            },
            ScopeTeam { id: "t-lab".into(), name: "Lab".into(), boards: vec![] },
        ]
    }

    fn scoped(team_ids: &[&str], board_ids: &[&str]) -> ScopeSelection {
        ScopeSelection {
            all_teams: false,
            team_ids: team_ids.iter().map(|id| id.to_string()).collect(),
            board_ids: board_ids.iter().map(|id| id.to_string()).collect(),
        }
    }

    #[test]
    fn effective_drops_boards_a_selected_whole_team_already_covers() {
        assert_eq!(
            effective_scope_selection(&tree(), &scoped(&["t-acme"], &["b-web"])),
            scoped(&["t-acme"], &[])
        );
    }

    #[test]
    fn effective_keeps_boards_outside_the_selected_teams() {
        assert_eq!(
            effective_scope_selection(&tree(), &scoped(&["t-lab"], &["b-web"])),
            scoped(&["t-lab"], &["b-web"])
        );
    }

    #[test]
    fn everything_carries_no_ids_at_all() {
        let value = ScopeSelection {
            all_teams: true,
            team_ids: vec!["t-acme".into()],
            board_ids: vec!["b-web".into()],
        };
        assert_eq!(effective_scope_selection(&tree(), &value), ScopeSelection::everything());
    }

    #[test]
    fn toggle_adds_once_and_removes() {
        let mut list = Vec::new();
        toggle(&mut list, "a", true);
        toggle(&mut list, "a", true);
        toggle(&mut list, "b", true);
        assert_eq!(list, vec!["a".to_string(), "b".to_string()]);
        toggle(&mut list, "a", false);
        assert_eq!(list, vec!["b".to_string()]);
        toggle(&mut list, "zzz", false);
        assert_eq!(list, vec!["b".to_string()]);
    }

    #[test]
    fn unticking_a_team_restores_the_boards_ticked_under_it() {
        let mut value = scoped(&["t-acme"], &["b-web"]);
        // Raw keeps the covered board; the effective pick drops it …
        assert_eq!(effective_scope_selection(&tree(), &value), scoped(&["t-acme"], &[]));
        // … and un-ticking the team brings it back into the pick.
        toggle(&mut value.team_ids, "t-acme", false);
        assert_eq!(effective_scope_selection(&tree(), &value), scoped(&[], &["b-web"]));
        assert!(effective_scope_selection(&tree(), &value).has_selection());
        toggle(&mut value.board_ids, "b-web", false);
        assert!(!effective_scope_selection(&tree(), &value).has_selection());
    }
}
