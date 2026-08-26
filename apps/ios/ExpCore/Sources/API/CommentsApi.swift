import Foundation

// Comment bodies are plain GFM markdown strings (the server stores them in a
// `text` column; the legacy jsonb `{text}` envelope was dropped in Phase F).
public struct CreateCommentInput: Encodable, Sendable {
    public let issueId: String
    public let body: String
    /// EXP-554 — the attachment rows this comment links (uploaded first through
    /// the REST image/file routes). Optional on the wire: the server defaults it
    /// to `[]`, and Swift's synthesized `encode(to:)` uses `encodeIfPresent`, so
    /// a nil simply never appears in the JSON body.
    public let attachmentIds: [String]?

    public init(issueId: String, body: String, attachmentIds: [String]? = nil) {
        self.issueId = issueId
        self.body = body
        self.attachmentIds = attachmentIds
    }
}

public struct UpdateCommentInput: Encodable, Sendable {
    public let id: String
    public let body: String
    /// The FULL desired set — rows linked to this comment but missing from it
    /// are hard-deleted server-side. Omitted (nil) leaves attachments untouched,
    /// which is what the MCP tools send.
    public let attachmentIds: [String]?

    public init(id: String, body: String, attachmentIds: [String]? = nil) {
        self.id = id
        self.body = body
        self.attachmentIds = attachmentIds
    }
}

public struct DeleteCommentInput: Encodable, Sendable {
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

// The web tRPC handlers return { txId, comment } on create/update and { txId }
// on delete. We don't read txId on iOS — Electric eventually delivers the
// canonical row — so we accept any decodable response shape.
private struct EmptyResult: Decodable {}

public final class CommentsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func create(
        accountId: String,
        issueId: String,
        text: String,
        attachmentIds: [String]? = nil
    ) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "comments.create",
            input: CreateCommentInput(issueId: issueId, body: text, attachmentIds: attachmentIds)
        )
    }

    public func update(
        accountId: String,
        id: String,
        text: String,
        attachmentIds: [String]? = nil
    ) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "comments.update",
            input: UpdateCommentInput(id: id, body: text, attachmentIds: attachmentIds)
        )
    }

    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "comments.delete", input: DeleteCommentInput(id: id))
    }
}
