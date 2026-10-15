import XCTest
@testable import ExpCore

/// EXP-1316: every question card offers the free-text answer as option N+1
/// (Android `FreeTextRowTest`).
final class FreeTextRowTests: XCTestCase {
    private func question(_ options: [AgentQuestionOption], planMode: Bool = false, askId: String? = nil, index: Int? = 1) -> AgentQuestion {
        AgentQuestion(id: 1, wireId: "q1", askId: askId, index: index, text: "Which?", options: options, planMode: planMode)
    }

    func testAQuestionWithoutOneGainsTheRowKeyedByItsPosition() {
        let item = question([.init(label: "A", key: "1"), .init(label: "B", key: "2")]).withFreeTextRow()
        XCTAssertEqual(item.options.map(\.key), ["1", "2", "3"])
        XCTAssertEqual(item.options.last?.label, "Type something.")
        XCTAssertEqual(item.options.last?.freeText, true)
    }

    func testATakenPositionKeyFallsBackToFreeText() {
        let item = question([.init(label: "A", key: "2"), .init(label: "B", key: "3")]).withFreeTextRow()
        XCTAssertEqual(item.options.last?.key, "free-text")
    }

    func testOwnRowPlansAndSubmitStepsAreLeftAlone() {
        let own = question([.init(label: "A", key: "1"), .init(label: "Type something.", key: "2", freeText: true)])
        XCTAssertEqual(own.withFreeTextRow(), own)
        let plan = question([.init(label: "Yes", key: "1")], planMode: true)
        XCTAssertEqual(plan.withFreeTextRow(), plan)
        let submit = question([.init(label: "Submit", key: "1")], askId: "a", index: nil)
        XCTAssertEqual(submit.withFreeTextRow(), submit)
    }
}
