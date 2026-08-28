import Foundation
import XCTest

@testable import ExpCore

// EXP-656: the rule that keeps a join replay from moving the reader. A reset
// opens a staging window instead of clearing the feed; the replay commits as
// one swap when the relay's marker (or a keepalive, or the timers) says the
// burst is over.
final class SteerReplayStagingTests: XCTestCase {
    private let t0 = Date(timeIntervalSince1970: 1_770_000_000)
    private let quiet: TimeInterval = 0.4
    private let max: TimeInterval = 3

    // MARK: - Frame parsing

    func testOnlyReplayFramesAreOwned() {
        XCTAssertEqual(SteerReplayStaging.Frame(wire: "activity"), .activity)
        XCTAssertEqual(SteerReplayStaging.Frame(wire: "activity_reset"), .activityReset)
        XCTAssertEqual(SteerReplayStaging.Frame(wire: "activity_synced"), .activitySynced)
        XCTAssertEqual(SteerReplayStaging.Frame(wire: "keepalive"), .keepalive)
        // Everything else belongs to the model's own switch.
        for wire in ["bye", "error", "input", "presence", "made_up"] {
            XCTAssertNil(SteerReplayStaging.Frame(wire: wire), wire)
        }
    }

    // MARK: - decide

    func testResetOpensStaging() {
        XCTAssertEqual(
            SteerReplayStaging.decide(staging: false, frame: .activityReset), .beginStaging
        )
    }

    // A publisher restarting its stream mid-replay supersedes what we buffered
    // — restart the window, never commit the half we have.
    func testASecondResetRestartsStaging() {
        XCTAssertEqual(
            SteerReplayStaging.decide(staging: true, frame: .activityReset), .beginStaging
        )
    }

    func testActivityStagesWhileStagingAndAppliesOtherwise() {
        XCTAssertEqual(SteerReplayStaging.decide(staging: true, frame: .activity), .stage)
        XCTAssertEqual(SteerReplayStaging.decide(staging: false, frame: .activity), .apply)
    }

    func testTheMarkerCommitsOnlyWhileStaging() {
        XCTAssertEqual(SteerReplayStaging.decide(staging: true, frame: .activitySynced), .commit)
        // An `activity_synced` outside a replay (a relay we joined before the
        // window opened) has nothing to commit — never a feed change.
        XCTAssertEqual(SteerReplayStaging.decide(staging: false, frame: .activitySynced), .ignore)
    }

    // The relay's own 15s beat: if it got a turn, the replay burst is over.
    // This is what ends a publisher-driven republish from an old relay.
    func testAKeepaliveEndsAStagedReplay() {
        XCTAssertEqual(SteerReplayStaging.decide(staging: true, frame: .keepalive), .commit)
        XCTAssertEqual(SteerReplayStaging.decide(staging: false, frame: .keepalive), .ignore)
    }

    // MARK: - shouldCommit

    func testQuietWindowCommits() {
        let started = t0
        let lastFrame = t0.addingTimeInterval(0.2)
        XCTAssertFalse(SteerReplayStaging.shouldCommit(
            now: lastFrame.addingTimeInterval(0.3), lastFrameAt: lastFrame,
            startedAt: started, quiet: quiet, max: max
        ))
        XCTAssertTrue(SteerReplayStaging.shouldCommit(
            now: lastFrame.addingTimeInterval(0.5), lastFrameAt: lastFrame,
            startedAt: started, quiet: quiet, max: max
        ))
    }

    // A stream that never goes quiet still commits at the cap — a stalled
    // republish must not hold the buffer forever.
    func testHardCapCommitsANeverQuietStream() {
        let now = t0.addingTimeInterval(3.1)
        XCTAssertTrue(SteerReplayStaging.shouldCommit(
            now: now, lastFrameAt: now, startedAt: t0, quiet: quiet, max: max
        ))
        XCTAssertFalse(SteerReplayStaging.shouldCommit(
            now: t0.addingTimeInterval(2.9), lastFrameAt: t0.addingTimeInterval(2.9),
            startedAt: t0, quiet: quiet, max: max
        ))
    }
}
