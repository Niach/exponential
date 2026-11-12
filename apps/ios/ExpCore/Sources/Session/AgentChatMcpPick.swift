import Foundation

/// #EXP-1793 (composer-menu.json `mcpPick`, agent-settings.json `mcpRow`):
/// an agent chat's "+" MCP servers pick = its conversation's servers. The
/// seed is the conversation's own pick (`sessions.mcpPicks`) once it exists,
/// else the agent's `mcpServerIds`; a send carries `mcpServerIds` whenever
/// the pick differs from that seed (an empty pick clears), and after the
/// send the pick IS the seed.
public struct AgentChatMcpPick: Equatable, Sendable {
    /// The agent whose conversation the pick rides into (`sessions.send
    /// {agentId, mcpServerIds}`).
    public let agentId: String
    public private(set) var seed: [String]
    public var picked: [String]

    public init(agentId: String, seed: [String]) {
        self.agentId = agentId
        self.seed = seed
        self.picked = seed
    }

    /// What the next send carries: nil = the pick stands (not sent).
    public var outgoing: [String]? {
        Set(picked) == Set(seed) ? nil : picked
    }

    /// A new seed (the conversation's read-back, the agent's change): an
    /// untouched pick follows it, a pending one stays.
    public mutating func reseed(_ next: [String]) {
        if outgoing == nil { picked = next }
        seed = next
    }

    /// The send went out: its pick is the seed now.
    public mutating func sent() {
        seed = picked
    }
}
