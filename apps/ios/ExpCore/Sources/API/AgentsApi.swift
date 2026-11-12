import Foundation

// Mirrors apps/web/src/lib/trpc/agents.ts (NEW-2). A team agent is a
// DEFINITION row (`agents`: name, icon, color, server-only instructions,
// triggers, its runner = device + cli + account + model), never a session.
// It runs as conversation sessions (one per member, `sessions.agent_id`);
// messaging one is `sessions.send({agentId, text})`. Member list/get, owner
// create/update/delete. The synced `agents` shape carries everything but the
// instructions (and the server's trigger bookkeeping).

/// `agents.create` (owner-only). `triggers` = the whole array, sent without
/// ids (the server mints them).
public struct AgentCreateInput: Encodable, Sendable, Equatable {
    public var teamId: String
    public var name: String
    public var icon: String?
    public var color: String?
    public var instructions: String
    public var triggers: [AgentTriggerInput]
    public var deviceId: String
    /// Contract `codingAgent` (claude/codex); nil = the device's default.
    public var cli: String?
    /// The login profile on the device; nil = its last used one.
    public var account: String?
    public var model: String?

    public init(
        teamId: String,
        name: String,
        icon: String? = nil,
        color: String? = nil,
        instructions: String = "",
        triggers: [AgentTriggerInput] = [],
        deviceId: String,
        cli: String? = nil,
        account: String? = nil,
        model: String? = nil
    ) {
        self.teamId = teamId
        self.name = name
        self.icon = icon
        self.color = color
        self.instructions = instructions
        self.triggers = triggers
        self.deviceId = deviceId
        self.cli = cli
        self.account = account
        self.model = model
    }

    enum CodingKeys: String, CodingKey {
        case teamId, name, icon, color, instructions, triggers, deviceId, cli, account, model
    }

    // Every nil is OMITTED (`.optional()` on the router).
    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(teamId, forKey: .teamId)
        try c.encode(name, forKey: .name)
        try c.encodeIfPresent(icon, forKey: .icon)
        try c.encodeIfPresent(color, forKey: .color)
        try c.encode(instructions, forKey: .instructions)
        try c.encode(triggers, forKey: .triggers)
        try c.encode(deviceId, forKey: .deviceId)
        try c.encodeIfPresent(cli, forKey: .cli)
        try c.encodeIfPresent(account, forKey: .account)
        try c.encodeIfPresent(model, forKey: .model)
    }
}

/// A partial `agents.update` (owner-gated). An OMITTED field (nil) keeps the
/// row's value; `account` / `model` are clearable, so `.some(nil)` sends an
/// explicit null ("the device's last used login" / "the CLI default").
/// `triggers` is the WHOLE array or nothing.
public struct AgentPatch: Sendable, Equatable {
    public var name: String?
    public var icon: String?
    public var color: String?
    public var instructions: String?
    public var triggers: [AgentTriggerInput]?
    public var deviceId: String?
    public var cli: String?
    public var account: String??
    public var model: String??
    public var sortOrder: Double?
    /// #EXP-1661: the WHOLE skill list (an empty one = none) or nothing.
    public var skillIds: [String]?
    /// #EXP-1793: the WHOLE MCP server list (an empty one = none) or nothing.
    public var mcpServerIds: [String]?

    public init(
        name: String? = nil,
        icon: String? = nil,
        color: String? = nil,
        instructions: String? = nil,
        triggers: [AgentTriggerInput]? = nil,
        deviceId: String? = nil,
        cli: String? = nil,
        account: String?? = nil,
        model: String?? = nil,
        sortOrder: Double? = nil,
        skillIds: [String]? = nil,
        mcpServerIds: [String]? = nil
    ) {
        self.name = name
        self.icon = icon
        self.color = color
        self.instructions = instructions
        self.triggers = triggers
        self.deviceId = deviceId
        self.cli = cli
        self.account = account
        self.model = model
        self.sortOrder = sortOrder
        self.skillIds = skillIds
        self.mcpServerIds = mcpServerIds
    }

    /// Nothing changed — no id-only mutation.
    public var isEmpty: Bool {
        name == nil && icon == nil && color == nil && instructions == nil && triggers == nil
            && deviceId == nil && cli == nil && account == nil && model == nil && sortOrder == nil
            && skillIds == nil && mcpServerIds == nil
    }
}

/// Internal (not private) so the wire test can pin omit-vs-null per field.
struct UpdateAgentInput: Encodable {
    let id: String
    let patch: AgentPatch

    enum CodingKeys: String, CodingKey {
        case id, name, icon, color, instructions, triggers, deviceId, cli, account, model, sortOrder, skillIds, mcpServerIds
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encodeIfPresent(patch.name, forKey: .name)
        try c.encodeIfPresent(patch.icon, forKey: .icon)
        try c.encodeIfPresent(patch.color, forKey: .color)
        try c.encodeIfPresent(patch.instructions, forKey: .instructions)
        // The whole array, an EMPTY one included ("no triggers left").
        try c.encodeIfPresent(patch.triggers, forKey: .triggers)
        try c.encodeIfPresent(patch.deviceId, forKey: .deviceId)
        try c.encodeIfPresent(patch.cli, forKey: .cli)
        // The unwrapped inner optional is encoded even when nil — an explicit
        // null is what clears the field server-side.
        if let account = patch.account {
            try c.encode(account, forKey: .account)
        }
        if let model = patch.model {
            try c.encode(model, forKey: .model)
        }
        try c.encodeIfPresent(patch.sortOrder, forKey: .sortOrder)
        try c.encodeIfPresent(patch.skillIds, forKey: .skillIds)
        try c.encodeIfPresent(patch.mcpServerIds, forKey: .mcpServerIds)
    }
}

