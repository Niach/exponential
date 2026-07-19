import Foundation

// MARK: - Input/Output types

public struct CreateIssueInput: Encodable, Sendable {
    public let boardId: String
    public let title: String
    public var status: String?
    public var priority: String?
    public var assigneeId: String?
    public var description: String?
    public var dueDate: String?
    public var dueTime: String?
    public var endTime: String?
    public var labelIds: [String]?

    public init(
        boardId: String,
        title: String,
        status: String? = nil,
        priority: String? = nil,
        assigneeId: String? = nil,
        description: String? = nil,
        dueDate: String? = nil,
        dueTime: String? = nil,
        endTime: String? = nil,
        labelIds: [String]? = nil
    ) {
        self.boardId = boardId
        self.title = title
        self.status = status
        self.priority = priority
        self.assigneeId = assigneeId
        self.description = description
        self.dueDate = dueDate
        self.dueTime = dueTime
        self.endTime = endTime
        self.labelIds = labelIds
    }
}

public struct UpdateIssueInput: Encodable, Sendable {
    public let id: String
    public var title: String?
    public var status: String?
    public var priority: String?
    public var assigneeId: String?
    public var description: String?
    public var dueDate: String?
    public var dueTime: String?
    public var endTime: String?
    /// Canonical issue this one duplicates — set together with
    /// `status = "duplicate"` in ONE update so the marking is atomic.
    public var duplicateOfId: String?
    public var archivedAt: String?

    // Fields listed here are encoded as JSON null (not omitted).
    // Use this when the server must distinguish "clear this field" from "don't touch it".
    public var explicitNulls: Set<String> = []

    public init(
        id: String,
        title: String? = nil,
        status: String? = nil,
        priority: String? = nil,
        assigneeId: String? = nil,
        description: String? = nil,
        dueDate: String? = nil,
        dueTime: String? = nil,
        endTime: String? = nil,
        duplicateOfId: String? = nil,
        archivedAt: String? = nil,
        explicitNulls: Set<String> = []
    ) {
        self.id = id
        self.title = title
        self.status = status
        self.priority = priority
        self.assigneeId = assigneeId
        self.description = description
        self.dueDate = dueDate
        self.dueTime = dueTime
        self.endTime = endTime
        self.duplicateOfId = duplicateOfId
        self.archivedAt = archivedAt
        self.explicitNulls = explicitNulls
    }

    enum CodingKeys: String, CodingKey {
        case id, title, status, priority, assigneeId, description
        case dueDate, dueTime, endTime
        case duplicateOfId, archivedAt
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encodeIfPresent(title, forKey: .title)
        try c.encodeIfPresent(status, forKey: .status)
        try c.encodeIfPresent(priority, forKey: .priority)
        try encodeNullable(assigneeId, forKey: .assigneeId, in: &c)
        try encodeNullable(description, forKey: .description, in: &c)
        try encodeNullable(dueDate, forKey: .dueDate, in: &c)
        try encodeNullable(dueTime, forKey: .dueTime, in: &c)
        try encodeNullable(endTime, forKey: .endTime, in: &c)
        try encodeNullable(duplicateOfId, forKey: .duplicateOfId, in: &c)
        try encodeNullable(archivedAt, forKey: .archivedAt, in: &c)
    }

    private func encodeNullable<T: Encodable>(_ value: T?, forKey key: CodingKeys, in container: inout KeyedEncodingContainer<CodingKeys>) throws {
        if value != nil {
            try container.encode(value, forKey: key)
        } else if explicitNulls.contains(key.rawValue) {
            try container.encodeNil(forKey: key)
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
public struct MergePrInput: Encodable, Sendable {
    public let issueId: String

    public init(issueId: String) {
        self.issueId = issueId
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
    public func mergePr(accountId: String, issueId: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "issues.mergePr", input: MergePrInput(issueId: issueId))
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
}
