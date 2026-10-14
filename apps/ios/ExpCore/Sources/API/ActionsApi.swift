import Foundation

// Mirrors apps/web/src/lib/trpc/actions.ts. Actions are SAVED PROMPTS
// (EXP-1291: name, icon, body, inputs, prompt placeholder — no triggers,
// repository or MCP picks any more). They sync as an Electric shape minus
// `body` (the ≤64KB markdown prompt never rides sync; tRPC `actions.get` stays
// the only body path). Running one is `sessions.send({actionId, inputs})`;
// mobile also EDITS them (EXP-694), which is what `get` (body) and `update`
// are for.

/// One typed action input (EXP-257): a PICK filled in the composer and
/// composed into the first prompt server-side. `type` is a contract value
/// (`DomainContract.actionInputTypeValues` — repo / board / pr / issue;
/// whatever the requester types is the message text); tolerate unknown future
/// types by gating the run, never by silently degrading. `required` absent =
/// optional (the wire omits the default-false flag).
public struct ActionInputDto: Decodable, Sendable, Equatable {
    public let key: String
    public let label: String
    public let type: String
    public let required: Bool?
    public let placeholder: String?

    public init(
        key: String,
        label: String,
        type: String,
        required: Bool? = nil,
        placeholder: String? = nil
    ) {
        self.key = key
        self.label = label
        self.type = type
        self.required = required
        self.placeholder = placeholder
    }

    public var isRequired: Bool { required == true }
}

/// One team action (`actions.list` row); `inputs` is the typed inputs
/// schema — nil when a synced row carries no parseable inputs JSON.
public struct ActionDto: Identifiable, Sendable, Decodable {
    public let id: String
    public let teamId: String
    public let name: String
    /// EXP-273: curated registry icon name; nil = the generic action glyph.
    public let icon: String?
    public let body: String
    public let sortOrder: Double
    public let createdAt: String
    public let updatedAt: String
    public let inputs: [ActionInputDto]?
    /// EXP-825: the composer's field hint while this action is picked (≤200
    /// chars); nil = the generic "Additional instructions (optional)…".
    public let promptPlaceholder: String?

    public init(
        id: String,
        teamId: String,
        name: String,
        icon: String? = nil,
        body: String,
        sortOrder: Double,
        createdAt: String,
        updatedAt: String,
        inputs: [ActionInputDto]? = nil,
        promptPlaceholder: String? = nil
    ) {
        self.id = id
        self.teamId = teamId
        self.name = name
        self.icon = icon
        self.body = body
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.inputs = inputs
        self.promptPlaceholder = promptPlaceholder
    }

    /// The server's cap on `promptPlaceholder` (`MAX_ACTION_PROMPT_PLACEHOLDER`
    /// in db-schema/domain.ts) — the editor refuses at the field, not at
    /// submit.
    public static let promptPlaceholderMaxLength = 200

    /// `promptPlaceholder` cut to the server's cap. The cap is a JS string
    /// length, i.e. UTF-16 code units — never `String.count` (a grapheme can
    /// be many units, so a flag-heavy hint would pass here and be refused
    /// on the wire). A split surrogate pair is dropped rather than shipped.
    public static func clampPromptPlaceholder(_ value: String) -> String {
        let cap = promptPlaceholderMaxLength
        guard value.utf16.count > cap else { return value }
        var units = Array(value.utf16.prefix(cap))
        if let last = units.last, UTF16.isLeadSurrogate(last) {
            units.removeLast()
        }
        return String(decoding: units, as: UTF16.self)
    }
}

public extension ActionDto {
    /// Build a list-surface DTO from the synced local row (EXP-268). `body`
    /// is deliberately empty — the actions shape excludes it (tRPC
    /// `actions.get` stays the only body path) and nothing on the mobile
    /// list+run surfaces needs it. `inputs` parses the stored JSON string
    /// (absent/unparseable → nil).
    init(entity: ActionEntity) {
        let parsedInputs = entity.inputs
            .flatMap { $0.data(using: .utf8) }
            .flatMap { try? JSONDecoder().decode([ActionInputDto].self, from: $0) }
        self.init(
            id: entity.id,
            teamId: entity.teamId,
            name: entity.name,
            icon: entity.icon,
            body: "",
            sortOrder: entity.sortOrder ?? 0,
            createdAt: entity.createdAt,
            updatedAt: entity.updatedAt,
            inputs: parsedInputs,
            promptPlaceholder: entity.promptPlaceholder
        )
    }
}

