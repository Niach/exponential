import Foundation

// MARK: - Input/Output types

public struct CreateIssueInput: Encodable, Sendable {
    public let boardId: String
    public let title: String
    /// The builtin ANCHOR enum value. Mutually exclusive with `statusId`
    /// server-side — send one or the other, never both (EXP-314).
    public var status: String?
    /// A team `issue_statuses` row id (EXP-314). Status pickers send this;
    /// the enum-only conveniences (swipes, toggles) keep sending `status`.
    public var statusId: String?
    public var priority: String?
    public var assigneeId: String?
    public var description: String?
    public var dueDate: String?
    public var labelIds: [String]?

    public init(
        boardId: String,
        title: String,
        status: String? = nil,
        statusId: String? = nil,
        priority: String? = nil,
        assigneeId: String? = nil,
        description: String? = nil,
        dueDate: String? = nil,
        labelIds: [String]? = nil
    ) {
        self.boardId = boardId
        self.title = title
        self.status = status
        self.statusId = statusId
        self.priority = priority
        self.assigneeId = assigneeId
        self.description = description
        self.dueDate = dueDate
        self.labelIds = labelIds
    }
}

public struct UpdateIssueInput: Encodable, Sendable {
    public let id: String
    public var title: String?
    /// The builtin ANCHOR enum value. Mutually exclusive with `statusId`
    /// server-side, and with a non-null `duplicateOfId` (EXP-314).
    public var status: String?
    /// A team `issue_statuses` row id (EXP-314) — what status pickers send.
    public var statusId: String?
    public var priority: String?
    public var assigneeId: String?
    public var description: String?
    public var dueDate: String?
    /// Canonical issue this one duplicates — set together with
    /// `status = "duplicate"` in ONE update so the marking is atomic.
    public var duplicateOfId: String?

    // Fields listed here are encoded as JSON null (not omitted).
    // Use this when the server must distinguish "clear this field" from "don't touch it".
    public var explicitNulls: Set<String> = []

    public init(
        id: String,
        title: String? = nil,
        status: String? = nil,
        statusId: String? = nil,
        priority: String? = nil,
        assigneeId: String? = nil,
        description: String? = nil,
        dueDate: String? = nil,
        duplicateOfId: String? = nil,
        explicitNulls: Set<String> = []
    ) {
        self.id = id
        self.title = title
        self.status = status
        self.statusId = statusId
        self.priority = priority
        self.assigneeId = assigneeId
        self.description = description
        self.dueDate = dueDate
        self.duplicateOfId = duplicateOfId
        self.explicitNulls = explicitNulls
    }

    enum CodingKeys: String, CodingKey {
        case id, title, status, statusId, priority, assigneeId, description
        case dueDate
        case duplicateOfId
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encodeIfPresent(title, forKey: .title)
        try c.encodeIfPresent(status, forKey: .status)
        try c.encodeIfPresent(statusId, forKey: .statusId)
        try c.encodeIfPresent(priority, forKey: .priority)
        try encodeNullable(assigneeId, forKey: .assigneeId, in: &c)
        try encodeNullable(description, forKey: .description, in: &c)
        try encodeNullable(dueDate, forKey: .dueDate, in: &c)
        try encodeNullable(duplicateOfId, forKey: .duplicateOfId, in: &c)
    }

    private func encodeNullable<T: Encodable>(_ value: T?, forKey key: CodingKeys, in container: inout KeyedEncodingContainer<CodingKeys>) throws {
        if value != nil {
            try container.encode(value, forKey: key)
        } else if explicitNulls.contains(key.rawValue) {
            try container.encodeNil(forKey: key)
        }
    }
}

/// Input for `issues.bulkUpdate` — one transactional property write across a
/// whole selection (`ids` is capped at 200 server-side). Hand-encoded for the
/// same reason `UpdateIssueInput` is: `assigneeId` must be OMITTED to leave
/// the assignee alone but sent as JSON null to unassign, and the two cases are
/// indistinguishable to the synthesized encoder.
public struct BulkUpdateIssuesInput: Encodable, Sendable {
    public let ids: [String]
    /// The builtin ANCHOR enum value; mutually exclusive with `statusId`.
    public var status: String?
    /// A team `issue_statuses` row id (EXP-314) — what the bulk bar sends.
    public var statusId: String?
    public var priority: String?
    public var assigneeId: String?

    /// Fields listed here are encoded as JSON null (not omitted).
    public var explicitNulls: Set<String> = []

    public init(
        ids: [String],
        status: String? = nil,
        statusId: String? = nil,
        priority: String? = nil,
        assigneeId: String? = nil,
        explicitNulls: Set<String> = []
    ) {
        self.ids = ids
        self.status = status
        self.statusId = statusId
        self.priority = priority
        self.assigneeId = assigneeId
        self.explicitNulls = explicitNulls
    }

