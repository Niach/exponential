import Foundation
import XCTest
@testable import ExpCore

// EXP-481: the devices/device_worktrees shape rows (shapes 17/18) decode
// tolerantly — Electric delivers Postgres text scalars ("t"/"0"), jsonb
// columns arrive as objects off the wire but as pre-stringified JSON from
// fixtures, and EVERY non-PK field must survive absence (a required absent
// field silently drops rows forever — the attachments uploader_id lesson).
final class DeviceEntityDecodingTests: XCTestCase {
    private func decodeDevice(_ json: String) throws -> DeviceEntity {
        try JSONDecoder().decode(DeviceEntity.self, from: Data(json.utf8))
    }

    private func decodeWorktree(_ json: String) throws -> DeviceWorktreeEntity {
        try JSONDecoder().decode(DeviceWorktreeEntity.self, from: Data(json.utf8))
    }

    func testDecodesFullWireRowWithJsonbObjects() throws {
        // The Electric wire form: jsonb columns as real JSON values, ints as
        // Postgres text.
        let device = try decodeDevice("""
        {"id":"row-1","user_id":"u1","device_id":"dev-1","label":"buildbox",
        "kind":"server","platform":"linux","version":"0.9.0",
        "agents":["claude","codex"],"caps":["actions","resume","worktrees"],
        "unauthed_agents":["pi"],
        "launch_defaults":{"defaultAgent":"codex","agents":{"claude":{"model":"fable","ultracode":true}}},
        "launch_defaults_updated_at":"2026-08-10T10:00:00.000Z",
        "active_sessions":"2","last_seen_at":"2026-08-11T10:00:00.000Z",
        "shared_team_id":"team-1","update_requested_at":null,
        "created_at":"2026-08-01T00:00:00Z","updated_at":"2026-08-11T10:00:00Z"}
        """)
        XCTAssertEqual(device.id, "row-1")
        XCTAssertEqual(device.deviceId, "dev-1")
        XCTAssertEqual(device.activeSessions, 2)
        XCTAssertEqual(device.sharedTeamId, "team-1")
        XCTAssertNil(device.updateRequestedAt)
        // jsonb columns land as stored JSON text, type-faithfully — the
        // launchDefaults booleans must survive the round trip as real bools.
        let defaults = try JSONDecoder().decode(
            DeviceLaunchDefaults.self, from: Data(XCTUnwrap(device.launchDefaults).utf8)
        )
        XCTAssertEqual(defaults.defaultAgent, "codex")
        XCTAssertEqual(defaults.agents?["claude"]?.model, "fable")
        XCTAssertEqual(defaults.agents?["claude"]?.ultracode, true)
        let agents = try JSONDecoder().decode(
            [String].self, from: Data(XCTUnwrap(device.agents).utf8)
        )
        XCTAssertEqual(agents, ["claude", "codex"])
    }

    func testDecodesPreStringifiedJsonbAndAbsentOptionals() throws {
        // Fixture/partial form: jsonb as an already-stringified JSON string;
        // every optional column absent.
        let device = try decodeDevice("""
        {"id":"row-2","user_id":"u1","device_id":"dev-2","label":"laptop",
        "launch_defaults":"{\\"defaultAgent\\":\\"claude\\"}"}
        """)
        XCTAssertEqual(device.label, "laptop")
        XCTAssertNil(device.kind)
        XCTAssertNil(device.agents)
        XCTAssertEqual(device.activeSessions, 0)
        XCTAssertNil(device.lastSeenAt)
        let defaults = try JSONDecoder().decode(
            DeviceLaunchDefaults.self, from: Data(XCTUnwrap(device.launchDefaults).utf8)
        )
        XCTAssertEqual(defaults.defaultAgent, "claude")
    }

    func testDecodesWorktreeWithPostgresTextBool() throws {
        let worktree = try decodeWorktree("""
        {"id":"wt-1","device_row_id":"row-1","repo_full_name":"acme/api",
        "branch":"exp/EXP-42","issue_identifier":"EXP-42",
        "agents":["claude"],"dirty":"clean","busy":"t",
        "reported_at":"2026-08-11T10:00:00Z"}
        """)
        XCTAssertEqual(worktree.deviceRowId, "row-1")
        XCTAssertEqual(worktree.issueIdentifier, "EXP-42")
        XCTAssertTrue(worktree.busy)
        XCTAssertEqual(worktree.agentIds, ["claude"])
    }

    func testWorktreeAbsentOptionalsDefault() throws {
        let worktree = try decodeWorktree("""
        {"id":"wt-2","device_row_id":"row-1","repo_full_name":"acme/api",
        "branch":"exp/batch-a1b2c3d4"}
        """)
        XCTAssertNil(worktree.issueIdentifier)
        // nil marker = pre-marker worktree: any agent may resume — the
        // absent/empty distinction is load-bearing, never collapse it.
        XCTAssertNil(worktree.agentIds)
        XCTAssertFalse(worktree.busy)
        XCTAssertNil(worktree.dirty)
    }
}
