import Foundation
import XCTest
@testable import ExpCore

// EXP-637: the agent closes out its own run (`exponential_sessions_end` stamps
// summary + outcome + ended_by), and every client shows the SAME words for
// what came back. These tests pin the labels (byte-equal with web's
// `sessionOutcomeLabel`, desktop's `ended_runs.rs` and Android's
// `domain/RunOutcome.kt`) and the gates on offering a Resume.
final class RunOutcomeTests: XCTestCase {

    private func session(
        id: String = "sess-1",
        userId: String = "user-1",
        status: String = "ended",
        deviceId: String? = "dev-1",
        outcome: String? = "done",
        endedBy: String? = "agent"
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: nil,
            teamId: "team-1",
            userId: userId,
            deviceLabel: "macbook",
            deviceId: deviceId,
            status: status,
            outcome: outcome,
            endedBy: endedBy,
            startedAt: "2026-08-27T09:00:00Z",
            endedAt: "2026-08-27T09:12:00Z",
            createdAt: "2026-08-27T09:00:00Z",
            updatedAt: "2026-08-27T09:12:00Z"
        )
    }

    private func device(
        id: String = "dev-1",
        caps: [String]? = ["resume-run"],
        online: Bool = true,
        owner: DeviceOwner? = nil
    ) -> SteerDevice {
        SteerDevice(
            deviceId: id, deviceLabel: "macbook", caps: caps, online: online, owner: owner
        )
    }

    // MARK: - Labels

    func testOutcomeLabelsAreTheSharedWords() {
        XCTAssertEqual(RunOutcomePresentation.label(DomainContract.codingSessionOutcomeDone), "Done")
        XCTAssertEqual(
            RunOutcomePresentation.label(DomainContract.codingSessionOutcomeBlocked), "Blocked"
        )
        XCTAssertEqual(
            RunOutcomePresentation.label(DomainContract.codingSessionOutcomeNoChanges), "No changes"
        )
    }

    func testAnEndWithoutAnOutcomeJustSaysEnded() {
        // Every path except the agent's own close-out (kill switch, tab close,
        // PR merge, sweep) leaves the column NULL.
        XCTAssertEqual(RunOutcomePresentation.label(nil), "Ended")
        // A value from a newer client must not render as a raw enum token.
        XCTAssertEqual(RunOutcomePresentation.label("something_new"), "Ended")
    }

    // MARK: - Resume gates

    func testResumeTargetsTheMachineThatRanIt() {
        let target = RunResume.target(
            for: session(), devices: [device(id: "other"), device()], currentUserId: "user-1"
        )
        XCTAssertEqual(target?.deviceId, "dev-1")
    }

    func testResumeNeedsOwnershipAndAnEndedRun() {
        XCTAssertNil(
            RunResume.target(for: session(), devices: [device()], currentUserId: "user-2")
        )
        XCTAssertNil(
            RunResume.target(for: session(), devices: [device()], currentUserId: nil)
        )
        // Still live: the server refuses it, so nothing may offer it.
        XCTAssertNil(
            RunResume.target(
                for: session(status: "running"), devices: [device()], currentUserId: "user-1"
            )
        )
    }

    func testResumeNeedsAnOnlineCapableMachine() {
        // An old desktop advertises no `resume-run` — it would start FRESH,
        // which reads as losing the run.
        XCTAssertNil(
            RunResume.target(
                for: session(), devices: [device(caps: [])], currentUserId: "user-1"
            )
        )
        XCTAssertNil(
            RunResume.target(
                for: session(), devices: [device(caps: nil)], currentUserId: "user-1"
            )
        )
        XCTAssertNil(
            RunResume.target(
                for: session(), devices: [device(online: false)], currentUserId: "user-1"
            )
        )
    }

    func testResumeNeedsAStampedMachine() {
        // A pre-EXP-549 row carries no device_id — nothing to route to.
        XCTAssertNil(
            RunResume.target(
                for: session(deviceId: nil), devices: [device()], currentUserId: "user-1"
            )
        )
        XCTAssertNil(
            RunResume.target(
                for: session(deviceId: ""), devices: [device()], currentUserId: "user-1"
            )
        )
        // No machine row for the stamped id at all.
        XCTAssertNil(
            RunResume.target(
                for: session(), devices: [device(id: "other")], currentUserId: "user-1"
            )
        )
    }

    func testResumePrefersOurOwnRowOverAShareOfTheSameMachine() {
        // A teammate's shared row carries `owner`; ours never does. Both can
        // name the same machine id, and the send belongs to ours.
        let shared = device(owner: DeviceOwner(id: "user-2", name: "Ada"))
        let target = RunResume.target(
            for: session(), devices: [shared, device()], currentUserId: "user-1"
        )
        XCTAssertNotNil(target)
        XCTAssertTrue(target?.isMine ?? false)
    }
}
