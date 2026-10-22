import Foundation

/// EXP-825: what a play button hands the home composer. Every launcher entry
/// point is NAVIGATION — the issue's Start, the bulk bar, a prompt, a
/// machine's play glyph — and this is the preselection it carries; the send
/// is ONE `sessions.send` (EXP-1291) in the ACTIVE team.
///
/// Hashable so it can ride an `AppRoute` case: two pushes with different
/// seeds are two different destinations.
public struct AgentComposerSeed: Hashable, Sendable {
    /// Pre-checked issue ids (the new thread's `issueIds`).
    public var issueIds: [String]
    /// A preselected team prompt (NEW-2). Wins over `issueIds` when both arrive (the
    /// web rule).
    public var promptId: String?
    /// The machine to preselect (the Devices tab's play glyph).
    public var deviceId: String?
    /// Text dropped into an EMPTY draft.
    public var text: String?
    /// The team the subject belongs to — an issue opened from the Inbox or
    /// Search can sit on a NON-active team, and the composer's pools are
    /// team-scoped, so the page aligns the active team to this before it
    /// builds. nil = the active team.
    public var teamId: String?

    public init(
        issueIds: [String] = [],
        promptId: String? = nil,
        deviceId: String? = nil,
        text: String? = nil,
        teamId: String? = nil
    ) {
        self.issueIds = issueIds
        self.promptId = promptId
        self.deviceId = deviceId
        self.text = text
        self.teamId = teamId
    }

    /// The composer FAB's seed: nothing preselected.
    public static let empty = AgentComposerSeed()

    /// Whether the seed names a subject at all.
    public var hasSubject: Bool {
        promptId != nil || !issueIds.isEmpty
    }

    /// The issue ids the composer should check — EMPTY when a prompt is
    /// seeded too, because `prompt` wins over `issues` (web parity).
    public var effectiveIssueIds: [String] {
        promptId == nil ? issueIds : []
    }
}
