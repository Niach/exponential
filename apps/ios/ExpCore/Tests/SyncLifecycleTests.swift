import Foundation
import XCTest

@testable import ExpCore

// EXP-656: the park/resume rule behind SyncManager's scene hooks. Background
// cancels immediately (no grace window — a monotonic timer never fires on a
// sleeping phone), foreground always relaunches a parked manager, and a
// network edge stays rate-limited while never fighting the resume.
final class SyncLifecycleTests: XCTestCase {
    private let t0 = Date(timeIntervalSince1970: 1_770_000_000)
    private let floor: TimeInterval = 5

    func testBackgroundParks() {
        let outcome = SyncLifecycleState().onBackground()
        XCTAssertEqual(outcome.action, .park)
        XCTAssertTrue(outcome.state.parked)
    }

    func testForegroundRelaunchesHoweverShortTheTripWas() {
        let parked = SyncLifecycleState().onBackground().state
        // One second in the background still means 19 cancelled pipelines.
        let resumed = parked.onForeground(now: t0.addingTimeInterval(1))
        XCTAssertEqual(resumed.action, .relaunchAll)
        XCTAssertFalse(resumed.state.parked)
    }

    // A cold launch (or an `.inactive`-only flip) never parked, so its
    // pipelines are already running and a restart would throw away the
    // snapshot that is landing.
    func testForegroundWithoutABackgroundDoesNothing() {
        let outcome = SyncLifecycleState().onForeground(now: t0)
        XCTAssertEqual(outcome.action, .ignore)
        XCTAssertFalse(outcome.state.parked)
    }

    // The floor exists to collapse duplicate restarts, never to delay the one
    // the user is waiting for.
    func testForegroundBypassesTheRestartFloor() {
        let recentlyRestarted = SyncLifecycleState(parked: false, lastRestartAllAt: t0)
        let parked = recentlyRestarted.onBackground().state
        let resumed = parked.onForeground(now: t0.addingTimeInterval(1))
        XCTAssertEqual(resumed.action, .relaunchAll)
        // ...and it stamps, so a co-firing path change coalesces into it.
        let edge = resumed.state.onNetworkEdge(now: t0.addingTimeInterval(2), floor: floor)
        XCTAssertEqual(edge.action, .ignore)
    }

    func testNetworkEdgeIsIgnoredWhileParked() {
        let parked = SyncLifecycleState().onBackground().state
        let edge = parked.onNetworkEdge(now: t0.addingTimeInterval(60), floor: floor)
        XCTAssertEqual(edge.action, .ignore)
        XCTAssertTrue(edge.state.parked)
    }

    func testNetworkEdgeRespectsTheFloorWhileActive() {
        let first = SyncLifecycleState().onNetworkEdge(now: t0, floor: floor)
        XCTAssertEqual(first.action, .relaunchAll)
        let tooSoon = first.state.onNetworkEdge(now: t0.addingTimeInterval(4), floor: floor)
        XCTAssertEqual(tooSoon.action, .ignore)
        let later = first.state.onNetworkEdge(now: t0.addingTimeInterval(5), floor: floor)
        XCTAssertEqual(later.action, .relaunchAll)
    }
}
