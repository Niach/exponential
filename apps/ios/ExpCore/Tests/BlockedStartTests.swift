import XCTest
@testable import ExpCore

// EXP-897/EXP-980: the blocked-start prompt's copy is byte-identical ×4.
final class BlockedStartTests: XCTestCase {
    func testTheCopyIsPinned() {
        XCTAssertEqual(BlockedStart.blockedStartTitle, "This issue is blocked")
        XCTAssertEqual(BlockedStart.blockedBatchTitle, "Some of these issues are blocked")
        XCTAssertEqual(BlockedStart.startAnywayLabel, "Start anyway")
        XCTAssertEqual(BlockedStart.bodyPrefix, "This issue is blocked by ")
        XCTAssertEqual(BlockedStart.bodySuffix, ". Start anyway?")
        XCTAssertEqual(
            BlockedStart.blockedBatchBody,
            "Open issues outside this batch block it. Start anyway?"
        )
    }
}
