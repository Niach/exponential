import Foundation
import XCTest
@testable import ExpCore

// EXP-481: the resume-offer matcher and the devices-shape composition — the
// pure halves behind the Start-coding sheet's "Resume previous session"
// toggle and the machines list.
final class WorktreeResumeTests: XCTestCase {
    private func worktree(
        id: String = "wt-1",
        deviceRowId: String = "row-1",
        identifier: String? = "EXP-42",
        agents: [String]? = ["claude"]
    ) -> DeviceWorktreeEntity {
        DeviceWorktreeEntity(
            id: id,
            deviceRowId: deviceRowId,
            repoFullName: "acme/api",
            branch: "exp/EXP-42",
            issueIdentifier: identifier,
            agents: agents.flatMap { list in
                String(data: try! JSONEncoder().encode(list), encoding: .utf8)
            }
        )
    }

    func testMatchesIdentifierCaseInsensitively() {
        let match = WorktreeResume.match(
            worktrees: [worktree()],
            deviceRowId: "row-1",
            issueIdentifier: "exp-42",
            agent: "claude"
        )
        XCTAssertEqual(match?.id, "wt-1")
    }

    func testNilAgentsMarkerAllowsAnyAgent() {
        let match = WorktreeResume.match(
            worktrees: [worktree(agents: nil)],
            deviceRowId: "row-1",
            issueIdentifier: "EXP-42",
            agent: "codex"
        )
        XCTAssertNotNil(match)
    }

    func testAgentMismatchRefuses() {
        XCTAssertNil(WorktreeResume.match(
            worktrees: [worktree(agents: ["claude"])],
            deviceRowId: "row-1",
            issueIdentifier: "EXP-42",
            agent: "codex"
        ))
    }

    func testWrongDeviceOrIdentifierRefuses() {
        XCTAssertNil(WorktreeResume.match(
            worktrees: [worktree()],
            deviceRowId: "row-2",
            issueIdentifier: "EXP-42",
            agent: "claude"
        ))
        XCTAssertNil(WorktreeResume.match(
            worktrees: [worktree()],
            deviceRowId: "row-1",
            issueIdentifier: "EXP-43",
            agent: "claude"
        ))
        XCTAssertNil(WorktreeResume.match(
            worktrees: [worktree(identifier: nil)],
            deviceRowId: "row-1",
            issueIdentifier: "EXP-42",
            agent: "claude"
        ))
        XCTAssertNil(WorktreeResume.match(
            worktrees: [worktree()],
            deviceRowId: nil,
            issueIdentifier: "EXP-42",
            agent: "claude"
        ))
    }

    // MARK: - DeviceQueries.compose

    private func entity(
        id: String,
        userId: String,
        deviceId: String,
        lastSeenAt: String?,
        sharedTeamId: String? = nil,
        kind: String = "server"
    ) -> DeviceEntity {
        DeviceEntity(
            id: id, userId: userId, deviceId: deviceId, label: deviceId,
            kind: kind, lastSeenAt: lastSeenAt, sharedTeamId: sharedTeamId
        )
    }

    func testComposeOrdersOwnThenSharedAndAttributesOwners() {
        let now = WireTimestamps.parse("2026-08-11T10:00:00.000Z")!
        let rows = [
            entity(id: "r1", userId: "me", deviceId: "old-box",
                   lastSeenAt: "2026-08-01T00:00:00Z"),
            entity(id: "r2", userId: "me", deviceId: "fresh-box",
                   lastSeenAt: "2026-08-11T09:59:50Z"),
            entity(id: "r3", userId: "mate", deviceId: "shared-box",
                   lastSeenAt: "2026-08-11T09:59:50Z", sharedTeamId: "team-1"),
            entity(id: "r4", userId: "mate", deviceId: "other-team-box",
                   lastSeenAt: "2026-08-11T09:59:50Z", sharedTeamId: "team-2"),
        ]
        let users = [
            UserEntity(id: "mate", name: "Mate", email: "m@example.com", image: nil,
                       createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z")
        ]
        let composed = DeviceQueries.compose(
            rows: rows, users: users, teamId: "team-1", userId: "me", now: now
        )
        XCTAssertEqual(composed.map(\.deviceId), ["fresh-box", "old-box", "shared-box"])
        // Own rows carry no owner (isMine); the shared one is attributed.
        XCTAssertTrue(composed[0].isMine)
        XCTAssertNil(composed[0].owner)
        XCTAssertEqual(composed[2].owner?.name, "Mate")
        XCTAssertFalse(composed[2].isMine)
        // Online derives from last_seen_at freshness; rowId joins worktrees.
        XCTAssertTrue(composed[0].isOnline)
        XCTAssertFalse(composed[1].isOnline)
        XCTAssertEqual(composed[0].rowId, "r2")
    }

    // EXP-623: online rows sort by label (heartbeats can't reorder them);
    // offline rows sit below, most recently seen first.
    func testComposeOrdersOnlineByLabelThenOfflineByLastSeen() {
        let now = WireTimestamps.parse("2026-08-11T10:00:00.000Z")!
        let rows = [
            // Freshest beat — would lead under last-seen ordering.
            entity(id: "r1", userId: "me", deviceId: "zeta",
                   lastSeenAt: "2026-08-11T09:59:59Z"),
            entity(id: "r2", userId: "me", deviceId: "Alpha",
                   lastSeenAt: "2026-08-11T09:58:40Z"),
            entity(id: "r3", userId: "me", deviceId: "aaa-stale",
                   lastSeenAt: "2026-08-11T08:00:00Z"),
            entity(id: "r4", userId: "me", deviceId: "zzz-recent",
                   lastSeenAt: "2026-08-11T09:50:00Z"),
        ]
        let composed = DeviceQueries.compose(
            rows: rows, users: [], teamId: nil, userId: "me", now: now
        )
        XCTAssertEqual(
            composed.map(\.deviceId),
            ["Alpha", "zeta", "zzz-recent", "aaa-stale"]
        )
    }

    // EXP-622: the flag is the ROW OWNER's preference — a teammate's shared
    // server must never prefill the caller's picker.
    func testComposeKeepsIsDefaultOnOwnRowsOnly() {
        let rows = [
            DeviceEntity(id: "r1", userId: "me", deviceId: "mine", label: "mine",
                         kind: "server", isDefault: true),
            DeviceEntity(id: "r2", userId: "mate", deviceId: "shared", label: "shared",
                         kind: "server", sharedTeamId: "team-1", isDefault: true),
        ]
        let composed = DeviceQueries.compose(
            rows: rows, users: [], teamId: "team-1", userId: "me"
        )
        XCTAssertEqual(composed.map(\.deviceId), ["mine", "shared"])
        XCTAssertTrue(composed[0].isDefaultDevice)
        XCTAssertFalse(composed[1].isDefaultDevice)
    }

    func testComposeWithoutTeamScopesToOwnRows() {
        let rows = [
            entity(id: "r1", userId: "me", deviceId: "mine", lastSeenAt: nil),
            entity(id: "r2", userId: "mate", deviceId: "shared", lastSeenAt: nil,
                   sharedTeamId: "team-1"),
        ]
        let composed = DeviceQueries.compose(
            rows: rows, users: [], teamId: nil, userId: "me"
        )
        XCTAssertEqual(composed.map(\.deviceId), ["mine"])
    }
}
