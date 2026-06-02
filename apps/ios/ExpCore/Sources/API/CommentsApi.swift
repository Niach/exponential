import Foundation

public struct CommentBody: Encodable, Sendable {
    public let text: String

    public init(text: String) {
        self.text = text
    }
}

public struct CreateCommentInput: Encodable, Sendable {
    public let issueId: String
    public let body: CommentBody

    public init(issueId: String, body: CommentBody) {
        self.issueId = issueId
        self.body = body
    }
}

public struct UpdateCommentInput: Encodable, Sendable {
    public let id: String
    public let body: CommentBody

    public init(id: String, body: CommentBody) {
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
            input: CreateCommentInput(issueId: issueId, body: CommentBody(text: text))
        )
    }

    public func update(accountId: String, id: String, text: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "comments.update",
            input: UpdateCommentInput(id: id, body: CommentBody(text: text))
        )
    }

    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "comments.delete", input: DeleteCommentInput(id: id))
    }
}
