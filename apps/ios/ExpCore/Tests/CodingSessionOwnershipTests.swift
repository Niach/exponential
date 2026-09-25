import Foundation
import XCTest
@testable import ExpCore

// EXP-312 follow-up: the Agents surface lists the signed-in user's own coding
// sessions only — a teammate's live run is neither viewable nor steerable, so
// it must not appear in the list at all.
final class CodingSessionOwnershipTests: XCTestCase {
    private static let baseIso = "2026-07-31T09:00:00Z"
    private static let base = ISO8601DateFormatter().date(from: baseIso)!

    private func session(
        id: String,
        userId: String,
        teamId: String = "team-1",
        status: String = "running",
        needsInput: Bool = false,
        endedBy: String? = nil,
        updatedAt: String = CodingSessionOwnershipTests.baseIso
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: "issue-1",
            boardId: nil,
            teamId: teamId,
            userId: userId,
            deviceLabel: nil,
            status: status,
            needsInput: needsInput,
            endedBy: endedBy,
            startedAt: Self.baseIso,
            endedAt: status == "ended" ? Self.baseIso : nil,
            createdAt: Self.baseIso,
            updatedAt: updatedAt
        )
    }

    func testOwnSessionIsOwn() {
        XCTAssertTrue(CodingSessionOwnership.isOwn(session(id: "s1", userId: "me"), userId: "me"))
    }

    func testTeammateSessionIsNotOwn() {
        XCTAssertFalse(CodingSessionOwnership.isOwn(session(id: "s1", userId: "you"), userId: "me"))
    }

    func testNoResolvedUserOwnsNothing() {
        // No signed-in account resolved: the list must render its empty state,
        // never every member's sessions.
        XCTAssertFalse(CodingSessionOwnership.isOwn(session(id: "s1", userId: "you"), userId: nil))
        XCTAssertFalse(CodingSessionOwnership.isOwn(session(id: "s1", userId: "you"), userId: ""))
    }

    func testOwnKeepsOnlyTheCallersSessions() {
        let sessions = [
            session(id: "mine-1", userId: "me"),
            session(id: "theirs-1", userId: "you"),
            session(id: "mine-2", userId: "me"),
            session(id: "theirs-2", userId: "someone-else"),
        ]
        XCTAssertEqual(
            CodingSessionOwnership.own(sessions, userId: "me").map(\.id),
            ["mine-1", "mine-2"]
        )
    }

    func testOwnIsEmptyWhenOnlyTeammatesAreRunning() {
        let sessions = [session(id: "theirs-1", userId: "you")]
        XCTAssertTrue(CodingSessionOwnership.own(sessions, userId: "me").isEmpty)
    }

    // Team scoping (web parity, `use-agents-data.ts`): the Agents surface lists
    // the ACTIVE team's own sessions — a run in another team belongs there.

    func testOwnSessionInTheActiveTeamIsOwn() {
        XCTAssertTrue(
            CodingSessionOwnership.isOwn(
                session(id: "s1", userId: "me"), userId: "me", teamId: "team-1"
            )
        )
    }

    func testOwnSessionInAnotherTeamIsExcluded() {
        XCTAssertFalse(
            CodingSessionOwnership.isOwn(
                session(id: "s1", userId: "me", teamId: "team-2"),
                userId: "me",
                teamId: "team-1"
            )
        )
    }

    func testNoActiveTeamShowsNothing() {
        // No team resolved yet: the empty state, never every team's sessions.
        XCTAssertFalse(
            CodingSessionOwnership.isOwn(
                session(id: "s1", userId: "me"), userId: "me", teamId: nil
            )
        )
        XCTAssertFalse(
            CodingSessionOwnership.isOwn(
                session(id: "s1", userId: "me"), userId: "me", teamId: ""
            )
        )
    }

    func testTeamScopedOwnKeepsOnlyTheActiveTeamsCallerSessions() {
        let sessions = [
            session(id: "mine-1", userId: "me"),
            session(id: "mine-elsewhere", userId: "me", teamId: "team-2"),
            session(id: "theirs-1", userId: "you"),
            session(id: "mine-2", userId: "me"),
        ]
        XCTAssertEqual(
            CodingSessionOwnership.own(sessions, userId: "me", teamId: "team-1").map(\.id),
            ["mine-1", "mine-2"]
        )
        XCTAssertTrue(
            CodingSessionOwnership.own(sessions, userId: "me", teamId: nil).isEmpty
        )
    }

    // EXP-1075: live own runs per team — the signal behind the board
    // switcher's "another team has your runs" dot. The active team is
    // excluded there: its runs already light the Agents tab.

    private var soon: Date { Self.base.addingTimeInterval(60) }

    func testLiveByTeamGroupsOwnLiveRunsByTeam() {
        let sessions = [
            session(id: "a", userId: "me", teamId: "team-1"),
            session(id: "b", userId: "me", teamId: "team-2"),
            session(id: "c", userId: "me", teamId: "team-2", status: "in_review"),
        ]
        let byTeam = CodingSessionOwnership.liveByTeam(sessions, userId: "me", now: soon)
        XCTAssertEqual(byTeam["team-1"], CodingSessionOwnership.TeamLiveRuns(count: 1))
        // `in_review` is still LIVE (EXP-194), so it counts.
        XCTAssertEqual(byTeam["team-2"], CodingSessionOwnership.TeamLiveRuns(count: 2))
    }

    func testLiveByTeamIgnoresTeammateSessions() {
        let sessions = [
            session(id: "mine", userId: "me", teamId: "team-1"),
            session(id: "theirs", userId: "you", teamId: "team-2"),
        ]
        let byTeam = CodingSessionOwnership.liveByTeam(sessions, userId: "me", now: soon)
        XCTAssertEqual(byTeam.keys.sorted(), ["team-1"])
        XCTAssertNil(CodingSessionOwnership.liveByTeam(sessions, userId: nil, now: soon)["team-1"])
    }

    func testLiveByTeamDropsEndedAndStaleSessions() {
        let sessions = [
            session(id: "ended", userId: "me", teamId: "team-2", status: "ended", endedBy: "user"),
            // Heartbeat older than the contract stale window (EXP-153): absent.
            session(id: "stale", userId: "me", teamId: "team-3"),
            session(id: "live", userId: "me", teamId: "team-4"),
        ]
        // `now` is 3h past the fixture heartbeat, so only a freshly beating row
        // survives.
        let stale = [sessions[0], sessions[1]]
        XCTAssertTrue(
            CodingSessionOwnership.liveByTeam(
                stale, userId: "me", now: Self.base.addingTimeInterval(3 * 3600)
            ).isEmpty
        )
        XCTAssertEqual(
            CodingSessionOwnership.liveByTeam([sessions[2]], userId: "me", now: soon)["team-4"],
            CodingSessionOwnership.TeamLiveRuns(count: 1)
        )
    }

    func testLiveByTeamNeedsInputLiftsOnlyItsOwnTeam() {
        let sessions = [
            session(id: "calm", userId: "me", teamId: "team-1"),
            session(id: "asking", userId: "me", teamId: "team-2", needsInput: true),
            session(id: "calm-2", userId: "me", teamId: "team-2"),
        ]
        let byTeam = CodingSessionOwnership.liveByTeam(sessions, userId: "me", now: soon)
        XCTAssertEqual(byTeam["team-1"]?.needsInput, false)
        XCTAssertEqual(byTeam["team-2"], CodingSessionOwnership.TeamLiveRuns(count: 2, needsInput: true))
    }

    func testLiveByTeamMasksNeedsInputBehindReview() {
        // EXP-531/679 ordering: an in_review run is "ready for review", never
        // amber — the switcher dot goes through the same mask as the tab dot.
        let sessions = [
            session(id: "review", userId: "me", teamId: "team-2", status: "in_review", needsInput: true)
        ]
        let byTeam = CodingSessionOwnership.liveByTeam(sessions, userId: "me", now: soon)
        XCTAssertEqual(byTeam["team-2"], CodingSessionOwnership.TeamLiveRuns(count: 1, needsInput: false))
    }

    func testOtherTeamsLiveExcludesTheActiveTeam() {
        let byTeam: [String: CodingSessionOwnership.TeamLiveRuns] = [
            "team-1": .init(count: 2, needsInput: true)
        ]
        let active = CodingSessionOwnership.otherTeamsLive(byTeam, activeTeamId: "team-1")
        XCTAssertFalse(active.any)
        XCTAssertFalse(active.needsInput)

        let elsewhere = CodingSessionOwnership.otherTeamsLive(byTeam, activeTeamId: "team-9")
        XCTAssertTrue(elsewhere.any)
        XCTAssertTrue(elsewhere.needsInput)
    }

    func testOtherTeamsLiveAmberOnlyWhenAnotherTeamAsks() {
        let byTeam: [String: CodingSessionOwnership.TeamLiveRuns] = [
            "team-1": .init(count: 1, needsInput: true),
            "team-2": .init(count: 1, needsInput: false),
        ]
        // The asking team IS the active one: the other team still shows a dot,
        // but a green one.
        let result = CodingSessionOwnership.otherTeamsLive(byTeam, activeTeamId: "team-1")
        XCTAssertTrue(result.any)
        XCTAssertFalse(result.needsInput)
    }

    func testOtherTeamsLiveIsQuietWithNothingLive() {
        XCTAssertFalse(CodingSessionOwnership.otherTeamsLive([:], activeTeamId: "team-1").any)
        XCTAssertFalse(
            CodingSessionOwnership.otherTeamsLive(
                ["team-2": .init(count: 0, needsInput: false)], activeTeamId: "team-1"
            ).any
        )
    }
}
