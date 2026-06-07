import Foundation

// Comment bodies are plain GFM markdown strings (the server stores them in a
// `text` column; the legacy jsonb `{text}` envelope was dropped in Phase F).
public struct CreateCommentInput: Encodable, Sendable {
    public let issueId: String
    public let body: String

    public init(issueId: String, body: String) {
        self.issueId = issueId
        self.body = body
    }
}

public struct UpdateCommentInput: Encodable, Sendable {
    public let id: String
    public let body: String

    public init(id: String, body: String) {
        self.id = id
        self.body = body
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

    public func create(accountId: String, issueId: String, text: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "comments.create",
            input: CreateCommentInput(issueId: issueId, body: text)
        )
    }

    public func update(accountId: String, id: String, text: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "comments.update",
            input: UpdateCommentInput(id: id, body: text)
        )
    }

    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "comments.delete", input: DeleteCommentInput(id: id))
    }
}
