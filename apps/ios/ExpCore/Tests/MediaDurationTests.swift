import XCTest
@testable import ExpCore

/// EXP-824 — the duration chip format, byte-locked against web
/// `formatDuration` (video-metadata.test.ts) and the native twins.
final class MediaDurationTests: XCTestCase {
    func testFormatsMinutesSecondsAndHours() {
        XCTAssertEqual(MediaDuration.format(ms: 0), "0:00")
        XCTAssertEqual(MediaDuration.format(ms: 7_250), "0:07")
        XCTAssertEqual(MediaDuration.format(ms: 154_000), "2:34")
        XCTAssertEqual(MediaDuration.format(ms: 3_723_000), "1:02:03")
    }

    func testRoundsToTheNearestSecond() {
        XCTAssertEqual(MediaDuration.format(ms: 6_500), "0:07")
        XCTAssertEqual(MediaDuration.format(ms: 6_499), "0:06")
        XCTAssertEqual(MediaDuration.format(ms: 59_600), "1:00")
    }

    func testNegativeClampsToZero() {
        XCTAssertEqual(MediaDuration.format(ms: -5_000), "0:00")
    }
}
