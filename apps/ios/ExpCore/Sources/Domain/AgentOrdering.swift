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

    /// #EXP-1714 (`assistant.teamAgentsCases`): the agents a team lists: its
    /// own rows (input order), plus the viewer's global Assistant (its team
    /// is none of `teamIds`, the synced teams: a personal team never syncs)
    /// first, while the team holds no Assistant of theirs. `visibleAgent`
    /// still applies.
    public static func agentsInTeam<T>(
        _ agents: [T], teamId: String, teamIds: [String],
        teamOf: (T) -> String, isMyAssistant: (T) -> Bool
    ) -> [T] {
        let own = agents.filter { teamOf($0) == teamId }
        if own.contains(where: isMyAssistant) { return own }
        let synced = Set(teamIds)
        guard let global = agents.first(where: { isMyAssistant($0) && !synced.contains(teamOf($0)) })
        else { return own }
        return [global] + own
    }

    public static func agentsInTeam<T: AgentTeamScoped>(
        _ agents: [T], teamId: String, teamIds: [String], me: String?
    ) -> [T] {
        agentsInTeam(
            agents, teamId: teamId, teamIds: teamIds,
            teamOf: \.agentTeamId,
            isMyAssistant: { me != nil && isAssistant(role: $0.agentRole) && $0.agentOwnerId == me }
        )
    }
}

/// #EXP-1714: an agent row with its team (`agentsInTeam`).
public protocol AgentTeamScoped: AgentRoleOrdered {
    var agentTeamId: String { get }
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