/// `agents.get` / `create` / `update`: the definition row, as far as a
/// client reads it off tRPC (the rest syncs over the `agents` shape).
public struct AgentDetails: Decodable, Sendable, Equatable {
    public let id: String
    /// Server-only markdown (never on the shape).
    public let instructions: String
    /// S-3: contract `agentRole`; absent (an older server) = `agent`.
    public let role: String

    public init(id: String, instructions: String, role: String = DomainContract.agentRoleAgent) {
        self.id = id
        self.instructions = instructions
        self.role = role
    }

    public var isAssistant: Bool { role == DomainContract.agentRoleAssistant }

    enum CodingKeys: String, CodingKey {
        case id, instructions, role
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        instructions = try c.decodeIfPresent(String.self, forKey: .instructions) ?? ""
        let role = try c.decodeIfPresent(String.self, forKey: .role) ?? ""
        self.role = role.isEmpty ? DomainContract.agentRoleAgent : role
    }
}

private struct AgentResult: Decodable {
    let agent: AgentDetails
}

/// S-3 `agents.ensureAssistant`: the caller's Assistant, or null while they
/// have no device to run it on.
struct EnsureAssistantResult: Decodable {
    let agent: AgentDetails?
}

private struct EnsureAssistantInput: Encodable {
    let teamId: String
}

private struct AgentIdInput: Encodable {
    let id: String
}

/// #EXP-1717 `agents.setCoordinator`: the team's ONE coordinator; a nil
/// `agentId` (an explicit JSON null) clears it. Owner-only server-side.
struct SetCoordinatorInput: Encodable {
    let teamId: String
    let agentId: String?

    enum CodingKeys: String, CodingKey {
        case teamId, agentId
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(teamId, forKey: .teamId)
        try c.encode(agentId, forKey: .agentId)
    }
}

/// #EXP-1735 `agents.lead`: the team's lead Assistant (another member's
/// Assistant, never in the viewer's synced agents), or null.
public struct TeamLead: Decodable, Equatable, Sendable {
    public let agentId: String
    public let userId: String
}

struct TeamLeadResult: Decodable {
    let lead: TeamLead?
}

/// #EXP-1735 `agents.setLeadAssistant`: the member whose Assistant leads
/// the team; a nil `userId` (an explicit JSON null) clears it. Owner-only
/// server-side.
struct SetLeadAssistantInput: Encodable {
    let teamId: String
    let userId: String?

    enum CodingKeys: String, CodingKey {
        case teamId, userId
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(teamId, forKey: .teamId)
        try c.encode(userId, forKey: .userId)
    }
}

public final class AgentsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// The agent's server-only instructions, for its settings page.
    public func get(accountId: String, id: String) async throws -> AgentDetails {
        let result: AgentResult = try await trpc.query(
            accountId: accountId, path: "agents.get", input: AgentIdInput(id: id)
        )
        return result.agent
    }

    /// S-3: the caller's Assistant in `teamId`, created on first use (on
    /// their newest own desktop); nil = they have no device yet
    /// (`AgentSettings.AssistantCopy.noDevice`). Call it once per team per
    /// app run when the composer home, the chats list or the Settings
    /// Agents group mounts; the row itself arrives over the agents shape.
    @discardableResult
    public func ensureAssistant(accountId: String, teamId: String) async throws -> AgentDetails? {
        let result: EnsureAssistantResult = try await trpc.mutation(
            accountId: accountId, path: "agents.ensureAssistant", input: EnsureAssistantInput(teamId: teamId)
        )
        return result.agent
    }

    /// Owner-gated create; answers the new definition (its id).
    @discardableResult
    public func create(accountId: String, _ input: AgentCreateInput) async throws -> AgentDetails {
        let result: AgentResult = try await trpc.mutation(
            accountId: accountId, path: "agents.create", input: input
        )
        return result.agent
    }

    public func update(accountId: String, id: String, patch: AgentPatch) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "agents.update", input: UpdateAgentInput(id: id, patch: patch)
        )
    }

    /// #EXP-1717 (agent-settings.json `coordinator`): set the team's
    /// coordinator (nil = none); the previous one goes back to `agent` in the
    /// same write. The row change arrives over the agents shape.
    public func setCoordinator(accountId: String, teamId: String, agentId: String?) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "agents.setCoordinator",
            input: SetCoordinatorInput(teamId: teamId, agentId: agentId)
        )
    }

    /// #EXP-1735 (agent-settings.json `lead`): the team's lead Assistant
    /// (its agent and member), or nil = none.
    public func lead(accountId: String, teamId: String) async throws -> TeamLead? {
        let result: TeamLeadResult = try await trpc.query(
            accountId: accountId, path: "agents.lead", input: TeamIdInput(teamId: teamId)
        )
        return result.lead
    }

    /// #EXP-1735: make `userId`'s Assistant the team's lead (nil = none); a
    /// lead pick sends the team's coordinator back to `agent` in the same
    /// write.
    public func setLeadAssistant(accountId: String, teamId: String, userId: String?) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "agents.setLeadAssistant",
            input: SetLeadAssistantInput(teamId: teamId, userId: userId)
        )
    }

    /// Deletes the definition and, by cascade, every conversation with it.
    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "agents.delete", input: AgentIdInput(id: id)
        )
    }
}