    enum CodingKeys: String, CodingKey {
        case ids, status, statusId, priority, assigneeId
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(ids, forKey: .ids)
        try c.encodeIfPresent(status, forKey: .status)
        try c.encodeIfPresent(statusId, forKey: .statusId)
        try c.encodeIfPresent(priority, forKey: .priority)
        if assigneeId != nil {
            try c.encode(assigneeId, forKey: .assigneeId)
        } else if explicitNulls.contains(CodingKeys.assigneeId.rawValue) {
            try c.encodeNil(forKey: .assigneeId)
        }
    }
}

public struct DeleteIssueInput: Encodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

/// Input for `issues.closePr` (EXP-100): close the issue's open PR WITHOUT
/// merging (the reject path — the work exists but the issue got dropped).
public struct ClosePrInput: Encodable, Sendable {
    public let issueId: String

    public init(issueId: String) {
        self.issueId = issueId
    }
}

/// Input for `issues.mergePr`: squash-merge the issue's open PR via the GitHub
/// App. For a batch PR (one PR linked to several issues) the server resolves
/// the PR to ALL its linked issues, so passing any one of them merges the PR
/// and completes them all.
/// EXP-358: `closeSessions` is the "Merge and close" path — a merge alone now
/// only flips live sessions to `merged` (they stay alive/steerable), so ending
/// the run has to be asked for explicitly.
public struct MergePrInput: Encodable, Sendable {
    public let issueId: String
    public let closeSessions: Bool

    public init(issueId: String, closeSessions: Bool = false) {
        self.issueId = issueId
        self.closeSessions = closeSessions
    }
}

/// Input for `issues.move` (EXP-57): move an issue to another board in the
/// SAME team. The server renumbers the issue in the target board
/// (Linear-style, EXP-42 → ABC-17) — the issue keeps its id but changes
/// `boardId`/`number`/`identifier`, which Electric echoes back into GRDB.
public struct MoveIssueInput: Encodable, Sendable {
    public let id: String
    public let boardId: String

    public init(id: String, boardId: String) {
        self.id = id
        self.boardId = boardId
    }
}

public struct IssueResult: Decodable, Sendable {
    public let issue: IssueResultData

    public init(issue: IssueResultData) {
        self.issue = issue
    }
}

public struct IssueResultData: Decodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

// MARK: - PR diff (issues.prFiles)

public struct PrFilesInput: Encodable, Sendable {
    public let issueId: String
    public init(issueId: String) { self.issueId = issueId }
}

public struct PrFile: Decodable, Sendable, Identifiable {
    public let filename: String
    public let status: String
    public let additions: Int
    public let deletions: Int
    public let patch: String?

    public var id: String { filename }
}

public struct PrFilesResult: Decodable, Sendable {
    public let repo: String?
    public let prNumber: Int?
    public let files: [PrFile]
}

// MARK: - Server search (issues.search)

public struct SearchIssuesInput: Encodable, Sendable {
    public let teamId: String
    public let query: String
    /// Server default 20, max 50. Omitted from the JSON when nil.
    public var limit: Int?

    public init(teamId: String, query: String, limit: Int? = nil) {
        self.teamId = teamId
        self.query = query
        self.limit = limit
    }
}

/// One relevance-ordered hit from `issues.search` — a slim projection, not the
/// full issue row. Full data comes from the local GRDB store when the id is
/// already synced.
public struct SearchIssueHit: Decodable, Sendable, Identifiable {
    public let id: String
    public let identifier: String
    public let title: String
    public let boardId: String
    public let status: String
    public let priority: String

    public init(id: String, identifier: String, title: String, boardId: String, status: String, priority: String) {
        self.id = id
        self.identifier = identifier
        self.title = title
        self.boardId = boardId
        self.status = status
        self.priority = priority
    }
}

// MARK: - Point read (issues.get)

public struct GetIssueInput: Encodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

/// One issue row read straight from the server (EXP-264) — the fallback for a
/// screen asked to show an issue the Electric shape hasn't delivered yet (a
/// push tap on a brand-new issue, a deep link into a board this account is
/// still syncing). The field list mirrors the issues shape's server-pinned
/// column allowlist EXACTLY, so the row can be merged into the local store
/// verbatim; `entity()` does that mapping. Timestamps arrive as ISO strings
/// (tRPC JSON-stringifies Dates) rather than the Postgres text the wire
/// protocol carries — WireTimestamps parses both, and the synced row
/// overwrites this one the moment it lands.
public struct FetchedIssue: Decodable, Sendable {
    public let id: String
    public let boardId: String
    public let number: Int?
    public let identifier: String?
    public let title: String
    public let description: String?
    public let status: String
    /// EXP-314 — nullable, and absent on older servers.
    public let statusId: String?
    public let priority: String
    public let assigneeId: String?
    public let creatorId: String?
    public let source: String?
    public let dueDate: String?
    public let sortOrder: Double?
    public let completedAt: String?
    public let duplicateOfId: String?
    public let prUrl: String?
    public let prNumber: Int?
    public let prState: String?
    public let branch: String?
    public let prMergedAt: String?
    public let createdAt: String
    public let updatedAt: String
}

