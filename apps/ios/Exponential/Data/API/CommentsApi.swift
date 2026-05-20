import Foundation

struct CommentBody: Encodable {
    let text: String
}

struct CreateCommentInput: Encodable {
    let issueId: String
    let body: CommentBody
}

struct UpdateCommentInput: Encodable {
    let id: String
    let body: CommentBody
}

struct DeleteCommentInput: Encodable {
    let id: String
}

// The web tRPC handlers return { txId, comment } on create/update and { txId }
// on delete. We don't read txId on iOS — Electric eventually delivers the
// canonical row — so we accept any decodable response shape.
private struct EmptyResult: Decodable {}

final class CommentsApi: Sendable {
    private let trpc: TrpcClient

    init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    func create(issueId: String, text: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            path: "comments.create",
            input: CreateCommentInput(issueId: issueId, body: CommentBody(text: text))
        )
    }

    func update(id: String, text: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            path: "comments.update",
            input: UpdateCommentInput(id: id, body: CommentBody(text: text))
        )
    }

    func delete(id: String) async throws {
        try await trpc.mutationVoid(path: "comments.delete", input: DeleteCommentInput(id: id))
    }
}
