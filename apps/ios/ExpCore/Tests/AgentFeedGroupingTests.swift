import XCTest

@testable import ExpCore

// EXP-97: consecutive runs of >=2 tool calls collapse into one render row.
// Mirrors the web agent-feed.test.ts / Android AgentFeedTest grouping cases.
final class AgentFeedGroupingTests: XCTestCase {
    func testCollapsesRunsOfTwoOrMoreConsecutiveTools() {
        // narration, tool, tool, tool, user_message, tool
        let isTool = [false, true, true, true, false, true]
        XCTAssertEqual(AgentFeedGrouping.toolRunRanges(isTool: isTool), [1..<4])
    }

    func testLoneToolsStaySingleRows() {
        let isTool = [true, false, true]
        XCTAssertEqual(AgentFeedGrouping.toolRunRanges(isTool: isTool), [])
    }

    func testTwoRunsSplitByANarrationStaySeparate() {
        let isTool = [true, true, false, true, true]
        XCTAssertEqual(AgentFeedGrouping.toolRunRanges(isTool: isTool), [0..<2, 3..<5])
    }

    func testAllToolFeedIsOneRunAndEmptyFeedHasNone() {
        XCTAssertEqual(AgentFeedGrouping.toolRunRanges(isTool: [true, true, true]), [0..<3])
        XCTAssertEqual(AgentFeedGrouping.toolRunRanges(isTool: []), [])
    }

    func testRunStartStaysStableAsTheTrailingRunGrows() {
        let before = AgentFeedGrouping.toolRunRanges(isTool: [false, true, true])
        let after = AgentFeedGrouping.toolRunRanges(isTool: [false, true, true, true])
        XCTAssertEqual(before, [1..<3])
        XCTAssertEqual(after, [1..<4])
    }

    func testTrailingNonToolItemsAreNeverAbsorbed() {
        // tool, tool, question, question
        let isTool = [true, true, false, false]
        XCTAssertEqual(AgentFeedGrouping.toolRunRanges(isTool: isTool), [0..<2])
    }
}
