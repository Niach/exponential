import Foundation

// #FEED-219 (fixtures/composer-draft.json): every composer's synced draft.
// Rows arrive over the `composer-drafts` shape (static `user_id = me`); the
// two writes live here (`composerDrafts.upsert` refuses an empty text: that
// is a `delete`, `ComposerDraft.write`).

/// The camelCase row `composerDrafts.upsert` answers with.
public struct ComposerDraftDto: Decodable, Sendable {
    public let id: String
    public let userId: String
    public let teamId: String
    public let key: String
    public let text: String
    public let createdAt: String?
    public let updatedAt: String?

    /// Mirror the server's row into the local store so a remount reads it
    /// before the long-poll delivers it; sync overwrites it with the same row.
    public func entity() -> ComposerDraftEntity {
        ComposerDraftEntity(
            id: id, userId: userId, teamId: teamId, key: key, text: text,
            createdAt: createdAt ?? "", updatedAt: updatedAt ?? ""
        )
    }
}

private struct ComposerDraftResult: Decodable {
    let draft: ComposerDraftDto
}

private struct ComposerDraftUpsertInput: Encodable {
    let teamId: String
    let key: String
    let text: String
}

private struct ComposerDraftKeyInput: Encodable {
    let key: String
}

public final class ComposerDraftsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func upsert(accountId: String, teamId: String, key: String, text: String) async throws -> ComposerDraftDto {
        let result: ComposerDraftResult = try await trpc.mutation(
            accountId: accountId, path: "composerDrafts.upsert",
            input: ComposerDraftUpsertInput(teamId: teamId, key: key, text: text)
        )
        return result.draft
    }

    /// Deleting nothing is fine; the `{txId, deleted}` body carries nothing
    /// the client needs.
    public func delete(accountId: String, key: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId, path: "composerDrafts.delete", input: ComposerDraftKeyInput(key: key)
        )
    }
}
