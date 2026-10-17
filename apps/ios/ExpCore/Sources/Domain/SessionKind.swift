import Foundation

// EXP-1341: ONE session model ×4. A thread and an agent run the same way; an
// agent is just `persistent` (never ends, revived on its device's return) and
// carries `triggers`. Branch on these traits, never on the kind string.
// A port of `packages/domain-contract/src/session-kind.ts`, locked by
// fixtures/session-kind.json (`SessionKindTests`).

public struct SessionTraits: Equatable, Sendable {
    /// Never ends: exits idle, `sessions.end` refuses, revived by its device.
    public let persistent: Bool
    /// Carries triggers (schedules / events) its device fires as messages.
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

    /// A persistent session (an agent root): it idles instead of ending.
    public static func isPersistent(_ kind: String?) -> Bool {
        traits(kind).persistent
    }
}
