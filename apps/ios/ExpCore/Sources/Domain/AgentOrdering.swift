import Foundation

/// S-3 / EXP-1635: the agent list rules (agent-settings.json `assistant`) on
/// their own, Foundation-only, so the share extension compiles them without
/// the rest of `AgentSettings`.
public enum AgentOrdering {
    public static func isAssistant(role: String?) -> Bool {
        role == DomainContract.agentRoleAssistant
    }

    /// `assistant.visibleCases`: another member's Assistant is never listed
    /// (settings, composer, chats).
    public static func visibleAgent(role: String?, ownerId: String?, me: String?) -> Bool {
        !isAssistant(role: role) || ownerId == me
    }

    public static func visibleAgent(_ agent: some AgentRoleOrdered, me: String?) -> Bool {
        visibleAgent(role: agent.agentRole, ownerId: agent.agentOwnerId, me: me)
    }

    /// `assistant.orderCases`: the agents a member sees, in order: their
    /// Assistant first, then the team agents by sortOrder, then name.
    public static func orderedAgents<T: AgentRoleOrdered>(_ agents: [T], me: String?) -> [T] {
        agents
            .filter { visibleAgent($0, me: me) }
            .enumerated()
            .sorted { lhs, rhs in
                let a = lhs.element, b = rhs.element
                let aAssistant = isAssistant(role: a.agentRole), bAssistant = isAssistant(role: b.agentRole)
                if aAssistant != bAssistant { return aAssistant }
                if a.agentSortOrder != b.agentSortOrder { return a.agentSortOrder < b.agentSortOrder }
                let byName = a.agentName.localizedCompare(b.agentName)
                if byName != .orderedSame { return byName == .orderedAscending }
                return lhs.offset < rhs.offset
            }
            .map(\.element)
    }
}

/// S-3: what `AgentOrdering.orderedAgents` / `visibleAgent` /
/// `ComposerAgentRow` read off an agent row.
public protocol AgentRoleOrdered {
    var agentId: String { get }
    var agentName: String { get }
    var agentRole: String? { get }
    var agentOwnerId: String? { get }
    var agentSortOrder: Double { get }
}
