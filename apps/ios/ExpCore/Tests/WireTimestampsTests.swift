import Foundation
import XCTest
@testable import ExpCore

// EXP-169/153: synced rows carry created_at/updatedAt in TWO wire forms —
// Electric's Postgres text (`2026-07-01 10:00:00.123456+00`, space separator,
// hour-only offset) and ISO-8601 (`2026-07-01T10:00:00Z`) from tRPC writes.
// WireTimestamps.parse must accept both, or every synced row's relative time
// blanks and the coding-session staleness guard stays fail-open.
final class WireTimestampsTests: XCTestCase {
    // Shared reference instant: 2026-07-01 10:00:00 UTC, via the ISO form.
    private let utcTen = WireTimestamps.parse("2026-07-01T10:00:00Z")!

    func testPostgresTextHourOnlyOffset() {
        XCTAssertEqual(WireTimestamps.parse("2026-07-01 10:00:00+00"), utcTen)
    }

    func testPostgresTextWithMicroseconds() {
        // Fraction truncates to milliseconds (.123456 → .123).
        let date = WireTimestamps.parse("2026-07-01 10:00:00.123456+00")
        XCTAssertNotNil(date)
        XCTAssertEqual(date!.timeIntervalSince(utcTen), 0.123, accuracy: 0.0005)
    }

    func testPostgresTextWithShortFraction() {
        // Single-digit fraction pads to milliseconds (.5 → .500).
        let date = WireTimestamps.parse("2026-07-01 10:00:00.5+00")
        XCTAssertNotNil(date)
        XCTAssertEqual(date!.timeIntervalSince(utcTen), 0.5, accuracy: 0.0005)
    }

    func testPostgresTextWithColonOffset() {
        // +05:30 is 5.5h ahead, so the instant is 5.5h before 10:00 UTC.
        let date = WireTimestamps.parse("2026-07-01 10:00:00+05:30")
        XCTAssertNotNil(date)
        XCTAssertEqual(date!.timeIntervalSince(utcTen), -19_800, accuracy: 0.5)
    }

    func testPostgresTextOffsetlessIsUTC() {
        XCTAssertEqual(WireTimestamps.parse("2026-07-01 10:00:00"), utcTen)
    }

    func testIsoPlain() {
        XCTAssertEqual(WireTimestamps.parse("2026-07-01T10:00:00Z"), utcTen)
    }

    func testIsoFractional() {
        let date = WireTimestamps.parse("2026-07-01T10:00:00.123Z")
        XCTAssertNotNil(date)
        XCTAssertEqual(date!.timeIntervalSince(utcTen), 0.123, accuracy: 0.0005)
    }

    func testCrossFormatEquality() {
        // The Postgres text and ISO encodings of the same instant must parse
        // to the identical Date.
        XCTAssertEqual(
            WireTimestamps.parse("2026-07-01 10:00:00+00"),
            WireTimestamps.parse("2026-07-01T10:00:00Z")
        )
    }

    func testGarbageIsNil() {
        XCTAssertNil(WireTimestamps.parse("not-a-timestamp"))
        XCTAssertNil(WireTimestamps.parse(""))
        XCTAssertNil(WireTimestamps.parse("   "))
    }
}
