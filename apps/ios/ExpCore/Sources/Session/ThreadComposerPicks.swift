import Foundation

/// EXP-1334: the mid-run picks a thread composer sent (`sessions.configure`),
/// nil = the synced row's own value stands. A pick shows at once; once the
/// synced row reads the same it goes (web `session-composer-options.tsx`
/// `dropSettled`), so a later change of the row never fights a stale pick.
public struct ThreadComposerPicks: Equatable, Sendable {
    public var planMode: Bool?
    public var effort: String?
    public var ultracode: Bool?
    public var computerUse: Bool?
    /// The MCP pick is server-only (`sessions.mcpPicks`): what this composer
    /// last read or sent, never settled against the row.
    public var mcpServerIds: [String]?

    public init(
        planMode: Bool? = nil,
        effort: String? = nil,
        ultracode: Bool? = nil,
        computerUse: Bool? = nil,
        mcpServerIds: [String]? = nil
    ) {
        self.planMode = planMode
        self.effort = effort
        self.ultracode = ultracode
        self.computerUse = computerUse
        self.mcpServerIds = mcpServerIds
    }

    /// The synced row's four option fields, as the composer reads them
    /// (`effort` in picker form: blank/NULL = `LaunchVocabulary.cliDefault`).
    public struct Row: Equatable, Sendable {
        public var planMode: Bool
        public var effort: String
        public var ultracode: Bool
        public var computerUse: Bool

        public init(planMode: Bool?, effort: String?, ultracode: Bool?, computerUse: Bool?) {
            self.planMode = planMode ?? false
            self.effort = LaunchVocabulary.effortChoice(effort)
            self.ultracode = ultracode ?? false
            self.computerUse = computerUse ?? false
        }
    }

    public mutating func apply(_ input: SessionConfigureInput) {
        if let value = input.planMode { planMode = value }
        if let value = input.effort { effort = value }
        if let value = input.ultracode { ultracode = value }
        if let value = input.computerUse { computerUse = value }
        if let value = input.mcpServerIds { mcpServerIds = value }
    }

    /// Drops every pick the row now reads (a pick the row still disagrees
    /// with stays: the sync has not landed, or a later pick replaced it).
    public mutating func settle(_ row: Row) {
        if planMode == row.planMode { planMode = nil }
        if let effort, LaunchVocabulary.effortChoice(effort) == row.effort { self.effort = nil }
        if ultracode == row.ultracode { ultracode = nil }
        if computerUse == row.computerUse { computerUse = nil }
    }
}
