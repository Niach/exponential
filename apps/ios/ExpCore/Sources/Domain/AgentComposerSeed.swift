import Foundation

/// EXP-825: what a play button hands the Agent page composer. Every launcher
/// entry point is NAVIGATION now — the issue detail's Start coding, the bulk
/// bar, an action's Run, New action / a suggestion, a machine's play glyph,
/// the Fix conflicts pills — and this is the preselection it carries, mirrored
/// ×4 (web search params on `/t/$teamSlug/agent`, desktop
/// `Navigation::pending_chat_seed`, Android nav args).
///
/// Hashable so it can ride an `AppRoute` case: two pushes with different
/// seeds are two different destinations.
public struct AgentComposerSeed: Hashable, Sendable {
    /// Pre-checked issue ids (1 = a single-issue run, 2+ = a batch).
    public var issueIds: [String]
    /// A preselected action — a team row or one of the builtins
    /// (`builtin:create-action`, `builtin:fix-conflicts`). Wins over
    /// `issueIds` when both arrive (the web rule).
    public var actionId: String?
    /// The machine to preselect (the Devices tab's play glyph).
    public var deviceId: String?
    /// An issue linked to the open PR a `pr` input should pre-pick (ANY
    /// linked issue resolves — the picker normalizes by membership).
    public var prIssueId: String?
    /// Text dropped into an EMPTY draft — a suggestion's description, with
    /// its automation note appended when the seed carries a trigger.
    public var text: String?
    /// A curated icon name seeding the Create action builtin's `icon` input.
    public var icon: String?
    /// The team the subject belongs to — an issue opened from the Inbox,
    /// Reviews or Search can sit on a NON-active team, and the composer's
    /// pools are team-scoped, so the page aligns the active team to this
    /// before it builds (web parity: the play button routes to THAT team's
    /// `/t/$teamSlug/agent`). nil = the active team.
    public var teamId: String?

    public init(
        issueIds: [String] = [],
        actionId: String? = nil,
        deviceId: String? = nil,
        prIssueId: String? = nil,
        text: String? = nil,
        icon: String? = nil,
        teamId: String? = nil
    ) {
        self.issueIds = issueIds
        self.actionId = actionId
        self.deviceId = deviceId
        self.prIssueId = prIssueId
        self.text = text
        self.icon = icon
        self.teamId = teamId
    }

    /// The Chat FAB's seed: nothing preselected.
    public static let empty = AgentComposerSeed()

    /// Whether the seed names a subject at all.
    public var hasSubject: Bool {
        actionId != nil || !issueIds.isEmpty
    }

    /// The issue ids the composer should check — EMPTY when an action is
    /// seeded too, because `action` wins over `issues` (web parity).
    public var effectiveIssueIds: [String] {
        actionId == nil ? issueIds : []
    }
}
