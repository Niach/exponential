import Foundation

/// EXP-1082 (workflow contract): the OPEN QUESTIONS of a workflow — every live
/// run of it that asked a person something. The question lives on the run
/// itself, in `coding_sessions.pending_question` (`{question, askedAt}` jsonb,
/// raw text on `CodingSessionEntity.pendingQuestion`), not in a table of its
/// own. STUB: EXP-1065 implements the selector (web `lib/workflow-questions.ts`
/// mirrors it ×4).
public struct WorkflowOpenQuestion: Sendable, Equatable {
    public let nodeId: String
    public let sessionId: String
    public let question: String
    public let askedAt: String

    public init(nodeId: String, sessionId: String, question: String, askedAt: String) {
        self.nodeId = nodeId
        self.sessionId = sessionId
        self.question = question
        self.askedAt = askedAt
    }
}

public enum WorkflowQuestions {
    /// The open question of each live run of `workflowId`. STUB → [].
    public static func open(_ sessions: [CodingSessionEntity], workflowId: String) -> [WorkflowOpenQuestion] {
        []
    }
}
