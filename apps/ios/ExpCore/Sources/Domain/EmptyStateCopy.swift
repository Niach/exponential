import Foundation

/// EXP-1274: the ×4 empty-state copy, locked against
/// `packages/domain-contract/fixtures/empty-state.json` (`EmptyStateCopyTests`;
/// web `lib/empty-states.ts`, desktop `empty_state_copy.rs`, Android
/// `EmptyStateCopy.kt`). ExpUI's `EmptyState(.inbox)` draws one (the states are `State` statics so call
/// sites read `.inbox`). `icon` is the
/// registry CONCEPT id (`packages/icons/icons.json`), resolved to a glyph in
/// ExpUI. Titles are at most four words and never end in a period; only a
/// `page` state carries a description or an action.
public enum EmptyStateCopy {
    public enum Size: String, Sendable, CaseIterable {
        case page, list
    }

    public struct State: Sendable, Equatable, Identifiable {
        public let id: String
        public let size: Size
        /// The icon CONCEPT id (`nav-inbox`).
        public let icon: String
        public let title: String
        public let description: String?
        /// The primary button's label, when the client can offer it.
        public let action: String?

        public init(id: String, size: Size, icon: String, title: String, description: String? = nil, action: String? = nil) {
            self.id = id
            self.size = size
            self.icon = icon
            self.title = title
            self.description = description
            self.action = action
        }
    }

    /// Every declared state (the test walks it against the fixture).
    public static let all: [State] = [
        .inbox,
        .myIssues,
        .drafts,
        .threads,
        .boards,
        .issues,
        .noTeam,
        .runNotFound,
        .threadNotFound,
        .noSelectionIssue,
        .noSelectionNotification,
        .boardSwitcher,
        .boardIssues,
        .teamBoards,
        .archivedBoards,
        .teams,
        .noTeamSelected,
        .actionRuns,
        .actionTriggers,
        .recentRuns,
        .runningAgents,
        .devices,
        .signedInDevices,
        .mcpServers,
        .builtinToolsNoMatch,
        .widgets,
        .labels,
        .statuses,
        .passkeys,
        .attachments,
        .transcript,
        .changedFiles,
        .guide,
        .activity,
    ]
}

public extension EmptyStateCopy.State {
    static let inbox = Self(
        id: "inbox", size: .page, icon: "nav-inbox",
        title: "All caught up", description: nil, action: nil
    )
    static let myIssues = Self(
        id: "my-issues", size: .page, icon: "nav-my-issues",
        title: "No issues assigned to you", description: nil, action: nil
    )
    static let drafts = Self(
        id: "drafts", size: .page, icon: "nav-drafts",
        title: "No drafts", description: nil, action: nil
    )
    static let threads = Self(
        id: "threads", size: .page, icon: "nav-threads",
        title: "No sessions yet", description: nil, action: nil
    )
    static let boards = Self(
        id: "boards", size: .page, icon: "nav-boards",
        title: "No boards yet", description: "Create a board to start tracking work.", action: "Create a board"
    )
    static let issues = Self(
        id: "issues", size: .page, icon: "nav-issues",
        title: "No issues yet", description: "Create an issue to start tracking work.", action: "New issue"
    )
    static let noTeam = Self(
        id: "no-team", size: .page, icon: "ui-team",
        title: "No team yet", description: "Create a team, or join one with an invite link.", action: nil
    )
    static let runNotFound = Self(
        id: "run-not-found", size: .page, icon: "action-run",
        title: "Session not found", description: nil, action: "Go to Agent"
    )
    /// EXP-1284: a thread the store does not hold (and `threads.get` refused).
    static let threadNotFound = Self(
        id: "thread-not-found", size: .page, icon: "work-faces",
        title: "Chat not found", description: "It may still be syncing, or it was deleted.",
        action: "Go to Agent"
    )
    static let noSelectionIssue = Self(
        id: "no-selection-issue", size: .page, icon: "ui-issue",
        title: "No issue selected", description: nil, action: nil
    )
    static let noSelectionNotification = Self(
        id: "no-selection-notification", size: .page, icon: "nav-inbox",
        title: "No notification selected", description: nil, action: nil
    )
    static let boardSwitcher = Self(
        id: "board-switcher", size: .list, icon: "nav-boards",
        title: "No boards yet", description: nil, action: nil
    )
    static let boardIssues = Self(
        id: "board-issues", size: .list, icon: "nav-issues",
        title: "No issues", description: nil, action: nil
    )
    static let teamBoards = Self(
        id: "team-boards", size: .list, icon: "settings-boards",
        title: "No boards yet", description: nil, action: nil
    )
    static let archivedBoards = Self(
        id: "archived-boards", size: .list, icon: "ui-archive",
        title: "No archived boards", description: nil, action: nil
    )
    static let teams = Self(
        id: "teams", size: .list, icon: "ui-team",
        title: "No teams yet", description: nil, action: nil
    )
    static let noTeamSelected = Self(
        id: "no-team-selected", size: .list, icon: "ui-team",
        title: "No team selected", description: nil, action: nil
    )
    static let actionRuns = Self(
        id: "action-runs", size: .list, icon: "action-run",
        title: "No sessions yet", description: nil, action: nil
    )
    static let actionTriggers = Self(
        id: "action-triggers", size: .list, icon: "trigger-event",
        title: "No triggers yet", description: nil, action: nil
    )
    static let recentRuns = Self(
        id: "recent-runs", size: .list, icon: "coding-ended",
        title: "No recent sessions", description: nil, action: nil
    )
    static let runningAgents = Self(
        id: "running-agents", size: .list, icon: "coding-running",
        title: "No agents running", description: nil, action: nil
    )
    static let devices = Self(
        id: "devices", size: .list, icon: "nav-devices",
        title: "No devices yet", description: nil, action: nil
    )
    static let signedInDevices = Self(
        id: "signed-in-devices", size: .list, icon: "ui-device",
        title: "No signed-in devices", description: nil, action: nil
    )
    static let mcpServers = Self(
        id: "mcp-servers", size: .list, icon: "settings-mcp",
        title: "No MCP servers yet", description: nil, action: nil
    )
    static let builtinToolsNoMatch = Self(
        id: "builtin-tools-no-match", size: .list, icon: "ui-empty-search",
        title: "No tools match", description: nil, action: nil
    )
    static let widgets = Self(
        id: "widgets", size: .list, icon: "settings-widget",
        title: "No widgets yet", description: nil, action: nil
    )
    static let labels = Self(
        id: "labels", size: .list, icon: "settings-labels",
        title: "No labels yet", description: nil, action: nil
    )
    static let statuses = Self(
        id: "statuses", size: .list, icon: "settings-statuses",
        title: "No statuses yet", description: nil, action: nil
    )
    static let passkeys = Self(
        id: "passkeys", size: .list, icon: "auth-passkey",
        title: "No passkeys yet", description: nil, action: nil
    )
    static let attachments = Self(
        id: "attachments", size: .list, icon: "ui-attach",
        title: "No attachments yet", description: nil, action: nil
    )
    static let transcript = Self(
        id: "transcript", size: .list, icon: "coding-assistant",
        title: "Nothing from this agent yet", description: nil, action: nil
    )
    static let changedFiles = Self(
        id: "changed-files", size: .list, icon: "coding-diff",
        title: "No changed files", description: nil, action: nil
    )
    static let guide = Self(
        id: "guide", size: .list, icon: "work-results",
        title: "No report yet", description: nil, action: nil
    )
    static let activity = Self(
        id: "activity", size: .list, icon: "ui-clock",
        title: "No activity yet", description: nil, action: nil
    )
}
