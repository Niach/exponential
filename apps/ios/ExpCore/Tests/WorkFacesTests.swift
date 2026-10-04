import Foundation
import XCTest

@testable import ExpCore

// EXP-893: the phone Work screen's pure rules — EXP-1150: the face tabs and
// the body swipe (`swipeTarget`). Test names mirror the web spec
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
        XCTAssertEqual(WorkFaces.openResultsLabel, "Open Results")
        XCTAssertEqual(WorkFaces.steerComposerPlaceholder, "Type / for commands")
        XCTAssertEqual(WorkFaces.planModeLabel, "Plan mode")
        XCTAssertEqual(WorkFaces.startCodingLabel, "Start coding")
        // The session composer reads the same words.
        XCTAssertEqual(AgentFeed.composerPlaceholder, WorkFaces.steerComposerPlaceholder)
        XCTAssertEqual(AgentFeed.planModeFooterLabel, WorkFaces.planModeLabel)
    }

    func testLabelsChangesWithItsCountsOnceTheFilesAreKnown() {
        XCTAssertNil(WorkFaces.changesFaceCounts(nil))
        XCTAssertNil(WorkFaces.changesFaceCounts(Diff.Totals(files: 0, additions: 0, deletions: 0)))
        XCTAssertEqual(
            WorkFaces.changesFaceCounts(Diff.Totals(files: 3, additions: 12, deletions: 2)),
            WorkFaces.ChangesFaceCounts(additions: 12, deletions: 2)
        )
        // U+2212 MINUS SIGN, never a hyphen — the contract's `deletionsLabel`.
        XCTAssertEqual(
            WorkFaces.changesFaceText(WorkFaces.ChangesFaceCounts(additions: 12, deletions: 2)),
            "+12 \u{2212}2"
        )
        XCTAssertEqual(
            WorkFaces.changesFaceText(WorkFaces.ChangesFaceCounts(additions: 0, deletions: 0)),
            "+0 \u{2212}0"
        )
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

    func testSwipesToTheNeighbouringFace() {
        let all: [WorkFaceKind] = [.issue, .run, .changes, .results]
        XCTAssertEqual(WorkFaces.swipeTarget(faces: all, shown: .issue, direction: .left), .run)
        XCTAssertEqual(WorkFaces.swipeTarget(faces: all, shown: .run, direction: .left), .changes)
        XCTAssertEqual(WorkFaces.swipeTarget(faces: all, shown: .changes, direction: .right), .run)
        XCTAssertEqual(WorkFaces.swipeTarget(faces: all, shown: .run, direction: .right), .issue)
        XCTAssertNil(WorkFaces.swipeTarget(faces: all, shown: .issue, direction: .right))
        XCTAssertNil(WorkFaces.swipeTarget(faces: all, shown: .results, direction: .left))
        XCTAssertEqual(
            WorkFaces.swipeTarget(faces: [.issue, .results], shown: .issue, direction: .left),
            .results
        )
        XCTAssertNil(WorkFaces.swipeTarget(faces: [.issue], shown: .run, direction: .left))
        XCTAssertNil(WorkFaces.swipeTarget(faces: [], shown: .issue, direction: .left))
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
        // A session subject opened with the page's face: Issue lands on Run,
        // a run face it has is kept (`WorkScreen.init`).
        XCTAssertEqual(
            WorkFaces.fallbackFace(shown: .issue, available: [.run, .changes, .results]), .run
        )
        XCTAssertEqual(
            WorkFaces.fallbackFace(shown: .results, available: [.run, .changes, .results]), .results
        )
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

    // EXP-934 — mirrors web `shows the context menu on the issue face alone`
    // and Android `WorkFacesTest`.
    func testShowsTheContextMenuOnTheIssueFaceAlone() {
        XCTAssertTrue(WorkFaces.faceShowsContextMenu(.issue))
        XCTAssertFalse(WorkFaces.faceShowsContextMenu(.run))
        XCTAssertFalse(WorkFaces.faceShowsContextMenu(.changes))
        XCTAssertFalse(WorkFaces.faceShowsContextMenu(.results))
    }

    // EXP-1154: a Reviews row's `.changes` arrival survives the runs landing
    // before the issue row.
    func testHoldsAPendingChangesOrResultsFaceUntilRunsAndTheIssueRowLanded() {
        func holds(
            _ pending: WorkFaceKind,
            shown: WorkFaceKind? = nil,
            available: [WorkFaceKind] = [.issue],
            runs: Bool,
            issue: Bool
        ) -> Bool {
            WorkFaces.holdsPendingFace(
                pending: pending, shown: shown ?? pending, available: available,
                runsResolved: runs, issueResolved: issue
            )
        }
        // Runs not read yet: hold, whatever the face.
        XCTAssertTrue(holds(.run, runs: false, issue: false))
        XCTAssertTrue(holds(.changes, runs: false, issue: true))
        // Runs read, issue row still missing: Changes / Results keep holding.
        XCTAssertTrue(holds(.changes, runs: true, issue: false))
        XCTAssertTrue(holds(.results, runs: true, issue: false))
        XCTAssertFalse(holds(.run, runs: true, issue: false))
        // Both landed and the face is still missing: let go (fall back).
        XCTAssertFalse(holds(.changes, runs: true, issue: true))
        // The face arrived, or the reader moved: let go.
        XCTAssertFalse(holds(.changes, available: [.issue, .changes], runs: false, issue: false))
        XCTAssertFalse(holds(.changes, shown: .issue, runs: false, issue: false))
    }
}
