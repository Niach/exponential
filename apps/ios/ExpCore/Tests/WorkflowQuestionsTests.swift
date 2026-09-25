import Foundation
import XCTest
@testable import ExpCore

// EXP-1082: a workflow's open questions, read off `coding_sessions.pending_question`.
// Pending until EXP-1065 implements `WorkflowQuestions.open` (Android
// `WorkflowQuestionsTest` carries the same table).
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
        throw XCTSkip("EXP-1065 implements the selector")
        let open = WorkflowQuestions.open(sessions, workflowId: "wf-1")
        XCTAssertEqual(open.map(\.sessionId), ["s1"])
        XCTAssertEqual(open.first?.nodeId, "n1")
        XCTAssertEqual(open.first?.question, "Which API?")
        XCTAssertEqual(open.first?.askedAt, "2026-09-25T10:00:00Z")
    }
}
