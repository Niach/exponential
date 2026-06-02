import Foundation

// Mirrors apps/web/src/lib/trpc/agent-plan.ts. The daemon submits plans via
// the same router; the iOS surface here only exposes the human-side
// approve / request-changes / retry actions.
private struct IssueIdInput: Encodable {
    let issueId: String
}

private struct EmptyResult: Decodable {}

public final class AgentPlanApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    public func approvePlan(accountId: String, issueId: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "agentPlan.approvePlan",
            input: IssueIdInput(issueId: issueId)
        )
    }

    public func requestChanges(accountId: String, issueId: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "agentPlan.requestChanges",
            input: IssueIdInput(issueId: issueId)
        )
    }

    public func retry(accountId: String, issueId: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            accountId: accountId,
            path: "agentPlan.retry",
            input: IssueIdInput(issueId: issueId)
        )
    }
}
