import Foundation

// Mirrors apps/web/src/lib/trpc/actions.ts (EXP-253). Since EXP-268 the team
// action prompts SYNC as the 15th Electric shape (minus `body` — the ≤64KB
// markdown prompt never rides sync; tRPC `actions.get` stays the only body
// path, and only the desktop ever executes a body behind its per-device trust
// prompt). Mobile lists actions from the local synced store and remote-starts
// them on a desktop via `steer.startSession({actionId})`; since EXP-694 it also
// EDITS them (EditActionSheet), which is what `get` (body) and `update` are for.

/// One typed action input (EXP-257): filled in the run dialog and injected
/// into the prompt by the desktop. `type` is a contract value
/// (`DomainContract.actionInputTypeValues` — text / repo / board / pr / icon);
/// tolerate unknown future types by gating the run, never by silently degrading.
/// `required` absent = optional (the wire omits the default-false flag).
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

/// One team action (`actions.list` row). `repositoryId` is nil for repo-less
/// actions (the desktop runs those in a scratch dir); `description` is the
/// optional one-liner under the name. `builtin == true` marks a virtual
/// builtin row (EXP-257/EXP-539 — constructed locally by every client, pinned
/// FIRST by this flag and never by sort order; non-editable, `body` empty).
/// `inputs` is the typed inputs schema — nil when a synced row carries no
/// parseable inputs JSON.
public struct ActionDto: Decodable, Identifiable, Sendable {
    public let id: String
    public let teamId: String
    public let repositoryId: String?
    public let name: String
    public let description: String?
    /// EXP-273: curated registry icon name; nil = the generic action glyph.
    public let icon: String?
    public let body: String
    public let sortOrder: Double
    public let createdAt: String
    public let updatedAt: String
    public let inputs: [ActionInputDto]?
    public let builtin: Bool?

    enum CodingKeys: String, CodingKey {
        case id, teamId, repositoryId, name, description, icon, body
        case sortOrder, createdAt, updatedAt, inputs, builtin
    }

    public init(
        id: String,
        teamId: String,
        repositoryId: String?,
        name: String,
        description: String?,
        icon: String? = nil,
        body: String,
        sortOrder: Double,
        createdAt: String,
        updatedAt: String,
        inputs: [ActionInputDto]? = nil,
        builtin: Bool? = nil
    ) {
        self.id = id
        self.teamId = teamId
        self.repositoryId = repositoryId
        self.name = name
        self.description = description
        self.icon = icon
        self.body = body
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.inputs = inputs
        self.builtin = builtin
    }

    /// The virtual builtin row (EXP-257).
    public var isBuiltin: Bool { builtin == true }
}

public extension ActionDto {
    /// The virtual "Create action" builtin (EXP-257). The server used to
    /// append it in `actions.list`; with actions synced via Electric
    /// (EXP-268, the 15th shape) clients PREPEND it locally instead — synced
    /// rows can't carry a virtual entry. Mirrors
    /// apps/web/src/lib/builtin-actions.ts field-for-field (the huge
    /// sortOrder only keeps naive sortOrder-asc renderers from interleaving
    /// it; pinning goes by the `builtin` flag).
    static func builtinCreateAction(teamId: String) -> ActionDto {
        ActionDto(
            id: DomainContract.builtinCreateActionId,
            teamId: teamId,
            repositoryId: nil,
            name: "Create action",
            description: "Describe a new action and let your agent author it for the team",
            icon: "sparkles",
            body: "",
            sortOrder: 1e9,
            createdAt: "1970-01-01T00:00:00.000Z",
            updatedAt: "1970-01-01T00:00:00.000Z",
            inputs: [
                ActionInputDto(
                    key: "description",
                    label: "Description",
                    type: "text",
                    required: true,
                    placeholder: "What should this action do?"
                ),
                // EXP-615: an optional name — blank lets the agent pick one.
                ActionInputDto(
                    key: "name",
                    label: "Name",
                    type: "text",
                    required: false,
                    placeholder: "Name (optional)"
                ),
                ActionInputDto(key: "repo", label: "Repository", type: "repo", required: false),
                // EXP-273: the author picks the new action's glyph up front.
                ActionInputDto(key: "icon", label: "Icon", type: "icon", required: false),
            ],
            builtin: true
        )
    }

    /// The virtual "Fix merge conflicts" builtin (EXP-259/EXP-270): pick a
    /// conflicted open PR and let the desktop rebase, resolve, push and merge
    /// it. Its `pr` input value is the REPRESENTATIVE issue id of an
    /// issue-linked open pull request (a batch PR links several issues to one
    /// PR — the picker dedupes by prUrl). Mirrors
    /// apps/web/src/lib/builtin-actions.ts field-for-field.
    static func builtinFixConflictsAction(teamId: String) -> ActionDto {
        ActionDto(
            id: DomainContract.builtinFixConflictsId,
            teamId: teamId,
            repositoryId: nil,
            name: "Fix merge conflicts",
            description: "Pick a conflicted pull request and let your agent rebase, resolve, and merge it",
            icon: "git-branch",
            body: "",
            sortOrder: 1e9 + 1,
            createdAt: "1970-01-01T00:00:00.000Z",
            updatedAt: "1970-01-01T00:00:00.000Z",
            inputs: [
                ActionInputDto(key: "pr", label: "Pull request", type: "pr", required: true),
            ],
            builtin: true
        )
    }

