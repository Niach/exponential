import Foundation
import XCTest

@testable import ExpCore

// EXP-893: the phone Work screen's pure rules. Test names mirror the web spec
// `lib/work-faces.test.ts` and Android `WorkFacesTest.kt`.
final class WorkFacesTests: XCTestCase {

    private let now = WireTimestamps.parse("2026-09-15T12:00:00Z")!

    private func run(
        _ id: String,
        issueId: String? = "issue-1",
        userId: String = "me",
        status: String = "running",
        startedAt: String = "2026-09-15T11:00:00Z",
        updatedAt: String = "2026-09-15T11:59:00Z"
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: issueId,
            teamId: "team-1",
            userId: userId,
            deviceLabel: "macbook",
            deviceId: "dev-1",
            status: status,
            agent: "claude",
            startedAt: startedAt,
            endedAt: nil,
            createdAt: startedAt,
            updatedAt: updatedAt
        )
    }

    func testListsTheAvailableFacesInIssueRunChangesResultsOrder() {
        XCTAssertEqual(
            WorkFaces.availableFaces(
                hasIssue: true, hasRun: true, hasChanges: true, hasResults: true
            ),
            [.issue, .run, .changes, .results]
        )
        XCTAssertEqual(
            WorkFaces.availableFaces(
                hasIssue: false, hasRun: true, hasChanges: false, hasResults: false
            ),
            [.run]
        )
        // Changes is independent of Run: an open PR with no run of mine.
        XCTAssertEqual(
            WorkFaces.availableFaces(
                hasIssue: true, hasRun: false, hasChanges: true, hasResults: false
            ),
            [.issue, .changes]
        )
        // EXP-879: Results is the RUN's, and always last.
        XCTAssertEqual(
            WorkFaces.availableFaces(
                hasIssue: true, hasRun: true, hasChanges: false, hasResults: true
            ),
            [.issue, .run, .results]
        )
    }

    func testLabelsTheFacesRunsOnceThereAreSeveral() {
        XCTAssertEqual(WorkFaces.faceLabel(.issue), "Issue")
        XCTAssertEqual(WorkFaces.faceLabel(.run), "Run")
        XCTAssertEqual(WorkFaces.faceLabel(.run, multipleRuns: true), "Runs")
        XCTAssertEqual(WorkFaces.faceLabel(.changes), "Changes")
        XCTAssertEqual(WorkFaces.faceLabel(.results), "Results")
        XCTAssertEqual(WorkFaces.steerComposerPlaceholder, "Type / for commands")
        XCTAssertEqual(WorkFaces.planModeLabel, "Plan mode")
        XCTAssertEqual(WorkFaces.startCodingLabel, "Start coding")
        // The session composer reads the same words.
        XCTAssertEqual(AgentFeed.composerPlaceholder, WorkFaces.steerComposerPlaceholder)
        XCTAssertEqual(AgentFeed.planModeFooterLabel, WorkFaces.planModeLabel)
    }

    func testTargetsTheBoundRunWhenItIsMineAndLive() {
        let rows = [
            run("a", startedAt: "2026-09-15T10:00:00Z"),
            run("b", startedAt: "2026-09-15T11:00:00Z"),
        ]
        XCTAssertEqual(
            WorkFaces.codingTarget(rows, issueId: "issue-1", boundId: "a", me: "me", now: now)?.id,
            "a"
        )
    }

    func testTargetsTheNewestLiveOwnRunElseTheNewestOwnRun() {
        let rows = [
            run("old-live", startedAt: "2026-09-15T09:00:00Z"),
            run("new-live", startedAt: "2026-09-15T11:00:00Z"),
            run("newest-ended", status: "ended", startedAt: "2026-09-15T11:30:00Z"),
            run("theirs", userId: "them", startedAt: "2026-09-15T11:45:00Z"),
        ]
        XCTAssertEqual(
            WorkFaces.codingTarget(rows, issueId: "issue-1", boundId: nil, me: "me", now: now)?.id,
            "new-live"
        )
        // A bound run that ENDED does not win over a live one.
        XCTAssertEqual(
            WorkFaces.codingTarget(
                rows, issueId: "issue-1", boundId: "newest-ended", me: "me", now: now
            )?.id,
            "new-live"
        )
        let ended = rows.filter { $0.status == "ended" }
        XCTAssertEqual(
            WorkFaces.codingTarget(ended, issueId: "issue-1", boundId: nil, me: "me", now: now)?.id,
            "newest-ended"
        )
        XCTAssertNil(WorkFaces.codingTarget(rows, issueId: "issue-2", boundId: nil, me: "me", now: now))
        XCTAssertNil(WorkFaces.codingTarget(rows, issueId: "issue-1", boundId: nil, me: nil, now: now))
    }

    func testTreatsAStaleRunningRowAsNotLive() {
        let rows = [
            run("stale", startedAt: "2026-09-15T11:00:00Z", updatedAt: "2026-09-15T08:00:00Z"),
            run("ended", status: "ended", startedAt: "2026-09-15T10:00:00Z"),
        ]
        // No live run: the newest own run is the stale one — still the target.
        XCTAssertEqual(
            WorkFaces.codingTarget(rows, issueId: "issue-1", boundId: nil, me: "me", now: now)?.id,
            "stale"
        )
        XCTAssertEqual(
            WorkFaces.codingTarget(rows, issueId: "issue-1", boundId: "stale", me: "me", now: now)?.id,
            "stale"
        )
    }

    func testPicksThePrimaryActionStopFirst() {
        XCTAssertEqual(
            WorkFaces.primaryAction(ownLive: true, ownEndedResumable: true, canStart: true), .stop
        )
        XCTAssertEqual(
            WorkFaces.primaryAction(ownLive: false, ownEndedResumable: true, canStart: true), .resume
        )
        XCTAssertEqual(
            WorkFaces.primaryAction(ownLive: false, ownEndedResumable: false, canStart: true), .start
        )
        XCTAssertEqual(
            WorkFaces.primaryAction(ownLive: false, ownEndedResumable: false, canStart: false), .none
        )
    }

    func testOffersTheOtherFacesAsSwitcherTargets() {
        XCTAssertEqual(
            WorkFaces.switcherTargets(
                faces: [.issue, .run, .changes], shown: .run, runIds: ["a"], shownRunId: "a",
                offerStart: false
            ),
            [.face(.issue), .face(.changes)]
        )
        XCTAssertEqual(
            WorkFaces.switcherTargets(
                faces: [.issue], shown: .issue, runIds: [], shownRunId: nil, offerStart: false
            ),
            []
        )
    }

    func testExpandsTheRunFaceIntoOneRowPerRunWithTwoOrMore() {
        XCTAssertEqual(
            WorkFaces.switcherTargets(
                faces: [.issue, .run], shown: .issue, runIds: ["a", "b"], shownRunId: "a",
                offerStart: false
            ),
            [.run(id: "a"), .run(id: "b")]
        )
        // On the Run face the shown run is not a target.
        XCTAssertEqual(
            WorkFaces.switcherTargets(
                faces: [.issue, .run, .changes], shown: .run, runIds: ["a", "b"], shownRunId: "a",
                offerStart: false
            ),
            [.face(.issue), .run(id: "b"), .face(.changes)]
        )
    }

    func testPrependsStartCodingWhenTheShownRunEndedForGood() {
        XCTAssertEqual(
            WorkFaces.switcherTargets(
                faces: [.issue, .run], shown: .run, runIds: ["a"], shownRunId: "a", offerStart: true
            ),
            [.startCoding, .face(.issue)]
        )
    }

    func testHidesTogglesOrOpensAMenuByTargetCount() {
        XCTAssertEqual(WorkFaces.switcherMode([]), .hidden)
        XCTAssertEqual(WorkFaces.switcherMode([.face(.run)]), .toggle(.face(.run)))
        guard case .menu = WorkFaces.switcherMode([.face(.issue), .face(.changes)]) else {
            return XCTFail("two targets open a menu")
        }
    }

    func testBadgesTheCircleWithTheSessionToneOffTheRunFace() {
        XCTAssertEqual(
            WorkFaces.switcherBadge(shown: .issue, sessionTone: .running, hasChanges: true),
            .tone(.running)
        )
        XCTAssertEqual(
            WorkFaces.switcherBadge(shown: .changes, sessionTone: .needsInput, hasChanges: false),
            .tone(.needsInput)
        )
        XCTAssertNil(WorkFaces.switcherBadge(shown: .issue, sessionTone: nil, hasChanges: true))
    }

    func testBadgesTheCircleWithChangesOnTheRunFace() {
        XCTAssertEqual(
            WorkFaces.switcherBadge(shown: .run, sessionTone: .running, hasChanges: true), .changes
        )
        XCTAssertNil(WorkFaces.switcherBadge(shown: .run, sessionTone: .running, hasChanges: false))
    }

    func testFallsBackChangesToRunToIssue() {
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .changes, available: [.issue, .run]), .run)
        // EXP-879: Results falls back the same way — both are the run's.
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .results, available: [.issue, .run]), .run)
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .results, available: [.issue]), .issue)
        XCTAssertNil(WorkFaces.fallbackFace(shown: .results, available: []))
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .changes, available: [.issue]), .issue)
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .run, available: [.issue]), .issue)
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .run, available: [.issue, .run]), .run)
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .issue, available: [.run]), .run)
        XCTAssertNil(WorkFaces.fallbackFace(shown: .run, available: []))
    }

    func testReadsTheSessionModelOffTheConfigOption() {
        XCTAssertNil(WorkFaces.sessionModel(nil))
        XCTAssertNil(WorkFaces.sessionModel(AgentSessionConfig(options: [])))
        XCTAssertNil(WorkFaces.sessionModel(AgentSessionConfig(options: [
            AgentConfigOption(id: "model", label: "Model", value: ""),
        ])))
        XCTAssertEqual(
            WorkFaces.sessionModel(AgentSessionConfig(options: [
                AgentConfigOption(id: "model", label: "Model", value: "opus"),
            ])),
            "opus"
        )
    }

    func testTonesTheStateDotOffThePhase() {
        let base = WorkFaces.phaseDotTone(
            live: true, connecting: false, awaitingInput: false, paused: false, stale: false
        )
        XCTAssertEqual(base.tone, .running)
        XCTAssertFalse(base.connecting)
        XCTAssertEqual(
            WorkFaces.phaseDotTone(
                live: true, connecting: false, awaitingInput: true, paused: false, stale: false
            ).tone,
            .needsInput
        )
        XCTAssertEqual(
            WorkFaces.phaseDotTone(
                live: true, connecting: false, awaitingInput: false, paused: false, stale: true
            ).tone,
            .needsInput
        )
        XCTAssertEqual(
            WorkFaces.phaseDotTone(
                live: true, connecting: false, awaitingInput: false, paused: true, stale: false
            ).tone,
            .muted
        )
        XCTAssertEqual(
            WorkFaces.phaseDotTone(
                live: false, connecting: false, awaitingInput: false, paused: false, stale: false
            ).tone,
            .muted
        )
        XCTAssertTrue(
            WorkFaces.phaseDotTone(
                live: false, connecting: true, awaitingInput: false, paused: false, stale: false
            ).connecting
        )
        XCTAssertFalse(
            WorkFaces.phaseDotTone(
                live: false, connecting: true, awaitingInput: false, paused: true, stale: false
            ).connecting
        )
    }
}
