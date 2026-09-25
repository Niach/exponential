import Foundation

/// EXP-1065 (workflow contract EXP-1082): the OPEN QUESTIONS of a workflow —
/// every live run of it that asked a person something. The question lives on
/// the run itself, in `coding_sessions.pending_question` (`{question, askedAt}`
/// jsonb, raw text on `CodingSessionEntity.pendingQuestion`), not in a table
/// of its own. The ONE rule, ×4 (web `lib/workflows/open-questions.ts`,
/// desktop `domain::workflow_questions`, Android `WorkflowQuestions`): a live
/// run (`running` / `in_review`) of the workflow, on a NODE, with a non-blank
/// question; a planner run's question (no node) reaches the person through
/// the notification and its own composer. An open question never changes a
/// node's state, only the badge.
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
    private static let liveStatuses: Set<String> = ["running", "in_review"]

    /// The open question of each live run of `workflowId`, oldest first.
    public static func open(_ sessions: [CodingSessionEntity], workflowId: String) -> [WorkflowOpenQuestion] {
        var open: [WorkflowOpenQuestion] = []
        for session in sessions {
            guard session.workflowId == workflowId,
                  let nodeId = session.workflowNodeId,
                  liveStatuses.contains(session.status),
                  let pending = parse(session.pendingQuestion),
                  let question = (pending["question"] as? String)?
                      .trimmingCharacters(in: .whitespacesAndNewlines),
                  !question.isEmpty
            else { continue }
            open.append(WorkflowOpenQuestion(
                nodeId: nodeId,
                sessionId: session.id,
                question: question,
                askedAt: pending["askedAt"] as? String ?? ""
            ))
        }
        return open.sorted {
            $0.askedAt == $1.askedAt ? $0.sessionId < $1.sessionId : $0.askedAt < $1.askedAt
        }
    }

    /// The jsonb text as an object; `nil` for no question or garbled text.
    private static func parse(_ raw: String?) -> [String: Any]? {
        guard let raw, let data = raw.data(using: .utf8),
              let object = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
        else { return nil }
        return object
    }
}
