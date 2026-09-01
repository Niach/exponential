import Foundation

// Mirrors apps/web/src/lib/trpc/automations.ts (EXP-583). An automation binds
// ONE action to ONE device with a schedule/event trigger and its own
// agent/model/effort (NULL = the device's launch defaults). Rows SYNC as the
// 19th Electric shape — this router is the write path (team-owner-only) plus a
// member-gated `list` kept for parity with the other clients; mobile reads the
// synced store and writes through here.

/// One `automations` row. `trigger` is the raw jsonb as a JSON string —
/// tolerant-parsed lazily via `AutomationTrigger.parse`, so a future trigger
/// kind reads as "no trigger" instead of dropping the row.
public struct AutomationDto: Identifiable, Sendable, Equatable {
    public let id: String
    public let teamId: String
    public let actionId: String
    /// The steer device id (`devices.device_id`) that runs it locally.
    public let deviceId: String
    public let enabled: Bool
    public let trigger: String?
    /// nil = the device's launch defaults (all three travel together).
    public let agent: String?
    public let model: String?
    public let effort: String?
    public let sortOrder: Double
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        actionId: String,
        deviceId: String,
        enabled: Bool,
        trigger: String?,
        agent: String? = nil,
        model: String? = nil,
        effort: String? = nil,
        sortOrder: Double = 0,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.actionId = actionId
        self.deviceId = deviceId
        self.enabled = enabled
        self.trigger = trigger
        self.agent = agent
        self.model = model
        self.effort = effort
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    /// The parsed trigger, nil when absent/malformed.
    public var parsedTrigger: AutomationTrigger? { AutomationTrigger.parse(trigger) }
}

extension AutomationDto: Decodable {
    enum CodingKeys: String, CodingKey {
        case id, enabled, trigger, agent, model, effort
        case teamId, actionId, deviceId, sortOrder, createdAt, updatedAt
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        teamId = try c.decode(String.self, forKey: .teamId)
        actionId = try c.decode(String.self, forKey: .actionId)
        deviceId = try c.decode(String.self, forKey: .deviceId)
        enabled = c.decodeWireBool(forKey: .enabled, default: true)
        // jsonb: an object over tRPC, a pre-stringified value from fixtures.
        trigger = c.decodeWireJsonString(forKey: .trigger)
        agent = try c.decodeIfPresent(String.self, forKey: .agent)
        model = try c.decodeIfPresent(String.self, forKey: .model)
        effort = try c.decodeIfPresent(String.self, forKey: .effort)
        sortOrder = (try c.decodeWireDouble(forKey: .sortOrder)) ?? 0
        createdAt = try c.decode(String.self, forKey: .createdAt)
        updatedAt = try c.decode(String.self, forKey: .updatedAt)
    }
}

public extension AutomationDto {
    /// The synced local row as the DTO every surface renders.
    init(entity: AutomationEntity) {
        self.init(
            id: entity.id,
            teamId: entity.teamId,
            actionId: entity.actionId,
            deviceId: entity.deviceId,
            enabled: entity.enabled,
            trigger: entity.trigger,
            agent: entity.agent,
            model: entity.model,
            effort: entity.effort,
            sortOrder: entity.sortOrder ?? 0,
            createdAt: entity.createdAt,
            updatedAt: entity.updatedAt
        )
    }
}

/// Server envelope: `automations.list` returns `{ automations: [<row>] }`.
public struct AutomationsListResult: Decodable, Sendable {
    public let automations: [AutomationDto]

    public init(automations: [AutomationDto]) {
        self.automations = automations
    }
}

/// `automations.create/update` return `{ automation, txId }`.
public struct AutomationResult: Decodable, Sendable {
    public let automation: AutomationDto
}

private struct ListInput: Encodable {
    let teamId: String
}

private struct CreateInput: Encodable {
    let teamId: String
    let actionId: String
    let deviceId: String
    let trigger: AutomationTrigger
    let enabled: Bool?
    let agent: String?
    let model: String?
    let effort: String?
}

