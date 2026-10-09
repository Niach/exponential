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

    func testListsTheAvailableFacesInIssueRunGuideOrder() {
        XCTAssertEqual(
            WorkFaces.availableFaces(hasIssue: true, hasRun: true, hasResults: true, hasDiff: true),
            [.issue, .run, .guide]
        )
        XCTAssertEqual(
            WorkFaces.availableFaces(hasIssue: false, hasRun: true, hasResults: false, hasDiff: false),
            [.run]
        )
        // EXP-1251: a diff alone (an open PR, no run of mine) is a Guide.
        XCTAssertEqual(
            WorkFaces.availableFaces(hasIssue: true, hasRun: false, hasResults: false, hasDiff: true),
            [.issue, .guide]
        )
        // So are results alone.
        XCTAssertEqual(
            WorkFaces.availableFaces(hasIssue: true, hasRun: true, hasResults: true, hasDiff: false),
            [.issue, .run, .guide]
        )
    }

    func testLabelsTheFacesRunsOnceThereAreSeveral() {
        XCTAssertEqual(WorkFaces.faceLabel(.issue), "Issue")
        XCTAssertEqual(WorkFaces.faceLabel(.run), "Run")
        XCTAssertEqual(WorkFaces.faceLabel(.run, multipleRuns: true), "Runs")
        XCTAssertEqual(WorkFaces.faceLabel(.guide), "Guide")
        XCTAssertEqual(WorkFaces.guideFaceLabel, "Guide")
        XCTAssertEqual(WorkFaces.openResultsLabel, "Open Guide")
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

    func testSwipesToTheNeighbouringFace() {
        let all: [WorkFaceKind] = [.issue, .run, .guide]
        XCTAssertEqual(WorkFaces.swipeTarget(faces: all, shown: .issue, direction: .left), .run)
        XCTAssertEqual(WorkFaces.swipeTarget(faces: all, shown: .run, direction: .left), .guide)
        XCTAssertEqual(WorkFaces.swipeTarget(faces: all, shown: .guide, direction: .right), .run)
        XCTAssertEqual(WorkFaces.swipeTarget(faces: all, shown: .run, direction: .right), .issue)
        XCTAssertNil(WorkFaces.swipeTarget(faces: all, shown: .issue, direction: .right))
        XCTAssertNil(WorkFaces.swipeTarget(faces: all, shown: .guide, direction: .left))
        XCTAssertEqual(
            WorkFaces.swipeTarget(faces: [.issue, .guide], shown: .issue, direction: .left),
            .guide
        )
        XCTAssertNil(WorkFaces.swipeTarget(faces: [.issue], shown: .run, direction: .left))
        XCTAssertNil(WorkFaces.swipeTarget(faces: [], shown: .issue, direction: .left))
    }

    func testFallsBackGuideToRunToIssue() {
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .guide, available: [.issue, .run]), .run)
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .guide, available: [.issue]), .issue)
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .guide, available: [.issue, .run, .guide]), .guide)
        XCTAssertNil(WorkFaces.fallbackFace(shown: .guide, available: []))
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .run, available: [.issue]), .issue)
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .run, available: [.issue, .run]), .run)
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .issue, available: [.run]), .run)
        XCTAssertNil(WorkFaces.fallbackFace(shown: .run, available: []))
        // A session subject opened with the page's face: Issue lands on Run.
        XCTAssertEqual(WorkFaces.fallbackFace(shown: .issue, available: [.run, .guide]), .run)
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
        XCTAssertFalse(WorkFaces.faceShowsContextMenu(.guide))
    }

    // EXP-1154: a Reviews row's Guide arrival survives the runs landing
    // before the issue row.
    func testHoldsAPendingGuideFaceUntilRunsAndTheIssueRowLanded() {
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
        XCTAssertTrue(holds(.guide, runs: false, issue: true))
        // Runs read, issue row still missing: the Guide keeps holding.
        XCTAssertTrue(holds(.guide, runs: true, issue: false))
        XCTAssertFalse(holds(.run, runs: true, issue: false))
        // Both landed and the face is still missing: let go (fall back).
        XCTAssertFalse(holds(.guide, runs: true, issue: true))
        // The face arrived, or the reader moved: let go.
        XCTAssertFalse(holds(.guide, available: [.issue, .guide], runs: false, issue: false))
        XCTAssertFalse(holds(.guide, shown: .issue, runs: false, issue: false))
    }

    // MARK: EXP-1251 — the Guide's section pages, mirrors web `guideSectionPage`.

    private let sectionFiles = [
        Diff.File(path: "a.ts", additions: 3, deletions: 1),
        Diff.File(path: "b.ts", previousPath: "old-b.ts", additions: 5, deletions: 0),
        Diff.File(path: "c.ts", additions: 1, deletions: 1),
    ]
    private let sectionGroups = [
        SessionResultGroup(topic: "Summary", entries: [], files: []),
        SessionResultGroup(topic: "Model", entries: [], files: ["a.ts"]),
        SessionResultGroup(topic: "Paint", entries: [], files: ["old-b.ts"]),
    ]

    func testOpensANumberedSectionWithItsCaptionAndCoveredFiles() {
        let page = WorkFaces.guideSectionPage(sectionGroups, files: sectionFiles, section: .section(2))
        XCTAssertEqual(page?.caption, "02 / 02")
        XCTAssertEqual(page?.title, "Paint")
        XCTAssertEqual(page?.files.map(\.path), ["b.ts"])
        XCTAssertEqual(page?.additions, 5)
        XCTAssertEqual(page?.deletions, 0)
        XCTAssertEqual(page.map(WorkFaces.guideSectionSummary), "+5 \u{2212}0 · 1 file")
    }

    func testOpensOtherChangesAndTheCompleteDiff() {
        let other = WorkFaces.guideSectionPage(sectionGroups, files: sectionFiles, section: .other)
        XCTAssertEqual(other?.files.map(\.path), ["c.ts"])
        XCTAssertEqual(other?.title, "Other changes")
        let all = WorkFaces.guideSectionPage(sectionGroups, files: sectionFiles, section: .all)
        XCTAssertEqual(all?.files.count, 3)
        XCTAssertNil(all?.caption)
        XCTAssertEqual(all?.additions, 9)
        XCTAssertEqual(all?.deletions, 2)
    }

    func testIsNullForAStaleSectionOrWhileTheDiffLoads() {
        XCTAssertNil(WorkFaces.guideSectionPage(sectionGroups, files: sectionFiles, section: .section(7)))
        XCTAssertNil(WorkFaces.guideSectionPage(sectionGroups, files: nil, section: .section(1)))
        XCTAssertNil(WorkFaces.guideSectionPage(
            [SessionResultGroup(topic: "Model", entries: [], files: ["a.ts", "b.ts", "c.ts"])],
            files: sectionFiles, section: .other
        ))
    }

    func testWithNoReportTheWholeDiffIsOneChangesSection() {
        XCTAssertEqual(WorkFaces.guideSectionPage([], files: sectionFiles, section: .other)?.title, "Changes")
    }

    // MARK: EXP-1175 — the run row (`run-row.json`), mirrors web `run row`.

    private func runRowFixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/run-row.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testLabelsTheShowWorkSwitchOffTheFixture() throws {
        let fixture = try runRowFixture()
        XCTAssertEqual(WorkFaces.showWorkText, fixture["showWorkLabel"] as? String)
        XCTAssertEqual(WorkFaces.hideWorkText, fixture["hideWorkLabel"] as? String)
        XCTAssertEqual(WorkFaces.showWorkDefault, fixture["showWorkDefault"] as? Bool)
        XCTAssertEqual(WorkFaces.showWorkLabel(false), fixture["showWorkLabel"] as? String)
        XCTAssertEqual(WorkFaces.showWorkLabel(true), fixture["hideWorkLabel"] as? String)
        XCTAssertEqual(WorkFaces.showWorkDefaultsKey(accountId: "u1"), "run_show_work_u1")
    }

    func testDerivesTheRunRowStatePerTheFixture() throws {
        let cases = try XCTUnwrap(try runRowFixture()["states"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let display = try XCTUnwrap(
                CodingSessionDisplayState(rawValue: try XCTUnwrap(testCase["display"] as? String)), name
            )
            let state = WorkFaces.runRowState(
                paused: try XCTUnwrap(testCase["paused"] as? Bool, name),
                ended: try XCTUnwrap(testCase["ended"] as? Bool, name),
                awaitingInput: try XCTUnwrap(testCase["awaitingInput"] as? Bool, name),
                working: try XCTUnwrap(testCase["working"] as? Bool, name),
                display: display
            )
            XCTAssertEqual(state.rawValue, testCase["expected"] as? String, name)
        }
    }

    func testCaptionsTheRunRowPerTheFixture() throws {
        let cases = try XCTUnwrap(try runRowFixture()["captions"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let state = try XCTUnwrap(RunRowState(rawValue: try XCTUnwrap(testCase["state"] as? String)), name)
            let expected = try XCTUnwrap(testCase["expected"] as? [String: Any], name)
            let now = try XCTUnwrap(WireTimestamps.parse(try XCTUnwrap(testCase["now"] as? String)), name)
            let caption = WorkFaces.runRowCaption(
                state: state,
                device: try XCTUnwrap(testCase["device"] as? String),
                startedAt: testCase["startedAt"] as? String,
                endedAt: testCase["endedAt"] as? String,
                now: now
            )
            XCTAssertEqual(caption.text, expected["text"] as? String, name)
            XCTAssertEqual(caption.tone.rawValue, expected["tone"] as? String, name)
        }
    }

    func testTurnRowCaptionMatchesTheSharedFixture() throws {
        let cases = try XCTUnwrap(try runRowFixture()["turnCaptions"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let turn = try XCTUnwrap(testCase["turn"] as? [String: Any], name)
            let state = try XCTUnwrap(RunRowState(rawValue: try XCTUnwrap(testCase["state"] as? String)), name)
            let now = try XCTUnwrap(WireTimestamps.parse(try XCTUnwrap(testCase["now"] as? String)), name)
            let caption = WorkFaces.turnRowCaption(
                turnStartedAt: (turn["startedAt"] as? String).flatMap(WireTimestamps.parse),
                turnEndedAt: (turn["endedAt"] as? String).flatMap(WireTimestamps.parse),
                state: state,
                device: try XCTUnwrap(testCase["device"] as? String),
                runEndedAt: (testCase["runEndedAt"] as? String).flatMap(WireTimestamps.parse),
                now: now
            )
            if let expected = testCase["expected"] as? [String: Any] {
                XCTAssertEqual(caption?.text, expected["text"] as? String, name)
                XCTAssertEqual(caption?.tone.rawValue, expected["tone"] as? String, name)
            } else {
                XCTAssertNil(caption, name)
            }
        }
    }
}
