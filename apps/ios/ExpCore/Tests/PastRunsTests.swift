import Foundation
import XCTest
@testable import ExpCore

// EXP-746: the "Recent" section's (and EXP-886's issue runs / run switcher)
// pure rules — the same tests web (`past-runs.test.ts`), Android
// (`AgentRowsTest`) and the desktop (`own_ended_runs_*` / `issue_runs_*`)
// run, so the four lists hold the same rows.
final class PastRunsTests: XCTestCase {

    private func session(
        id: String,
        userId: String = "user-1",
        teamId: String = "team-1",
        status: String = "ended",
        startedReason: String? = nil,
        issueId: String? = "issue-1",
        actionName: String? = nil,
        endedAt: String? = "2026-09-01T10:00:00Z",
        updatedAt: String = "2026-09-01T10:00:00Z"
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: issueId,
            teamId: teamId,
            userId: userId,
            deviceLabel: "macbook",
            deviceId: "dev-1",
            status: status,
            agent: "claude",
            actionName: actionName,
            startedReason: startedReason,
            endedBy: "user",
            startedAt: "2026-09-01T09:00:00Z",
            endedAt: endedAt,
            createdAt: "2026-09-01T09:00:00Z",
            updatedAt: updatedAt
        )
    }

    private func issue(id: String = "issue-1", title: String = "Fix the sync loop") -> IssueEntity {
        IssueEntity(
            id: id,
            boardId: "board-1",
            number: 12,
            identifier: "EXP-12",
            title: title,
            description: nil,
            status: "done",
            priority: "none",
            assigneeId: nil,
            creatorId: nil,
            source: nil,
            dueDate: nil,
            sortOrder: nil,
            completedAt: nil,
            duplicateOfId: nil,
            prUrl: nil,
            prNumber: nil,
            prState: nil,
            branch: nil,
            prMergedAt: nil,
            createdAt: "2026-09-01T09:00:00Z",
            updatedAt: "2026-09-01T09:00:00Z"
        )
    }

    func testSelectPastRunsListsOnlyOwnEndedPersonStartedRuns() {
        let rows = PastRuns.select(
            [
                session(id: "mine"),
                session(id: "theirs", userId: "user-2"),
                session(id: "other-team", teamId: "team-2"),
                session(id: "live", status: "running"),
            ],
            userId: "user-1",
            teamId: "team-1"
        )
        XCTAssertEqual(rows.map(\.id), ["mine"])
        // No resolved user or team lists nothing at all.
        XCTAssertTrue(PastRuns.select([session(id: "mine")], userId: nil, teamId: "team-1").isEmpty)
        XCTAssertTrue(PastRuns.select([session(id: "mine")], userId: "user-1", teamId: nil).isEmpty)
    }

    /// EXP-676: automation runs live under Automations' "Recent automated
    /// runs" and nowhere else.
    func testAScheduledRunNeverListsUnderPast() {
        let rows = PastRuns.select(
            [
                session(id: "scheduled", startedReason: "schedule"),
                session(id: "event", startedReason: "event"),
                session(id: "person"),
            ],
            userId: "user-1",
            teamId: "team-1"
        )
        XCTAssertEqual(rows.map(\.id), ["person"])
    }

    func testTheListIsNewestFirstByEndedAtThenUpdatedAt() {
        let rows = PastRuns.select(
            [
                session(id: "older", endedAt: "2026-09-01T08:00:00Z"),
                // No end stamp ever landed — its heartbeat orders it.
                session(id: "heartbeat", endedAt: nil, updatedAt: "2026-09-01T12:00:00Z"),
                session(id: "newer", endedAt: "2026-09-01T11:00:00Z"),
            ],
            userId: "user-1",
            teamId: "team-1"
        )
        XCTAssertEqual(rows.map(\.id), ["heartbeat", "newer", "older"])
        XCTAssertEqual(PastRuns.endedAt(session(id: "a")), "2026-09-01T10:00:00Z")
        XCTAssertEqual(
            PastRuns.endedAt(session(id: "b", endedAt: nil, updatedAt: "2026-09-01T12:00:00Z")),
            "2026-09-01T12:00:00Z"
        )
    }

    func testTheListIsCappedAtTwenty() {
        let sessions = (0..<30).map { i in
            session(id: "s\(i)", endedAt: String(format: "2026-09-01T%02d:00:00Z", i))
        }
        let rows = PastRuns.select(sessions, userId: "user-1", teamId: "team-1")
        XCTAssertEqual(PastRuns.cap, 20)
        XCTAssertEqual(rows.count, 20)
        XCTAssertEqual(rows.first?.id, "s29")
        XCTAssertEqual(rows.last?.id, "s10")
    }

    /// EXP-758: the SQL observation bounds the fetch, the pure filter bounds
    /// the list — so the query has to read WIDER than the cap (Android's
    /// `PAST_RUN_QUERY_LIMIT`), or a dropped row would shorten the section.
    func testTheQueryReadsWiderThanTheCap() {
        XCTAssertEqual(PastRuns.queryLimit, 50)
        XCTAssertGreaterThan(PastRuns.queryLimit, PastRuns.cap)
        // The cap still holds when the query hands over its full page.
        let sessions = (0..<PastRuns.queryLimit).map { i in
            session(id: "s\(i)", endedAt: String(format: "2026-09-01T%02d:00:00Z", i % 24))
        }
        XCTAssertEqual(
            PastRuns.select(sessions, userId: "user-1", teamId: "team-1").count, PastRuns.cap
        )
    }

    /// EXP-886: the issue's runs behind the Run/Runs label and the switcher.
    func testIssueRunsListOnlyTheCallersOwnRunsOfThatIssue() {
        let rows = PastRuns.issueRuns(
            [
                session(id: "mine"),
                // An automated run of the issue is one of its runs too.
                session(id: "automated", startedReason: "schedule"),
                session(id: "theirs", userId: "user-2"),
                // So are the live ones — they lead the list.
                session(id: "live", status: "running", endedAt: nil, updatedAt: "2026-09-01T12:00:00Z"),
                session(id: "in-review", status: "in_review", endedAt: nil, updatedAt: "2026-09-01T11:00:00Z"),
                session(id: "other-issue", issueId: "issue-2"),
                session(id: "batch", issueId: nil),
                // Team does not scope it: the issue already does.
                session(id: "other-team", teamId: "team-2", endedAt: "2026-09-01T09:00:00Z"),
            ],
            issueId: "issue-1",
            userId: "user-1"
        )
        XCTAssertEqual(rows.map(\.id), ["live", "in-review", "mine", "automated", "other-team"])
        XCTAssertTrue(PastRuns.issueRuns([session(id: "mine")], issueId: "issue-1", userId: nil).isEmpty)
        XCTAssertTrue(PastRuns.issueRuns([session(id: "mine")], issueId: nil, userId: "user-1").isEmpty)
        XCTAssertTrue(PastRuns.isLiveRunStatus("running"))
        XCTAssertTrue(PastRuns.isLiveRunStatus("in_review"))
        XCTAssertFalse(PastRuns.isLiveRunStatus("ended"))
    }

    func testIssueRunsPutLiveFirstThenNewestEndFirstUncapped() {
        let sessions = (0..<30).map { i in
            session(id: "s\(i)", endedAt: String(format: "2026-09-02T%02d:%02d:00Z", i / 2, i))
        } + [
            session(id: "heartbeat", endedAt: nil, updatedAt: "2026-09-03T00:00:00Z"),
            // A live run with an OLD heartbeat still leads: liveness is by
            // status, the stamp only orders within a group.
            session(id: "stale-live", status: "running", endedAt: nil, updatedAt: "2026-08-01T00:00:00Z"),
        ]
        let rows = PastRuns.issueRuns(sessions, issueId: "issue-1", userId: "user-1")
        XCTAssertEqual(rows.count, 32)
        XCTAssertEqual(rows[0].id, "stale-live")
        XCTAssertEqual(rows[1].id, "heartbeat")
        XCTAssertEqual(rows[2].id, "s29")
        XCTAssertEqual(rows.last?.id, "s0")
    }

    /// EXP-886: a switcher entry says `Live` for a live run and the ended
    /// time otherwise — byte-identical ×4 (web `issueRunEntryLabel`).
    func testARunEntrySaysLiveForALiveRunAndTheEndedTimeOtherwise() {
        XCTAssertEqual(PastRuns.liveRunLabel, "Live")
        let live = session(id: "live", status: "running")
        XCTAssertEqual(PastRuns.issueRunWhen(live, endedRelative: "2 hours ago"), "Live")
        XCTAssertEqual(
            PastRuns.byline(device: "macbook", relativeTime: PastRuns.issueRunWhen(live, endedRelative: "")),
            "macbook · Live"
        )
        let review = session(id: "review", status: "in_review")
        XCTAssertEqual(PastRuns.issueRunWhen(review, endedRelative: ""), "Live")
        let ended = session(id: "ended")
        XCTAssertEqual(PastRuns.issueRunWhen(ended, endedRelative: "2 hours ago"), "2 hours ago")
        // No honest stamp: the time segment simply drops.
        XCTAssertEqual(
            PastRuns.byline(device: "macbook", relativeTime: PastRuns.issueRunWhen(ended, endedRelative: "")),
            "macbook"
        )
    }

    func testThePastBylineNamesTheDeviceAndWhenItEnded() {
        // Byte-identical ×4 — web `pastRunByline`, Android `pastRunByline`,
        // desktop `run_rows::past_run_byline`. EXP-833: device and time only,
        // no agent, no "ended by".
        XCTAssertEqual(
            PastRuns.byline(device: "macbook", relativeTime: "5m ago"),
            "macbook · 5m ago"
        )
        // A missing segment drops out instead of printing a placeholder.
        XCTAssertEqual(PastRuns.byline(device: "macbook", relativeTime: ""), "macbook")
        XCTAssertEqual(PastRuns.byline(device: "", relativeTime: "5m ago"), "5m ago")
    }

    func testARowTitlesItselfFromWhateverItHas() {
        // Byte-identical ×4 — web `pastRunTitle`, Android `pastRunTitle`,
        // desktop `session_title` name the same ended run the same way.
        XCTAssertEqual(PastRuns.title(session(id: "a"), issue: issue()), "Fix the sync loop")
        XCTAssertEqual(
            PastRuns.title(session(id: "a"), issue: issue(title: "  ")), "Untitled issue"
        )
        // The issue row has not synced yet.
        XCTAssertEqual(PastRuns.title(session(id: "a"), issue: nil), "Issue syncing…")
        XCTAssertEqual(
            PastRuns.title(
                session(id: "a", issueId: nil, actionName: "Release train"), issue: nil
            ),
            "Release train"
        )
        // A chat run carries "Chat" as its action snapshot (EXP-615).
        XCTAssertEqual(
            PastRuns.title(session(id: "a", issueId: nil, actionName: "Chat"), issue: nil),
            "Chat"
        )
        XCTAssertEqual(
            PastRuns.title(session(id: "a", issueId: nil, actionName: "  "), issue: nil),
            "Batch run"
        )
        XCTAssertEqual(PastRuns.title(session(id: "a", issueId: nil), issue: nil), "Batch run")
    }
}
