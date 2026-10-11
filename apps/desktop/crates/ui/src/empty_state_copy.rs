//! EXP-1274 — the desktop's half of the ×4 empty-state copy contract
//! (`packages/domain-contract/fixtures/empty-state.json`). One const per
//! fixture id the IDE draws, named after the id; `*_DESCRIPTION` only where
//! the fixture carries one, `*_ACTION` names the page's primary button. The
//! test locks every const to the fixture byte for byte. Drawn with
//! `controls::empty_state` (page) / `controls::empty_state_list` (list).

pub(crate) const INBOX: &str = "All caught up";
pub(crate) const MY_ISSUES: &str = "No issues assigned to you";
pub(crate) const DRAFTS: &str = "No drafts";
pub(crate) const REVIEWS: &str = "No open pull requests";
pub(crate) const BOARDS: &str = "No boards yet";
pub(crate) const BOARDS_DESCRIPTION: &str = "Create a board to start tracking work.";
pub(crate) const BOARDS_ACTION: &str = "Create a board";
pub(crate) const ISSUES: &str = "No issues yet";
pub(crate) const ISSUES_DESCRIPTION: &str = "Create an issue to start tracking work.";
pub(crate) const NO_TEAM: &str = "No team yet";
pub(crate) const NO_TEAM_DESCRIPTION: &str = "Create a team, or join one with an invite link.";
pub(crate) const NO_SELECTION_ISSUE: &str = "No issue selected";
pub(crate) const NO_SELECTION_NOTIFICATION: &str = "No notification selected";
pub(crate) const ARCHIVED_BOARDS: &str = "No archived boards";
pub(crate) const NO_TEAM_SELECTED: &str = "No team selected";
pub(crate) const ACTION_RUNS: &str = "No runs yet";
pub(crate) const ACTION_TRIGGERS: &str = "No triggers yet";
pub(crate) const RECENT_RUNS: &str = "No recent runs";
pub(crate) const RUNNING_AGENTS: &str = "No agents running";
pub(crate) const DEVICES: &str = "No devices yet";
pub(crate) const MCP_SERVERS: &str = "No MCP servers yet";
pub(crate) const BUILTIN_TOOLS_NO_MATCH: &str = "No tools match";
pub(crate) const WIDGETS: &str = "No widgets yet";
pub(crate) const LABELS: &str = "No labels yet";
pub(crate) const STATUSES: &str = "No statuses yet";
pub(crate) const PASSKEYS: &str = "No passkeys yet";
pub(crate) const ATTACHMENTS: &str = "No attachments yet";
pub(crate) const CHANGED_FILES: &str = "No changed files";
pub(crate) const ACTIVITY: &str = "No activity yet";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const FIXTURE: &str =
        include_str!("../../../../../packages/domain-contract/fixtures/empty-state.json");

    fn state(id: &str) -> Value {
        let fixture: Value = serde_json::from_str(FIXTURE).expect("empty-state.json parses");
        fixture["states"][id].clone()
    }

    fn field(id: &str, key: &str) -> Option<String> {
        state(id)[key].as_str().map(str::to_string)
    }

    #[test]
    fn every_const_is_the_fixture_copy() {
        let titles: &[(&str, &str)] = &[
            ("inbox", INBOX),
            ("my-issues", MY_ISSUES),
            ("drafts", DRAFTS),
            ("reviews", REVIEWS),
            ("boards", BOARDS),
            ("issues", ISSUES),
            ("no-team", NO_TEAM),
            ("no-selection-issue", NO_SELECTION_ISSUE),
            ("no-selection-notification", NO_SELECTION_NOTIFICATION),
            ("archived-boards", ARCHIVED_BOARDS),
            ("no-team-selected", NO_TEAM_SELECTED),
            ("action-runs", ACTION_RUNS),
            ("action-triggers", ACTION_TRIGGERS),
            ("recent-runs", RECENT_RUNS),
            ("running-agents", RUNNING_AGENTS),
            ("devices", DEVICES),
            ("mcp-servers", MCP_SERVERS),
            ("builtin-tools-no-match", BUILTIN_TOOLS_NO_MATCH),
            ("widgets", WIDGETS),
            ("labels", LABELS),
            ("statuses", STATUSES),
            ("passkeys", PASSKEYS),
            ("attachments", ATTACHMENTS),
            ("changed-files", CHANGED_FILES),
            ("activity", ACTIVITY),
        ];
        for (id, title) in titles {
            assert_eq!(field(id, "title").as_deref(), Some(*title), "{id} title");
        }
        let descriptions: &[(&str, &str)] = &[
            ("boards", BOARDS_DESCRIPTION),
            ("issues", ISSUES_DESCRIPTION),
            ("no-team", NO_TEAM_DESCRIPTION),
        ];
        for (id, description) in descriptions {
            assert_eq!(field(id, "description").as_deref(), Some(*description), "{id}");
        }
        assert_eq!(field("boards", "action").as_deref(), Some(BOARDS_ACTION));
    }

    /// The states the IDE draws as titles only carry no description in the
    /// fixture either: a stray one there would go undrawn here.
    #[test]
    fn title_only_states_have_no_description() {
        for id in ["inbox", "my-issues", "drafts", "reviews", "no-selection-issue"] {
            assert!(field(id, "description").is_none(), "{id}");
        }
    }
}
