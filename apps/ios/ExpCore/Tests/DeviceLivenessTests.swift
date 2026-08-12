import Foundation
import XCTest
@testable import ExpCore

// EXP-481: online-ness derives client-side from last_seen_at freshness
// against the contract window. FAIL-CLOSED, unlike session liveness — an
// unparseable stamp claims an unstartable machine is startable, so it reads
// offline instead.
final class DeviceLivenessTests: XCTestCase {
    private let now = WireTimestamps.parse("2026-08-11T10:00:00.000Z")!

    func testFreshStampReadsOnline() {
        XCTAssertTrue(DeviceLiveness.isOnline(lastSeenAt: "2026-08-11T09:59:31.000Z", now: now))
    }

    func testBoundaryIsExclusive() {
        // Exactly the 90s window: age * 1000 == deviceOnlineWindowMs → offline.
        XCTAssertFalse(DeviceLiveness.isOnline(lastSeenAt: "2026-08-11T09:58:30.000Z", now: now))
        XCTAssertTrue(DeviceLiveness.isOnline(lastSeenAt: "2026-08-11T09:58:30.001Z", now: now))
    }

    func testFutureStampClampsOnline() {
        // Client clock behind the server stamp — never punish skew.
        XCTAssertTrue(DeviceLiveness.isOnline(lastSeenAt: "2026-08-11T10:00:05.000Z", now: now))
    }

    func testUnparseableOrAbsentReadsOffline() {
        XCTAssertFalse(DeviceLiveness.isOnline(lastSeenAt: "not-a-date", now: now))
        XCTAssertFalse(DeviceLiveness.isOnline(lastSeenAt: nil, now: now))
        XCTAssertFalse(DeviceLiveness.isOnline(lastSeenAt: "", now: now))
    }

    func testPostgresTextFormParses() {
        // Electric delivers Postgres text timestamps (space separator).
        XCTAssertTrue(DeviceLiveness.isOnline(lastSeenAt: "2026-08-11 09:59:45+00", now: now))
    }
}