/// `issues.get`'s envelope. `teamId` is TOP-LEVEL, not a field of the issue:
/// it isn't part of the synced row, but the local `issue_labels` rows carry it
/// denormalized, so writing the labels needs it.
public struct IssueGetResult: Decodable, Sendable {
    public let issue: FetchedIssue
    public let labelIds: [String]
    public let teamId: String
}

// MARK: - API

public final class IssuesApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func create(accountId: String, _ input: CreateIssueInput) async throws -> String {
        let result: IssueResult = try await trpc.mutation(accountId: accountId, path: "issues.create", input: input)
        return result.issue.id
    }

    public func update(accountId: String, _ input: UpdateIssueInput) async throws {
        let _: IssueResult = try await trpc.mutation(accountId: accountId, path: "issues.update", input: input)
    }

    /// Apply one property change to a whole selection through the same
    /// `issues.bulkUpdate` procedure web and desktop use. It runs the batch in
    /// ONE transaction and deliberately skips the per-issue notification
    /// fan-out past 25 ids — the client-side loop of `issues.update` calls it
    /// replaces had neither, so a 60-issue assign fired 60 pushes at one
    /// person and could half-apply. Callers chunk at the server's 200-id cap.
    public func bulkUpdate(accountId: String, _ input: BulkUpdateIssuesInput) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "issues.bulkUpdate", input: input)
    }

    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "issues.delete", input: DeleteIssueInput(id: id))
    }

    /// Close the issue's open PR WITHOUT merging (EXP-100 — the reject path
    /// for an issue that got dropped after the work was done). Server-side
    /// via the GitHub App; the `prState` flip arrives through Electric sync.
    public func closePr(accountId: String, issueId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "issues.closePr", input: ClosePrInput(issueId: issueId))
    }

    /// Squash-merge the issue's open PR via the GitHub App (EXP-131). Server
    /// resolves a batch PR to every linked issue, so merging completes them all;
    /// the `prState`/`status` flips arrive through Electric sync.
    /// `closeSessions` (EXP-358) additionally ENDS the linked coding sessions —
    /// merge-only leaves them alive on the new `merged` status, so every
    /// plain merge affordance keeps the default.
    public func mergePr(accountId: String, issueId: String, closeSessions: Bool = false) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "issues.mergePr",
            input: MergePrInput(issueId: issueId, closeSessions: closeSessions)
        )
    }

    /// Move the issue to another board in the same team (EXP-57). The
    /// response also carries the fresh identity (`issue` + target slug); only
    /// the standard `{issue: {id}}` envelope is decoded — clients pick up the
    /// new identifier/boardId from Electric sync like every other mutation.
    public func move(accountId: String, id: String, boardId: String) async throws {
        let _: IssueResult = try await trpc.mutation(
            accountId: accountId,
            path: "issues.move",
            input: MoveIssueInput(id: id, boardId: boardId)
        )
    }

    /// The changed files for the issue's PR (one issue = one PR), for the diff
    /// view. `issues.prFiles` is a `.query`, so this uses the GET-with-input
    /// helper. Returns `repo == nil` / empty `files` when there's no PR yet.
    public func prFiles(accountId: String, issueId: String) async throws -> PrFilesResult {
        try await trpc.query(accountId: accountId, path: "issues.prFiles", input: PrFilesInput(issueId: issueId))
    }

    /// Server-side full-text search (title + description + comment text) over
    /// one team, relevance-ordered. `issues.search` is a `.query`, so this
    /// uses the same GET-with-input helper as `prFiles`. Requires the caller to
    /// be a member of `teamId`.
    public func search(accountId: String, teamId: String, query: String, limit: Int? = nil) async throws -> [SearchIssueHit] {
        try await trpc.query(
            accountId: accountId,
            path: "issues.search",
            input: SearchIssuesInput(teamId: teamId, query: query, limit: limit)
        )
    }

    /// Point-read ONE issue by row id or human identifier ("EXP-42"), for
    /// screens that must show an issue before sync delivers it (EXP-264).
    /// `issues.get` is a `.query`, so this takes the same GET-with-input
    /// helper `prFiles`/`search` use. Throws NOT_FOUND for an unknown or
    /// trashed issue, FORBIDDEN for a non-member — and plain HTTP 404 against
    /// an older server that has no such procedure, so callers must treat any
    /// error as "not available" rather than a failure worth surfacing.
    public func get(accountId: String, id: String) async throws -> IssueGetResult {
        try await trpc.query(
            accountId: accountId,
            path: "issues.get",
            input: GetIssueInput(id: id)
        )
    }
}