/// Server envelope: `actions.list` returns `{ actions: [<row>] }`.
public struct ActionsListResult: Decodable, Sendable {
    public let actions: [ActionDto]

    public init(actions: [ActionDto]) {
        self.actions = actions
    }
}

/// Server envelope: `actions.get` / `actions.update` return `{ action }`.
public struct ActionResult: Decodable, Sendable {
    public let action: ActionDto

    public init(action: ActionDto) {
        self.action = action
    }
}

/// A partial `actions.update` payload (EXP-694 — mobile edits actions).
/// Mirrors the router's optional inputs, omit-vs-null per field: an OMITTED
/// field (nil) keeps what the row has, while the CLEARABLE ones are nested
/// optionals, so `.some(nil)` sends an explicit null — "no icon" / "no
/// composer hint" (EXP-825).
public struct ActionPatch: Sendable, Equatable {
    public var name: String?
    public var icon: String??
    public var body: String?
    public var promptPlaceholder: String??

    public init(
        name: String? = nil,
        icon: String?? = nil,
        body: String? = nil,
        promptPlaceholder: String?? = nil
    ) {
        self.name = name
        self.icon = icon
        self.body = body
        self.promptPlaceholder = promptPlaceholder
    }

    /// Nothing changed — the editor keeps its Save disabled rather than
    /// sending an id-only mutation.
    public var isEmpty: Bool {
        name == nil && icon == nil && body == nil && promptPlaceholder == nil
    }
}

private struct ListInput: Encodable {
    let teamId: String
}

private struct IdInput: Encodable {
    let id: String
}

/// Internal (not private) so the wire-format test can pin the omit-vs-null
/// encoding per field.
struct ActionUpdateInput: Encodable {
    let id: String
    let patch: ActionPatch

    enum CodingKeys: String, CodingKey {
        case id, name, icon, body, promptPlaceholder
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encodeIfPresent(patch.name, forKey: .name)
        try c.encodeIfPresent(patch.body, forKey: .body)
        // The unwrapped inner optional is encoded even when nil — an explicit
        // null is what clears the field server-side.
        if let icon = patch.icon {
            try c.encode(icon, forKey: .icon)
        }
        if let promptPlaceholder = patch.promptPlaceholder {
            try c.encode(promptPlaceholder, forKey: .promptPlaceholder)
        }
    }
}

public final class ActionsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Member-gated `actions.list` — the team's actions, sortOrder-then-name
    /// ordered server-side.
    public func list(accountId: String, teamId: String) async throws -> [ActionDto] {
        let result: ActionsListResult = try await trpc.query(
            accountId: accountId,
            path: "actions.list",
            input: ListInput(teamId: teamId)
        )
        return result.actions
    }

    /// Member-gated `actions.get` — the ONLY body path (the synced shape
    /// excludes the ≤64KB prompt), so the edit sheet fetches it on open.
    public func get(accountId: String, id: String) async throws -> ActionDto {
        let result: ActionResult = try await trpc.query(
            accountId: accountId,
            path: "actions.get",
            input: IdInput(id: id)
        )
        return result.action
    }

    /// Owner-gated `actions.update` — a partial patch (EXP-694). The synced
    /// row echoes the metadata back, so success needs no local write.
    @discardableResult
    public func update(
        accountId: String,
        id: String,
        patch: ActionPatch
    ) async throws -> ActionDto {
        let result: ActionResult = try await trpc.mutation(
            accountId: accountId,
            path: "actions.update",
            input: ActionUpdateInput(id: id, patch: patch)
        )
        return result.action
    }
}
