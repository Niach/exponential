import Foundation

// Mirrors apps/web/src/lib/trpc/actions.ts (EXP-253). Since EXP-268 the team
// action prompts SYNC as the 15th Electric shape (minus `body` — the ≤64KB
// markdown prompt never rides sync; tRPC `actions.get` stays the only body
// path, and only the desktop ever executes a body behind its per-device trust
// prompt). Mobile is view + run only: it lists actions from the local synced
// store and remote-starts them on a desktop via
// `steer.startSession({actionId})` — the body itself never matters here.

/// One typed action input (EXP-257): filled in the run dialog and injected
/// into the prompt by the desktop. `type` is a contract value
/// (`DomainContract.actionInputTypeValues` — text / repo / board); tolerate
/// unknown future types by gating the run, never by silently degrading.
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
/// optional one-liner under the name. `builtin == true` marks the server-
/// appended virtual "Create action" row (EXP-257 — pinned FIRST by this flag,
/// never by sort order; non-editable, `body` empty); `inputs` is the typed
/// inputs schema. Both are optional so older servers keep decoding.
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
            description: "Describe a new action and let Claude author it for the team",
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
            description: "Pick a conflicted pull request and let Claude rebase, resolve, and merge it",
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

    /// Both builtins, in the order every client pins them (EXP-270 — mobile
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

private struct ListInput: Encodable {
    let teamId: String
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
}
