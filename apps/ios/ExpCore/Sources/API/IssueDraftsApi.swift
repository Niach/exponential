import Foundation

// EXP-878 — issue drafts: a composed-but-unfiled issue, owned by ONE user.
// Rows arrive over the 22nd Electric shape (`issue_drafts`, static
// `user_id = me`); the three writes live here.
//
// The DTO + its `entity()` mapping deliberately sit in THIS file rather than in
// `IssuesApi.swift`: the Share Extension compiles that one, and it has no
// business carrying the drafts surface.

/// The camelCase draft row `issueDrafts.upsert` answers with. Everything but
/// the identity columns is tolerated absent so a leaner server response never
/// fails the call — the write already happened.
public struct IssueDraftDto: Decodable, Sendable {
    public let id: String
    public let userId: String
    public let teamId: String
    public let boardId: String
    public let title: String?
    public let description: String?
    public let statusId: String?
    public let priority: String?
    public let assigneeId: String?
    public let labelIds: [String]?
    public let dueDate: String?
    public let createdAt: String?
    public let updatedAt: String?

    public init(
        id: String,
        userId: String,
        teamId: String,
        boardId: String,
        title: String? = nil,
        description: String? = nil,
        statusId: String? = nil,
        priority: String? = nil,
        assigneeId: String? = nil,
        labelIds: [String]? = nil,
        dueDate: String? = nil,
        createdAt: String? = nil,
        updatedAt: String? = nil
    ) {
        self.id = id
        self.userId = userId
        self.teamId = teamId
        self.boardId = boardId
        self.title = title
        self.description = description
        self.statusId = statusId
        self.priority = priority
        self.assigneeId = assigneeId
        self.labelIds = labelIds
        self.dueDate = dueDate
        self.createdAt = createdAt
        self.updatedAt = updatedAt
    }

    /// Mirror the server's row into the local store so the Drafts list and a
    /// re-open render before the Electric long-poll delivers it (the
    /// `mirrorCreatedIssue` pattern). Sync re-delivers the same row and
    /// overwrites this one.
    public func entity() -> IssueDraftEntity {
        IssueDraftEntity(
            id: id,
            userId: userId,
            teamId: teamId,
            boardId: boardId,
            title: title ?? "",
            description: description,
            statusId: statusId,
            priority: priority ?? IssuePriority.none.rawValue,
            assigneeId: assigneeId,
            labelIds: labelIds ?? [],
            dueDate: dueDate,
            createdAt: createdAt ?? "",
            updatedAt: updatedAt ?? ""
        )
    }
}

/// One attachment already uploaded against a draft (`issueDrafts.listAttachments`).
/// The drafts' rows are EXCLUDED from the attachments shape (`issue_id IS NOT
/// NULL`), so this tRPC read is the only way to see them — `issueId`/`boardId`
/// come back null and are simply not decoded.
public struct DraftAttachmentDto: Decodable, Identifiable, Sendable {
    public let id: String
    public let filename: String
    public let contentType: String
    public let sizeBytes: Int
    public let url: String
    public let width: Int?
    public let height: Int?
    public let durationMs: Int?
    public let posterStorageKey: String?
    public let createdAt: String?

    public init(
        id: String,
        filename: String,
        contentType: String,
        sizeBytes: Int,
        url: String,
        width: Int? = nil,
        height: Int? = nil,
        durationMs: Int? = nil,
        posterStorageKey: String? = nil,
        createdAt: String? = nil
    ) {
        self.id = id
        self.filename = filename
        self.contentType = contentType
        self.sizeBytes = sizeBytes
        self.url = url
        self.width = width
        self.height = height
        self.durationMs = durationMs
        self.posterStorageKey = posterStorageKey
        self.createdAt = createdAt
    }

    enum CodingKeys: String, CodingKey {
        case id, filename, contentType, sizeBytes, url, width, height
        case durationMs, posterStorageKey, createdAt
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        filename = (try? c.decode(String.self, forKey: .filename)) ?? "file"
        contentType = (try? c.decode(String.self, forKey: .contentType))
            ?? "application/octet-stream"
        sizeBytes = (try? c.decodeWireInt(forKey: .sizeBytes)) ?? 0
        url = (try? c.decode(String.self, forKey: .url)) ?? ""
        width = try? c.decodeWireInt(forKey: .width)
        height = try? c.decodeWireInt(forKey: .height)
        durationMs = try? c.decodeWireInt(forKey: .durationMs)
        posterStorageKey = try c.decodeIfPresent(String.self, forKey: .posterStorageKey)
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt)
    }
}

/// The full snapshot ONE close (or the first eager upload) writes. A draft is
/// rewritten whole, never patched: `labelIds` always rides along, and the
/// nullable picks are omitted when unset.
public struct UpsertIssueDraftInput: Encodable, Sendable {
    public let id: String
    public let teamId: String
    public let boardId: String
    public let title: String
    public let description: String
    public var statusId: String?
    public var priority: String?
    public var assigneeId: String?
    public var labelIds: [String]
    public var dueDate: String?

    public init(
        id: String,
        teamId: String,
        boardId: String,
        title: String,
        description: String,
        statusId: String? = nil,
        priority: String? = nil,
        assigneeId: String? = nil,
        labelIds: [String] = [],
        dueDate: String? = nil
    ) {
        self.id = id
        self.teamId = teamId
        self.boardId = boardId
        self.title = title
        self.description = description
        self.statusId = statusId
        self.priority = priority
        self.assigneeId = assigneeId
        self.labelIds = labelIds
        self.dueDate = dueDate
    }
}

/// Server envelope: `issueDrafts.upsert` returns `{ draft, txId }`.
public struct IssueDraftResult: Decodable, Sendable {
    public let draft: IssueDraftDto

    public init(draft: IssueDraftDto) {
        self.draft = draft
    }
}

private struct IssueDraftIdInput: Encodable {
    let id: String
}

public final class IssueDraftsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Write the whole draft (create or replace). Returns the server row so
    /// the caller can mirror it locally without waiting for sync.
    public func upsert(accountId: String, _ input: UpsertIssueDraftInput) async throws -> IssueDraftDto {
        let result: IssueDraftResult = try await trpc.mutation(
            accountId: accountId, path: "issueDrafts.upsert", input: input
        )
        return result.draft
    }

    /// Drop a draft and everything bound to it. The `{txId, deleted}` body
    /// carries nothing the client needs.
    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "issueDrafts.delete", input: IssueDraftIdInput(id: id)
        )
    }

    /// The draft's own attachments — never synced (the attachments shape is
    /// scoped to `issue_id IS NOT NULL`), so re-opening a draft reads them here.
    public func listAttachments(accountId: String, id: String) async throws -> [DraftAttachmentDto] {
        try await trpc.query(
            accountId: accountId, path: "issueDrafts.listAttachments",
            input: IssueDraftIdInput(id: id)
        )
    }
}
