import Foundation
import XCTest
@testable import ExpCore

// EXP-536: every start surface pushes the run it just launched, so the row the
// desktop inserts has to be recognizable from the client. Android
// (`domain/StartedRunMatch.kt`) and web (`lib/started-run-match.ts`) mirror
// these rules.
final class StartedRunMatchTests: XCTestCase {
    // 2026-07-17T12:00:00Z; the watch cuts off at now - skew.
    private let now = Date(timeIntervalSince1970: 1_784_289_600)
    private var cutoff: Date { now.addingTimeInterval(-StartedRunMatch.skew) }

    private func session(
        id: String = "sess-1",
        issueId: String? = nil,
        actionName: String? = nil,
        startedReason: String? = nil,
        resumedFromId: String? = nil,
        userId: String = "user-1",
        startedAt: String = "2026-07-17T11:59:30Z"
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: issueId,
            teamId: "team-1",
            userId: userId,
            deviceLabel: nil,
            status: "running",
            actionName: actionName,
            startedReason: startedReason,
            resumedFromId: resumedFromId,
            startedAt: startedAt,
            endedAt: nil,
            createdAt: startedAt,
            updatedAt: startedAt
        )
    }

    private func matches(_ session: CodingSessionEntity, _ key: StartedRunKey) -> Bool {
        StartedRunMatch.matches(session, key: key, userId: "user-1", cutoff: cutoff)
    }

    func testOneIssueIsASingleRunTwoOrMoreIsABatch() {
        XCTAssertNil(StartedRunKey.forIssues([]))
        XCTAssertEqual(StartedRunKey.forIssues(["a"]), .issue(id: "a"))
        XCTAssertEqual(StartedRunKey.forIssues(["a", "b"]), .batch)
    }

    func testSingleRunMatchesItsOwnIssueRow() {
        XCTAssertTrue(matches(session(issueId: "issue-1"), .issue(id: "issue-1")))
        XCTAssertFalse(matches(session(issueId: "issue-2"), .issue(id: "issue-1")))
    }

    func testBatchMatchesTheIssuelessActionlessRow() {
        XCTAssertTrue(matches(session(), .batch))
        // A single-issue run is not the batch we started…
        XCTAssertFalse(matches(session(issueId: "issue-1"), .batch))
        // …and neither is an action run, which also carries no issue.
        XCTAssertFalse(matches(session(actionName: "Fix merge conflicts"), .batch))
    }

    func testActionRunMatchesOnTheNameSnapshot() {
        // action_id is NULL for the builtins, so the name is the only key.
        XCTAssertTrue(
            matches(session(actionName: "Fix merge conflicts"), .action(name: "Fix merge conflicts"))
        )
        XCTAssertFalse(
            matches(session(actionName: "Create action"), .action(name: "Fix merge conflicts"))
        )
    }

    // EXP-530: an automation-started run carries the same action-name
    // snapshot — a user's pending start watch must never grab it, or a
    // schedule firing in the window hijacks the navigation.
    func testActionRunIgnoresAutomationStartedRows() {
        XCTAssertFalse(
            matches(
                session(actionName: "Fix merge conflicts", startedReason: "schedule"),
                .action(name: "Fix merge conflicts")
            )
        )
        XCTAssertFalse(
            matches(
                session(actionName: "Fix merge conflicts", startedReason: "event"),
                .action(name: "Fix merge conflicts")
            )
        )
    }

    // EXP-637: a resume is keyed on the ENDED run's id, which the desktop
    // stamps as `resumed_from_id` on the fresh row. The row's own kind is
    // whatever the ended run was, so nothing else may be asserted.
    func testResumedRunMatchesOnTheResumeLink() {
        XCTAssertTrue(
            matches(session(resumedFromId: "sess-old"), .resumed(fromId: "sess-old"))
        )
        XCTAssertTrue(
            matches(
                session(issueId: "issue-1", resumedFromId: "sess-old"),
                .resumed(fromId: "sess-old")
            )
        )
        XCTAssertTrue(
            matches(
                session(actionName: "Refresh screenshots", resumedFromId: "sess-old"),
                .resumed(fromId: "sess-old")
            )
        )
        XCTAssertFalse(matches(session(resumedFromId: "other"), .resumed(fromId: "sess-old")))
        // A fresh run started alongside the resume carries no link at all.
        XCTAssertFalse(matches(session(), .resumed(fromId: "sess-old")))
    }

    // Only a person resumes a run, so an automation-started row can never be
    // this send (the same rule the action key already applies).
    func testResumedRunIgnoresAutomationStartedRows() {
        XCTAssertFalse(
            matches(
                session(startedReason: "schedule", resumedFromId: "sess-old"),
                .resumed(fromId: "sess-old")
            )
        )
    }

    func testResumedRunIgnoresTeammatesAndOldRows() {
        XCTAssertFalse(
            matches(session(resumedFromId: "sess-old", userId: "user-2"), .resumed(fromId: "sess-old"))
        )
        // An earlier resume of the SAME ended run is outside the cutoff.
        XCTAssertFalse(
            matches(
                session(resumedFromId: "sess-old", startedAt: "2026-07-17T11:00:00Z"),
                .resumed(fromId: "sess-old")
            )
        )
    }

    func testOtherKeysIgnoreAResumedRow() {
        // A resumed batch row still satisfies `.batch` — the resume link is an
        // ADDITIONAL key, not an exclusion on the existing ones.
        XCTAssertTrue(matches(session(resumedFromId: "sess-old"), .batch))
    }

    func testIssueRunIgnoresAnActionRunOnTheSameIssue() {
        let row = session(issueId: "issue-1", actionName: "Fix merge conflicts")
        XCTAssertFalse(matches(row, .issue(id: "issue-1")))
    }

    func testTeammatesRunsAndOldRowsNeverMatch() {
        XCTAssertFalse(matches(session(issueId: "issue-1", userId: "user-2"), .issue(id: "issue-1")))
        // Started an hour ago — a pre-existing session, not the one we sent.
        XCTAssertFalse(
            matches(session(issueId: "issue-1", startedAt: "2026-07-17T11:00:00Z"), .issue(id: "issue-1"))
        )
    }

    func testUnparseableStartedAtFailsClosed() {
        // Unlike liveness (fail-open), pushing into the wrong session is worse
        // than not pushing at all.
        XCTAssertFalse(matches(session(issueId: "issue-1", startedAt: "nonsense"), .issue(id: "issue-1")))
    }

    func testFindPicksTheMatchingRow() {
        let rows = [session(id: "other", issueId: "issue-9"), session(id: "mine", issueId: "issue-1")]
        XCTAssertEqual(
            StartedRunMatch.find(in: rows, key: .issue(id: "issue-1"), userId: "user-1", cutoff: cutoff)?.id,
            "mine"
        )
        XCTAssertNil(
            StartedRunMatch.find(in: rows, key: .batch, userId: "user-1", cutoff: cutoff)
        )
    }

    func testElectricWireTimestampFormParses() {
        // Electric syncs started_at as Postgres text (space separator,
        // hour-only offset) — an ISO8601-only parser would reject every row
        // and no start would ever navigate.
        XCTAssertTrue(
            matches(session(issueId: "issue-1", startedAt: "2026-07-17 11:59:30.123456+00"), .issue(id: "issue-1"))
        )
    }
}
