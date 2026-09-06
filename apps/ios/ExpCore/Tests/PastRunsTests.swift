import Foundation
import XCTest
@testable import ExpCore

// EXP-746: the "Past" section's pure rules — the same five tests web
// (`past-runs.test.ts`), Android (`AgentRowsTest`) and the desktop
// (`own_ended_runs_*`) run, so the four lists hold the same rows.
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

    func testThePastBylineNamesDeviceAgentAndWhoEndedIt() {
        XCTAssertEqual(
            PastRuns.byline(
                device: "macbook", agent: "Claude Code", endedBy: "user", relativeTime: "5m ago"
            ),
            "macbook · Claude Code · ended by you · 5m ago"
        )
        XCTAssertEqual(PastRuns.endedByPhrase("agent"), "agent")
        XCTAssertEqual(PastRuns.endedByPhrase("client"), "the app")
        XCTAssertEqual(PastRuns.endedByPhrase("merge"), "a merge")
        XCTAssertEqual(PastRuns.endedByPhrase("system"), "the system")
        // An unknown or absent value says less rather than inventing a culprit.
        XCTAssertNil(PastRuns.endedByPhrase("sweeper"))
        XCTAssertNil(PastRuns.endedByPhrase(nil))
        XCTAssertEqual(
            PastRuns.byline(device: "macbook", agent: nil, endedBy: nil, relativeTime: ""),
            "macbook"
        )
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
