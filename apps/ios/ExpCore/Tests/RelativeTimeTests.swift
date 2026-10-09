import XCTest
@testable import ExpCore

final class RelativeTimeTests: XCTestCase {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    func testCompactRoundsLikeWebAndDesktop() {
        XCTAssertEqual(RelativeTime.compact(now.addingTimeInterval(-20), now: now), "just now")
        XCTAssertEqual(RelativeTime.compact(now.addingTimeInterval(-120), now: now), "2m")
        XCTAssertEqual(RelativeTime.compact(now.addingTimeInterval(-3 * 3600), now: now), "3h")
        XCTAssertEqual(RelativeTime.compact(now.addingTimeInterval(-26 * 3600), now: now), "1d")
        XCTAssertEqual(RelativeTime.compact(now.addingTimeInterval(-50 * 3600), now: now), "2d")
        XCTAssertEqual(RelativeTime.compact(now.addingTimeInterval(3600), now: now), "just now")
    }

    func testLongIsTheSpelledOutForm() {
        XCTAssertEqual(RelativeTime.long(now.addingTimeInterval(-5 * 60), now: now), "5 minutes ago")
        XCTAssertEqual(RelativeTime.long(now.addingTimeInterval(-2 * 3600), now: now), "2 hours ago")
        XCTAssertEqual(RelativeTime.long(now.addingTimeInterval(-2 * 86400), now: now), "2 days ago")
    }

    func testWireFormParses() {
        XCTAssertEqual(RelativeTime.compact(wire: "garbage", now: now), "")
        XCTAssertFalse(RelativeTime.compact(wire: "2026-07-03 10:11:12.345+00").isEmpty)
    }
}
