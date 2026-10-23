import Foundation

// Mirrors apps/web/src/lib/trpc/prompts.ts (NEW-2). Prompts REPLACE actions:
// a saved prompt = name, icon, markdown body (≤64KB) — no inputs, triggers,
// repository or composer hint. They sync as the `prompts` shape minus `body`
// (tRPC `prompts.get` is the only body path). Running one is
// `sessions.send({promptId, text, deviceId})`: a new root whose first
// message is the body plus what was typed. Member list/get, owner writes.
// NEW-8: the client no longer syncs the legacy `actions` shape.

/// One prompt row (`prompts.list` without a body; `get` / `create` /
/// `update` with one).
public struct PromptDto: Identifiable, Sendable, Decodable, Equatable {
    public let id: String
    public let teamId: String
    public let name: String
    /// Curated registry icon name; nil = the generic prompt glyph.
    public let icon: String?
    /// nil on a `list` row (the body never rides the list or the shape).
    public let body: String?
    public let sortOrder: Double
    public let createdAt: String
    public let updatedAt: String

    public init(
        id: String,
        teamId: String,
        name: String,
        icon: String? = nil,
        body: String? = nil,
        sortOrder: Double = 0,
        createdAt: String,
        updatedAt: String
    ) {
        self.id = id
        self.teamId = teamId
        self.name = name
        self.icon = icon
        self.body = body
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }
}

/// A partial `prompts.update` payload. An OMITTED field (nil) keeps what the
/// row has; `icon` is clearable, so `.some(nil)` sends an explicit null ("no
/// icon").
public struct PromptPatch: Sendable, Equatable {
    public var name: String?
    public var icon: String??
    public var body: String?
    public var sortOrder: Double?

    public init(
        name: String? = nil,
        icon: String?? = nil,
        body: String? = nil,
        sortOrder: Double? = nil
    ) {
        self.name = name
        self.icon = icon
        self.body = body
        self.sortOrder = sortOrder
    }

    /// Nothing changed — the editor keeps its Save disabled rather than
    /// sending an id-only mutation.
    public var isEmpty: Bool {
        name == nil && icon == nil && body == nil && sortOrder == nil
    }
}

/// `prompts.create` (owner-only): the router's strict object.
public struct PromptCreateInput: Encodable, Sendable, Equatable {
    public let teamId: String
    public let name: String
    public let icon: String?
    public let body: String

    public init(teamId: String, name: String, icon: String? = nil, body: String) {
        self.teamId = teamId
        self.name = name
        self.icon = icon
        self.body = body
    }

    enum CodingKeys: String, CodingKey {
        case teamId, name, icon, body
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(teamId, forKey: .teamId)
        try c.encode(name, forKey: .name)
        try c.encodeIfPresent(icon, forKey: .icon)
        try c.encode(body, forKey: .body)
    }
}

/// Internal (not private) so the wire-format test can pin the omit-vs-null
/// encoding per field.
struct PromptUpdateInput: Encodable {
    let id: String
    let patch: PromptPatch

    enum CodingKeys: String, CodingKey {
        case id, name, icon, body, sortOrder
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encodeIfPresent(patch.name, forKey: .name)
        try c.encodeIfPresent(patch.body, forKey: .body)
        // The unwrapped inner optional is encoded even when nil — an explicit
        // null is what clears the icon server-side.
        if let icon = patch.icon {
            try c.encode(icon, forKey: .icon)
        }
        try c.encodeIfPresent(patch.sortOrder, forKey: .sortOrder)
    }
}

private struct PromptsListResult: Decodable {
    let prompts: [PromptDto]
}

private struct PromptResult: Decodable {
    let prompt: PromptDto
}

private struct PromptsTeamInput: Encodable {
    let teamId: String
}

private struct IdInput: Encodable {
    let id: String
}

public final class PromptsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Member-gated `prompts.list` — sortOrder-then-name, without bodies.
    public func list(accountId: String, teamId: String) async throws -> [PromptDto] {
        let result: PromptsListResult = try await trpc.query(
            accountId: accountId, path: "prompts.list", input: PromptsTeamInput(teamId: teamId)
        )
        return result.prompts
    }

    /// Member-gated `prompts.get` — the ONLY body path.
    public func get(accountId: String, id: String) async throws -> PromptDto {
        let result: PromptResult = try await trpc.query(
            accountId: accountId, path: "prompts.get", input: IdInput(id: id)
        )
        return result.prompt
    }

    /// Owner-gated `prompts.create`; a taken name refuses with CONFLICT.
    @discardableResult
    public func create(accountId: String, _ input: PromptCreateInput) async throws -> PromptDto {
        let result: PromptResult = try await trpc.mutation(
            accountId: accountId, path: "prompts.create", input: input
        )
        return result.prompt
    }

    /// Owner-gated `prompts.update` — a partial patch. The synced row echoes
    /// the metadata back, so success needs no local write.
    @discardableResult
    public func update(accountId: String, id: String, patch: PromptPatch) async throws -> PromptDto {
        let result: PromptResult = try await trpc.mutation(
            accountId: accountId, path: "prompts.update", input: PromptUpdateInput(id: id, patch: patch)
        )
        return result.prompt
    }

    /// Owner-gated `prompts.delete`.
    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "prompts.delete", input: IdInput(id: id))
    }
}
