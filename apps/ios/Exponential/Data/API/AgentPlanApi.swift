import Foundation

// Mirrors apps/web/src/lib/trpc/agent-plan.ts. The daemon submits plans via
// the same router; the iOS surface here only exposes the human-side
// approve / request-changes / retry actions.
private struct IssueIdInput: Encodable {
    let issueId: String
}

private struct EmptyResult: Decodable {}

final class AgentPlanApi: Sendable {
    private let trpc: TrpcClient

    init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    func approvePlan(issueId: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            path: "agentPlan.approvePlan",
            input: IssueIdInput(issueId: issueId)
        )
    }

    func requestChanges(issueId: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            path: "agentPlan.requestChanges",
            input: IssueIdInput(issueId: issueId)
        )
    }

    func retry(issueId: String) async throws {
        let _: EmptyResult = try await trpc.mutation(
            path: "agentPlan.retry",
            input: IssueIdInput(issueId: issueId)
        )
    }
}
