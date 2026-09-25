import Foundation
import XCTest
@testable import ExpCore

// EXP-1082 / EXP-1065: a workflow's open questions, read off
// `coding_sessions.pending_question` (Android `WorkflowQuestionsTest` and the
// web `open-questions.test.ts` carry the same table).
final class WorkflowQuestionsTests: XCTestCase {
    private func run(
        _ id: String,
        workflowId: String?,
        nodeId: String?,
        status: String = "running",
        question: String? = nil
    ) -> CodingSessionEntity {
        let stamp = "2026-09-25T09:00:00Z"
        return CodingSessionEntity(
            id: id,
            issueId: nil,
            teamId: "team-1",
            userId: "user-1",
            deviceLabel: "macbook",
            status: status,
            workflowId: workflowId,
            workflowNodeId: nodeId,
            workflowRole: DomainContract.wfSessionRoleAuthor,
            pendingQuestion: question.map { "{\"question\":\"\($0)\",\"askedAt\":\"2026-09-25T10:00:00Z\"}" },
            startedAt: stamp,
            endedAt: nil,
            createdAt: stamp,
            updatedAt: stamp
        )
    }

    func testListsTheOpenQuestionOfEachLiveRunOfTheWorkflow() throws {
        let sessions = [
            run("s1", workflowId: "wf-1", nodeId: "n1", question: "Which API?"),
            run("s2", workflowId: "wf-1", nodeId: "n2"),
            run("s3", workflowId: "wf-1", nodeId: "n3", status: "ended", question: "Stale?"),
            run("s4", workflowId: "wf-2", nodeId: "n4", question: "Other workflow?"),
        ]
        let open = WorkflowQuestions.open(sessions, workflowId: "wf-1")
        XCTAssertEqual(open.map(\.sessionId), ["s1"])
        XCTAssertEqual(open.first?.nodeId, "n1")
        XCTAssertEqual(open.first?.question, "Which API?")
        XCTAssertEqual(open.first?.askedAt, "2026-09-25T10:00:00Z")
    }

    /// A planner run (no node) and a blank question are left out; the rest
    /// reads oldest first.
    func testSkipsNodeLessAndBlankQuestionsAndOrdersByAskedAt() {
        let stamped = { (id: String, node: String?, question: String, at: String) -> CodingSessionEntity in
            CodingSessionEntity(
                id: id,
                issueId: nil,
                teamId: "team-1",
                userId: "user-1",
                deviceLabel: "macbook",
                status: "in_review",
                workflowId: "wf-1",
                workflowNodeId: node,
                workflowRole: DomainContract.wfSessionRoleAuthor,
                pendingQuestion: "{\"question\":\"\(question)\",\"askedAt\":\"\(at)\"}",
                startedAt: "2026-09-25T09:00:00Z",
                endedAt: nil,
                createdAt: "2026-09-25T09:00:00Z",
                updatedAt: "2026-09-25T09:00:00Z"
            )
        }
        let sessions = [
            stamped("b", "node-b", "Later?", "2026-09-25T11:00:00Z"),
            stamped("plan", nil, "Runner device?", "2026-09-25T09:00:00Z"),
            stamped("a", "node-a", "Earlier?", "2026-09-25T10:00:00Z"),
            stamped("blank", "node-c", "   ", "2026-09-25T09:30:00Z"),
        ]
        XCTAssertEqual(WorkflowQuestions.open(sessions, workflowId: "wf-1").map(\.sessionId), ["a", "b"])
    }
}