/// The three launch fields move together and are TRI-STATE on the wire: an
/// absent key keeps what the row has, an explicit null resets that field to
/// the device's launch defaults. Pass the patch only when the form actually
/// edited them (the enable toggle never does).
public struct AutomationLaunchPatch: Sendable, Equatable {
    public let agent: String?
    public let model: String?
    public let effort: String?

    public init(agent: String?, model: String?, effort: String?) {
        self.agent = agent
        self.model = model
        self.effort = effort
    }
}

// Every field but `id` is OMITTED when nil — the server reads an absent key as
// "keep", so the enable toggle really sends only { id, enabled }.
private struct UpdateInput: Encodable {
    let id: String
    var actionId: String?
    var deviceId: String?
    var trigger: AutomationTrigger?
    var enabled: Bool?
    var launch: AutomationLaunchPatch?

    enum CodingKeys: String, CodingKey {
        case id, actionId, deviceId, trigger, enabled, agent, model, effort
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encodeIfPresent(actionId, forKey: .actionId)
        try c.encodeIfPresent(deviceId, forKey: .deviceId)
        try c.encodeIfPresent(trigger, forKey: .trigger)
        try c.encodeIfPresent(enabled, forKey: .enabled)
        // Explicit nulls: "back to the device's launch defaults".
        if let launch {
            try c.encode(launch.agent, forKey: .agent)
            try c.encode(launch.model, forKey: .model)
            try c.encode(launch.effort, forKey: .effort)
        }
    }
}

private struct IdInput: Encodable {
    let id: String
}

public final class AutomationsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Member-gated `automations.list` — sortOrder-then-createdAt ordered
    /// server-side. The synced shape is the read path on mobile; this stays
    /// for one-shot reconciliation.
    public func list(accountId: String, teamId: String) async throws -> [AutomationDto] {
        let result: AutomationsListResult = try await trpc.query(
            accountId: accountId,
            path: "automations.list",
            input: ListInput(teamId: teamId)
        )
        return result.automations
    }

    /// Owner-gated `automations.create`. The server refuses a builtin or
    /// foreign action, an action with required inputs while enabled, a device
    /// that is not yours/team-shared or lacks the `automations` cap, and an
    /// agent the device doesn't advertise.
    @discardableResult
    public func create(
        accountId: String,
        teamId: String,
        actionId: String,
        deviceId: String,
        trigger: AutomationTrigger,
        enabled: Bool? = nil,
        agent: String? = nil,
        model: String? = nil,
        effort: String? = nil
    ) async throws -> AutomationDto {
        let result: AutomationResult = try await trpc.mutation(
            accountId: accountId,
            path: "automations.create",
            input: CreateInput(
                teamId: teamId,
                actionId: actionId,
                deviceId: deviceId,
                trigger: trigger,
                enabled: enabled,
                agent: agent,
                model: model,
                effort: effort
            )
        )
        return result.automation
    }

    /// Owner-gated `automations.update` — a partial patch. The enable toggle
    /// is `update(id:enabled:)`; the synced row echoes the change back, so
    /// success needs no local write.
    @discardableResult
    public func update(
        accountId: String,
        id: String,
        actionId: String? = nil,
        deviceId: String? = nil,
        trigger: AutomationTrigger? = nil,
        enabled: Bool? = nil,
        launch: AutomationLaunchPatch? = nil
    ) async throws -> AutomationDto {
        let result: AutomationResult = try await trpc.mutation(
            accountId: accountId,
            path: "automations.update",
            input: UpdateInput(
                id: id,
                actionId: actionId,
                deviceId: deviceId,
                trigger: trigger,
                enabled: enabled,
                launch: launch
            )
        )
        return result.automation
    }

    /// Owner-gated `automations.delete`.
    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "automations.delete",
            input: IdInput(id: id)
        )
    }
}