    /// The HIDDEN "Chat" builtin (EXP-615): a free-prompt agent session on a
    /// repository's trunk clone at its default branch — the iOS twin of the
    /// desktop's chat tab. Unlike the other two it is appended to NO list and
    /// belongs in NO picker: the Start-coding sheet's Chat tab constructs it
    /// directly for its submit. Mirrors apps/web/src/lib/builtin-actions.ts
    /// field-for-field.
    static func builtinChatAction(teamId: String) -> ActionDto {
        ActionDto(
            id: DomainContract.builtinChatId,
            teamId: teamId,
            repositoryId: nil,
            name: "Chat",
            description: "Chat with your agent on a repository",
            icon: "message-circle",
            body: "",
            sortOrder: 1e9 + 2,
            createdAt: "1970-01-01T00:00:00.000Z",
            updatedAt: "1970-01-01T00:00:00.000Z",
            inputs: [
                ActionInputDto(
                    key: "prompt",
                    label: "Prompt",
                    type: "textarea",
                    required: true,
                    placeholder: "What should the agent do?"
                ),
                ActionInputDto(key: "repo", label: "Repository", type: "repo", required: true),
            ],
            builtin: true
        )
    }

    /// Both LISTED builtins, in the order every client pins them (EXP-270 — mobile
    /// used to construct only "Create action", so "Fix merge conflicts"
    /// silently disappeared from iOS when EXP-268 moved the list onto the
    /// synced shape).
    static func builtinActions(teamId: String) -> [ActionDto] {
        [builtinCreateAction(teamId: teamId), builtinFixConflictsAction(teamId: teamId)]
    }

    /// Build a list-surface DTO from the synced local row (EXP-268). `body`
    /// is deliberately empty — the actions shape excludes it (tRPC
    /// `actions.get` stays the only body path) and nothing on the mobile
    /// view+run surfaces needs it. `inputs` parses the stored JSON string
    /// (absent/unparseable → nil).
    init(entity: ActionEntity) {
        let parsedInputs = entity.inputs
            .flatMap { $0.data(using: .utf8) }
            .flatMap { try? JSONDecoder().decode([ActionInputDto].self, from: $0) }
        self.init(
            id: entity.id,
            teamId: entity.teamId,
            repositoryId: entity.repositoryId,
            name: entity.name,
            description: entity.description,
            icon: entity.icon,
            body: "",
            sortOrder: entity.sortOrder ?? 0,
            createdAt: entity.createdAt,
            updatedAt: entity.updatedAt,
            inputs: parsedInputs,
            builtin: false
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

/// A partial `actions.update` payload (EXP-694 — mobile edits actions now).
/// Mirrors the router's optional inputs with the AutomationsApi omit-vs-null
/// rule, per field: an OMITTED field (nil) keeps what the row has, while the
/// two CLEARABLE ones are nested optionals, so `.some(nil)` sends an explicit
/// null — "no icon" / "no repository".
public struct ActionPatch: Sendable, Equatable {
    public var name: String?
    public var description: String??
    public var icon: String??
    public var repositoryId: String??
    public var body: String?

    public init(
        name: String? = nil,
        description: String?? = nil,
        icon: String?? = nil,
        repositoryId: String?? = nil,
        body: String? = nil
    ) {
        self.name = name
        self.description = description
        self.icon = icon
        self.repositoryId = repositoryId
        self.body = body
    }

    /// Nothing changed — the editor keeps its Save disabled rather than
    /// sending an id-only mutation.
    public var isEmpty: Bool {
        name == nil && description == nil && icon == nil
            && repositoryId == nil && body == nil
    }
}

private struct ListInput: Encodable {
    let teamId: String
}

private struct IdInput: Encodable {
    let id: String
}

private struct UpdateInput: Encodable {
    let id: String
    let patch: ActionPatch

    enum CodingKeys: String, CodingKey {
        case id, name, description, icon, repositoryId, body
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encodeIfPresent(patch.name, forKey: .name)
        try c.encodeIfPresent(patch.body, forKey: .body)
        // The unwrapped inner optional is encoded even when nil — an explicit
        // null is what clears the field server-side.
        if let description = patch.description {
            try c.encode(description, forKey: .description)
        }
        if let icon = patch.icon {
            try c.encode(icon, forKey: .icon)
        }
        if let repositoryId = patch.repositoryId {
            try c.encode(repositoryId, forKey: .repositoryId)
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
    /// Refuses the virtual builtins server-side: they have no row.
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
            input: UpdateInput(id: id, patch: patch)
        )
        return result.action
    }
}
