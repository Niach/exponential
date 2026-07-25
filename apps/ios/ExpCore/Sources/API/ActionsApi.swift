import Foundation

// Mirrors apps/web/src/lib/trpc/actions.ts (EXP-253). Team action prompts are
// tRPC-only — NOT an Electric shape: clients fetch on demand, and only the
// desktop ever executes a body (behind its per-device trust prompt). Mobile is
// view + run only: it lists actions and remote-starts them on a desktop via
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
        self.body = body
        self.sortOrder = sortOrder
        self.createdAt = createdAt
        self.updatedAt = updatedAt
        self.inputs = inputs
        self.builtin = builtin
    }

    /// The server-appended virtual builtin row (EXP-257).
    public var isBuiltin: Bool { builtin == true }
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
