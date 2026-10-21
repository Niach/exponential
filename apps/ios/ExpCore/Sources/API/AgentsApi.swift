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
        sortOrder: Double? = nil
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
    }

    /// Nothing changed — no id-only mutation.
    public var isEmpty: Bool {
        name == nil && icon == nil && color == nil && instructions == nil && triggers == nil
            && deviceId == nil && cli == nil && account == nil && model == nil && sortOrder == nil
    }
}

/// Internal (not private) so the wire test can pin omit-vs-null per field.
struct UpdateAgentInput: Encodable {
    let id: String
    let patch: AgentPatch

    enum CodingKeys: String, CodingKey {
        case id, name, icon, color, instructions, triggers, deviceId, cli, account, model, sortOrder
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
    }
}

/// `agents.get` / `create` / `update`: the definition row, as far as a
/// client reads it off tRPC (the rest syncs over the `agents` shape).
public struct AgentDetails: Decodable, Sendable, Equatable {
    public let id: String
    /// Server-only markdown (never on the shape).
    public let instructions: String

    public init(id: String, instructions: String) {
        self.id = id
        self.instructions = instructions
    }

    enum CodingKeys: String, CodingKey {
        case id, instructions
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        instructions = try c.decodeIfPresent(String.self, forKey: .instructions) ?? ""
    }
}

private struct AgentResult: Decodable {
    let agent: AgentDetails
}

private struct AgentIdInput: Encodable {
    let id: String
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

    /// Deletes the definition and, by cascade, every conversation with it.
    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "agents.delete", input: AgentIdInput(id: id)
        )
    }
}
