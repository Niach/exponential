import Foundation

// Mirrors apps/web/src/lib/trpc/agent-plan.ts. The agent submits plans via the
// same router; this surface exposes the human-side actions (approve / request
// changes / retry / answer a question) plus the structured plan/question read
// (`getState`) that backs the native Plan Panel.
private struct IssueIdInput: Encodable {
    let issueId: String
}

private struct AnswerInput: Encodable {
    let issueId: String
    let answer: String
}

private struct EmptyResult: Decodable {}

/// The structured agent plan/question state for an issue. The plan/question TEXT
/// is server-only (not synced via Electric), so it's fetched on demand here.
/// Keys match the camelCase tRPC payload (the server uses no transformer), so a
/// plain `Decodable` with matching property names maps 1:1.
public struct AgentPlanStateResult: Decodable, Sendable {
    public let planText: String?
    public let question: String?
    public let questionAskedAt: String?
    public let state: String?
    public let revision: Int
    public let approvedAt: String?

    public init(
        planText: String?,
        question: String?,
        questionAskedAt: String?,
        state: String?,
        revision: Int,
        approvedAt: String?
    ) {
        self.planText = planText
        self.question = question
        self.questionAskedAt = questionAskedAt
        self.state = state
        self.revision = revision
        self.approvedAt = approvedAt
    }
}

public final class AgentPlanApi: Sendable {
    private let trpc: TrpcClient

    public init(trpc: TrpcClient) {
        self.trpc = trpc
    }

    /// Read the structured plan/question text + state for an issue. tRPC routes
    /// reads as GET, so this uses the input-bearing `query` helper.
    public func getState(accountId: String, issueId: String) async throws -> AgentPlanStateResult {
        try await trpc.query(
            accountId: accountId,
            path: "agentPlan.getState",
            input: IssueIdInput(issueId: issueId)
        )
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

    /// Record the human's answer to the agent's open question. The server clears
    /// the question, flips the issue to `drafting` (the re-plan signal), and
    /// records an `agent_answer` event.
    public func answerQuestion(accountId: String, issueId: String, answer: String) async throws {
        try await trpc.mutationVoid(
            accountId: accountId,
            path: "agentPlan.answerQuestion",
            input: AnswerInput(issueId: issueId, answer: answer)
        )
    }
}
