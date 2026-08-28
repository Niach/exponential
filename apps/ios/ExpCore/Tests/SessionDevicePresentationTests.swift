import Foundation
import XCTest
@testable import ExpCore

// EXP-549/550: a session's host machine resolves against the LIVE devices row
// (renames land; a silent machine reads paused), never the start-time
// `device_label` snapshot alone.
final class SessionDevicePresentationTests: XCTestCase {
    private let now = WireTimestamps.parse("2026-08-19T10:00:00.000Z")!
    // Inside the 90s contract window vs. well outside it.
    private let fresh = "2026-08-19T09:59:50Z"
    private let stale = "2026-08-19T09:50:00Z"

    private func session(
        deviceId: String?,
        deviceLabel: String?,
        userId: String = "me",
        status: String = "running",
        needsInput: Bool = false
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: "sess-1",
            issueId: "issue-1",
            boardId: nil,
            teamId: "team-1",
            userId: userId,
            deviceLabel: deviceLabel,
            deviceId: deviceId,
            status: status,
            needsInput: needsInput,
            startedAt: "2026-08-19T09:00:00Z",
            endedAt: nil,
            createdAt: "2026-08-19T09:00:00Z",
            updatedAt: "2026-08-19T09:30:00Z"
        )
    }

    private func device(
        id: String,
        userId: String = "me",
        deviceId: String,
        label: String,
        lastSeenAt: String?
    ) -> DeviceEntity {
        DeviceEntity(
            id: id, userId: userId, deviceId: deviceId, label: label,
            kind: "desktop", lastSeenAt: lastSeenAt
        )
    }

    // EXP-549: the rename lives on the devices row — the session's snapshot is
    // the stale hostname and must lose.
    func testLiveRowLabelBeatsSnapshot() {
        let rows = [
            device(id: "r1", deviceId: "dev-1", label: "macbook", lastSeenAt: fresh)
        ]
        let resolved = SessionDevicePresentation.resolve(
            session: session(deviceId: "dev-1", deviceLabel: "MacBook-Pro-von-Danny.local"),
            devices: rows,
            now: now
        )
        XCTAssertEqual(resolved.label, "macbook")
        XCTAssertEqual(resolved.displayLabel, "macbook")
        XCTAssertFalse(resolved.offline)
    }

    // The id is the only join: another row whose label happens to equal the
    // snapshot never interferes.
    func testMatchByIdIgnoresLabelLookalike() {
        let rows = [
            device(id: "r1", deviceId: "dev-1", label: "macbook", lastSeenAt: fresh),
            device(id: "r2", deviceId: "dev-2", label: "old-name", lastSeenAt: stale),
        ]
        let resolved = SessionDevicePresentation.resolve(
            session: session(deviceId: "dev-1", deviceLabel: "old-name"),
            devices: rows,
            now: now
        )
        XCTAssertEqual(resolved.label, "macbook")
        XCTAssertFalse(resolved.offline)
    }

    // Two users can see the same machine id (a shared server row); the
    // session owner's own row is the one that describes THIS run.
    func testOwnRowPreferredAmongSameDeviceId() {
        let rows = [
            device(id: "r1", userId: "mate", deviceId: "dev-1",
                   label: "mate-view", lastSeenAt: fresh),
            device(id: "r2", userId: "me", deviceId: "dev-1",
                   label: "my-view", lastSeenAt: fresh),
        ]
        let resolved = SessionDevicePresentation.resolve(
            session: session(deviceId: "dev-1", deviceLabel: nil),
            devices: rows,
            now: now
        )
        XCTAssertEqual(resolved.label, "my-view")
    }

    // A stamped-but-unknown device id does NOT fall back to the label — the row
    // it names simply hasn't synced, which is not evidence the machine is
    // offline.
    func testUnknownDeviceIdKeepsSnapshotAndStaysOnline() {
        let rows = [
            device(id: "r1", deviceId: "dev-1", label: "macbook", lastSeenAt: stale)
        ]
        let stamped = SessionDevicePresentation.resolve(
            session: session(deviceId: "dev-gone", deviceLabel: "macbook"),
            devices: rows,
            now: now
        )
        XCTAssertEqual(stamped.label, "macbook")
        XCTAssertFalse(stamped.offline)
    }

    // EXP-560: no stamped device_id resolves NO row, even when exactly one
    // row's label equals the snapshot. The snapshot is still what we print, and
    // an unmatched machine is never reported offline.
    func testMissingDeviceIdMatchesNoRowDespiteUniqueLabel() {
        let rows = [
            device(id: "r1", deviceId: "dev-1", label: "macbook", lastSeenAt: stale)
        ]
        let resolved = SessionDevicePresentation.resolve(
            session: session(deviceId: nil, deviceLabel: "macbook"),
            devices: rows,
            now: now
        )
        XCTAssertEqual(resolved.label, "macbook")
        XCTAssertFalse(resolved.offline)
    }

    // No row at all (and no snapshot): the generic byline fallback.
    func testNoMatchFallsBackToSnapshotThenDesktop() {
        let none = SessionDevicePresentation.resolve(
            session: session(deviceId: "dev-1", deviceLabel: nil),
            devices: [],
            now: now
        )
        XCTAssertNil(none.label)
        XCTAssertEqual(none.displayLabel, "Desktop")
        XCTAssertFalse(none.offline)
    }

    // EXP-550: a stale heartbeat pauses a still-coding run — and only that.
    func testOfflinePausesRunningAndNeedsInputOnly() {
        let rows = [
            device(id: "r1", deviceId: "dev-1", label: "macbook", lastSeenAt: stale)
        ]
        let offline = SessionDevicePresentation.resolve(
            session: session(deviceId: "dev-1", deviceLabel: "macbook"),
            devices: rows,
            now: now
        )
        XCTAssertTrue(offline.offline)
        XCTAssertTrue(offline.isPaused(.running))
        XCTAssertTrue(offline.isPaused(.needsInput))
        // The PR is out — the machine's presence stopped mattering.
        XCTAssertFalse(offline.isPaused(.review))
        XCTAssertFalse(offline.isPaused(.done))
    }

    func testOnlineNeverPauses() {
        let rows = [
            device(id: "r1", deviceId: "dev-1", label: "macbook", lastSeenAt: fresh)
        ]
        let online = SessionDevicePresentation.resolve(
            session: session(deviceId: "dev-1", deviceLabel: "macbook"),
            devices: rows,
            now: now
        )
        XCTAssertFalse(online.offline)
        XCTAssertFalse(online.isPaused(.running))
        XCTAssertFalse(online.isPaused(.needsInput))
    }

    // A missing last_seen_at fails CLOSED in DeviceLiveness — a machine that
    // never reported is offline, and the session reads paused.
    func testMissingHeartbeatReadsOffline() {
        let rows = [
            device(id: "r1", deviceId: "dev-1", label: "macbook", lastSeenAt: nil)
        ]
        let resolved = SessionDevicePresentation.resolve(
            session: session(deviceId: "dev-1", deviceLabel: "macbook"),
            devices: rows,
            now: now
        )
        XCTAssertTrue(resolved.offline)
    }

    // MARK: - Freshness-unknown guard (EXP-656)

    // The report's exact shape: the phone slept, the devices cursor is older
    // than the contract window, and the row's last_seen_at is pre-sleep. That
    // is ignorance, not evidence — never "Paused".
    func testAnUnrefreshedCursorNeverPauses() {
        let rows = [
            device(id: "r1", deviceId: "dev-1", label: "macbook", lastSeenAt: stale)
        ]
        let resolved = SessionDevicePresentation.resolve(
            session: session(deviceId: "dev-1", deviceLabel: "macbook"),
            devices: rows,
            now: now,
            devicesFresh: false
        )
        XCTAssertNil(resolved.online)
        XCTAssertFalse(resolved.offline)
        XCTAssertFalse(resolved.isPaused(.running))
        XCTAssertEqual(resolved.label, "macbook")
    }

    // A stale cursor can only produce a false OFFLINE (last_seen_at only moves
    // forward), so an online verdict stands whatever our cursor says.
    func testAFreshHeartbeatStaysOnlineWithAStaleCursor() {
        let rows = [
            device(id: "r1", deviceId: "dev-1", label: "macbook", lastSeenAt: fresh)
        ]
        let resolved = SessionDevicePresentation.resolve(
            session: session(deviceId: "dev-1", deviceLabel: "macbook"),
            devices: rows,
            now: now,
            devicesFresh: false
        )
        XCTAssertEqual(resolved.online, true)
        XCTAssertFalse(resolved.offline)
    }

    // EXP-550 guard: knowledge still pauses. A cursor inside the window plus a
    // stale heartbeat means the machine really did go away.
    func testAFreshCursorStillPausesAStaleHeartbeat() {
        let rows = [
            device(id: "r1", deviceId: "dev-1", label: "macbook", lastSeenAt: stale)
        ]
        let resolved = SessionDevicePresentation.resolve(
            session: session(deviceId: "dev-1", deviceLabel: "macbook"),
            devices: rows,
            now: now,
            devicesFresh: true
        )
        XCTAssertEqual(resolved.online, false)
        XCTAssertTrue(resolved.offline)
        XCTAssertTrue(resolved.isPaused(.running))
    }

    // No matched row is unknown too — an absent machine is not an offline one.
    func testAnUnmatchedRowResolvesUnknown() {
        let resolved = SessionDevicePresentation.resolve(
            session: session(deviceId: "dev-gone", deviceLabel: "macbook"),
            devices: [],
            now: now
        )
        XCTAssertNil(resolved.online)
        XCTAssertFalse(resolved.offline)
    }
}
