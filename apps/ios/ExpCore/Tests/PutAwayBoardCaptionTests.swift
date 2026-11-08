import Foundation
import XCTest
@testable import ExpCore

// #NEW-37: the Archived / Trash captions match web `formatArchivedOn` /
// `formatPurgeCountdown` and desktop `all_boards.rs`.
final class PutAwayBoardCaptionTests: XCTestCase {
    func testArchivedCaptionMatchesWeb() {
        XCTAssertEqual(PutAwayBoardCaption.archived("2026-07-01T10:00:00.000Z"), "Archived Jul 1, 2026")
        XCTAssertEqual(PutAwayBoardCaption.archived(nil), "Archived")
    }

    func testPurgeCountdownMatchesWeb() {
        let now = ISO8601DateFormatter().date(from: "2026-07-01T10:00:00Z")!
        XCTAssertEqual(PutAwayBoardCaption.purge("2026-07-03T03:30:00.000Z", now: now), "Purges in ~42h")
        XCTAssertEqual(PutAwayBoardCaption.purge("2026-07-01T09:00:00.000Z", now: now), "Purging soon")
        XCTAssertEqual(PutAwayBoardCaption.purge(nil, now: now), "Purges soon")
    }
}
