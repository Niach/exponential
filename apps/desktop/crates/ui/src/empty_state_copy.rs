//! EXP-1274 — the desktop's half of the ×4 empty-state copy contract
//! (`packages/domain-contract/fixtures/empty-state.json`). One const per
//! fixture id the IDE draws, named after the id; `*_DESCRIPTION` only where
//! the fixture carries one, `*_ACTION` names the page's primary button. The
//! test locks every const to the fixture byte for byte. Drawn with
//! `controls::empty_state` (page) / `controls::empty_state_list` (list).

pub(crate) const INBOX: &str = "All caught up";
pub(crate) const MY_ISSUES: &str = "No issues assigned to you";
pub(crate) const DRAFTS: &str = "No drafts";
// EXP-1294: the Threads page (the empty-state.json `threads` state).
pub(crate) const THREADS: &str = "No sessions yet";
pub(crate) const BOARDS: &str = "No boards yet";
pub(crate) const BOARDS_DESCRIPTION: &str = "Create a board to start tracking work.";
pub(crate) const BOARDS_ACTION: &str = "Create a board";
pub(crate) const ISSUES: &str = "No issues yet";
pub(crate) const ISSUES_DESCRIPTION: &str = "Create an issue to start tracking work.";
pub(crate) const ISSUES_ACTION: &str = "New issue";
pub(crate) const NO_TEAM: &str = "No team yet";
pub(crate) const NO_TEAM_DESCRIPTION: &str = "Create a team, or join one with an invite link.";
pub(crate) const NO_SELECTION_ISSUE: &str = "No issue selected";
pub(crate) const NO_SELECTION_NOTIFICATION: &str = "No notification selected";
pub(crate) const BOARD_SWITCHER: &str = "No boards yet";
pub(crate) const ARCHIVED_BOARDS: &str = "No archived boards";
pub(crate) const NO_TEAM_SELECTED: &str = "No team selected";
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
// EXP-1284: the thread page whose thread is not (or no longer) there.
pub(crate) const THREAD_NOT_FOUND: &str = "Thread not found";
pub(crate) const THREAD_NOT_FOUND_DESCRIPTION: &str = "It may still be syncing, or it was deleted.";
pub(crate) const THREAD_NOT_FOUND_ACTION: &str = "Go to Agent";

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
            ("threads", THREADS),
            ("boards", BOARDS),
            ("issues", ISSUES),
            ("no-team", NO_TEAM),
            ("no-selection-issue", NO_SELECTION_ISSUE),
            ("no-selection-notification", NO_SELECTION_NOTIFICATION),
            ("board-switcher", BOARD_SWITCHER),
            ("archived-boards", ARCHIVED_BOARDS),
            ("no-team-selected", NO_TEAM_SELECTED),
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
            ("thread-not-found", THREAD_NOT_FOUND),
        ];
        for (id, title) in titles {
            assert_eq!(field(id, "title").as_deref(), Some(*title), "{id} title");
        }
        let descriptions: &[(&str, &str)] = &[
            ("boards", BOARDS_DESCRIPTION),
            ("issues", ISSUES_DESCRIPTION),
            ("no-team", NO_TEAM_DESCRIPTION),
            ("thread-not-found", THREAD_NOT_FOUND_DESCRIPTION),
        ];
        for (id, description) in descriptions {
            assert_eq!(field(id, "description").as_deref(), Some(*description), "{id}");
        }
        assert_eq!(field("boards", "action").as_deref(), Some(BOARDS_ACTION));
        assert_eq!(field("issues", "action").as_deref(), Some(ISSUES_ACTION));
        assert_eq!(field("thread-not-found", "action").as_deref(), Some(THREAD_NOT_FOUND_ACTION));
        // The page state's glyph is the `work-faces` concept (registry).
        assert_eq!(field("thread-not-found", "icon").as_deref(), Some("work-faces"));
    }

    /// The states the IDE draws as titles only carry no description in the
    /// fixture either: a stray one there would go undrawn here.
    #[test]
    fn title_only_states_have_no_description() {
        for id in ["inbox", "my-issues", "drafts", "threads", "no-selection-issue"] {
            assert!(field(id, "description").is_none(), "{id}");
        }
    }
}
