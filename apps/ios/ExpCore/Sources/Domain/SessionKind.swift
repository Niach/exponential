import Foundation

// EXP-1341/NEW-2: ONE session model ×4. A thread and an agent's conversation
// run the same way; a conversation session (a root with `agentId`) is just
// `persistent` (never ends, revived on its device's return). Branch on these
// traits, never on the kind string: `kindOf(agentId:)` derives the kind from
// `agentId`.
// A port of `packages/domain-contract/src/session-kind.ts`, locked by
// fixtures/session-kind.json (`SessionKindTests`).

public struct SessionTraits: Equatable, Sendable {
    /// Never ends: exits idle, `sessions.end` refuses, revived by its device.
    public let persistent: Bool
    /// The kind whose definition carries triggers (the server fires them).
    public let triggers: Bool

    public init(persistent: Bool, triggers: Bool) {
        self.persistent = persistent
        self.triggers = triggers
    }
}

public enum SessionKind {
    /// fixtures/session-kind.json `kinds`.
    public static let kinds: [String: SessionTraits] = [
        "thread": SessionTraits(persistent: false, triggers: false),
        "agent": SessionTraits(persistent: true, triggers: true),
    ]

    /// An unknown or missing kind = `thread`.
    public static func traits(_ kind: String?) -> SessionTraits {
        kind.flatMap { kinds[$0] } ?? kinds["thread"]!
    }

    /// NEW-2: the kind of a session row: `agent` when it is an agent's
    /// conversation session (`agentId` set), else `thread`.
    public static func kindOf(agentId: String?) -> String {
        (agentId?.isEmpty == false) ? DomainContract.sessionKindAgent : DomainContract.sessionKindThread
    }

    /// A persistent session (an agent's conversation): it idles instead of ending.
    public static func isPersistent(agentId: String?) -> Bool {
        traits(kindOf(agentId: agentId)).persistent
    }
}

/// NEW-2: `sessionKindOf` of session-kind.ts (the fixture's `kindOfCases`).
public func sessionKindOf(agentId: String?) -> String {
    SessionKind.kindOf(agentId: agentId)
}
