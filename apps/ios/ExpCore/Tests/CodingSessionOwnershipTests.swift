import Foundation
import XCTest
@testable import ExpCore

// EXP-312 follow-up: the Agents surface lists the signed-in user's own coding
// sessions only — a teammate's live run is neither viewable nor steerable, so
// it must not appear in the list at all.
final class CodingSessionOwnershipTests: XCTestCase {
    private func session(id: String, userId: String) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: "issue-1",
            boardId: nil,
            teamId: "team-1",
            userId: userId,
            deviceLabel: nil,
            status: "running",
            startedAt: "2026-07-31T09:00:00Z",
            endedAt: nil,
            createdAt: "2026-07-31T09:00:00Z",
            updatedAt: "2026-07-31T09:00:00Z"
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
}
