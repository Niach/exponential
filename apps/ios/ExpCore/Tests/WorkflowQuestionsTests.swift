import Foundation
import XCTest
@testable import ExpCore

// EXP-1082: a workflow's open questions, read off `coding_sessions.pending_question`.
// Pending until EXP-1065 implements `WorkflowQuestions.open`.
final class WorkflowQuestionsTests: XCTestCase {
    func testListsTheOpenQuestionOfEachLiveRunOfTheWorkflow() throws {
        throw XCTSkip("EXP-1065")
    }
}
