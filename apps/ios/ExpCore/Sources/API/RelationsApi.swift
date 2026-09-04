import Foundation

// Mirrors apps/web/src/lib/trpc/relations.ts (EXP-736) — typed links between
// two issues. The server owns the canonical direction: `inverse` says the
// CALLER's issue is the related side, so it swaps the pair before writing.
public struct CreateRelationInput: Encodable, Sendable {
    public let issueId: String
    public let relatedIssueId: String
    /// An `IssueRelationType` raw value.
    public let type: String
    public let inverse: Bool

    public init(issueId: String, relatedIssueId: String, type: String, inverse: Bool) {
        self.issueId = issueId
        self.relatedIssueId = relatedIssueId
        self.type = type
        self.inverse = inverse
    }
}

public struct DeleteRelationInput: Encodable, Sendable {
    /// The `issue_relations` row id.
    public let id: String

    public init(id: String) {
        self.id = id
    }
}

public final class RelationsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Both mutations return `{txId}` only — the row itself arrives over the
    /// `issue-relations` shape, so the body is ignored.
    public func create(accountId: String, _ input: CreateRelationInput) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "relations.create", input: input)
    }

    public func delete(accountId: String, id: String) async throws {
        try await trpc.mutationVoid(accountId: accountId, path: "relations.delete", input: DeleteRelationInput(id: id))
    }
}
