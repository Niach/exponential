import Foundation

// Mirrors apps/web/src/lib/trpc/subscriptions.ts — per-issue subscribe toggle.
private struct SubIssueIdInput: Encodable {
    let issueId: String
}

private struct EmptyResult: Decodable {}

public final class SubscriptionsApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func subscribe(accountId: String, issueId: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "subscriptions.subscribe",
            input: SubIssueIdInput(issueId: issueId)
        )
    }

    public func unsubscribe(accountId: String, issueId: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "subscriptions.unsubscribe",
            input: SubIssueIdInput(issueId: issueId)
        )
    }
}
