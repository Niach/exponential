import Foundation
import XCTest
@testable import ExpCore

// EXP-153: the client-side staleness guard must mirror the server sweep's
// rule — a `running` row is live only while its heartbeat (updatedAt) is
// inside the contract stale window; stale rows render as absent.
final class CodingSessionLivenessTests: XCTestCase {
    // 2026-07-17T12:00:00Z
    private let now = Date(timeIntervalSince1970: 1_784_289_600)

    private func session(status: String, updatedAt: String) -> CodingSessionEntity {
        CodingSessionEntity(
            id: "sess-1",
            issueId: "issue-1",
            projectId: nil,
            workspaceId: "ws-1",
            userId: "user-1",
            deviceLabel: nil,
            status: status,
            startedAt: "2026-07-17T09:00:00Z",
            endedAt: nil,
            createdAt: "2026-07-17T09:00:00Z",
            updatedAt: updatedAt
        )
    }

    func testLiveWithinStaleWindow() {
        // Heartbeat 30 minutes ago — well inside the 2h window.
        XCTAssertTrue(
            CodingSessionLiveness.isLive(session(status: "running", updatedAt: "2026-07-17T11:30:00Z"), now: now)
        )
    }

    func testStalePastWindow() {
        // Last heartbeat 3h ago — past the window; renders as absent.
        XCTAssertFalse(
            CodingSessionLiveness.isLive(session(status: "running", updatedAt: "2026-07-17T09:00:00Z"), now: now)
        )
    }

    func testNonRunningIsNeverLive() {
        XCTAssertFalse(
            CodingSessionLiveness.isLive(session(status: "ended", updatedAt: "2026-07-17T11:59:00Z"), now: now)
        )
    }

    func testUnparseableTimestampFailsOpen() {
        // A garbled liveness signal must never hide a session the server
        // still considers alive — the sweep is the backstop.
        XCTAssertNil(CodingSessionLiveness.parseIso("not-a-timestamp"))
        XCTAssertTrue(
            CodingSessionLiveness.isLive(session(status: "running", updatedAt: "not-a-timestamp"), now: now)
        )
    }

    func testParsesBothIsoPrecisions() {
        // The two forms the sync layer stores: plain seconds and fractional.
        XCTAssertFalse(CodingSessionLiveness.isStale(updatedAt: "2026-07-17T11:30:00Z", now: now))
        XCTAssertFalse(CodingSessionLiveness.isStale(updatedAt: "2026-07-17T11:30:00.123Z", now: now))
        XCTAssertTrue(CodingSessionLiveness.isStale(updatedAt: "2026-07-17T09:00:00.500Z", now: now))
    }
}
