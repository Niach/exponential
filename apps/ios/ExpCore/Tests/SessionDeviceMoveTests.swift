import Foundation
import XCTest
@testable import ExpCore

// EXP-1252: moving a session to another device — the rules and copy
// hand-mirrored ×4 (web `lib/session-device-move.ts`, desktop
// `crates/ui/src/device_move.rs`, Android `domain/SessionDeviceMove.kt`).
final class SessionDeviceMoveTests: XCTestCase {
    private func device(
        _ id: String,
        label: String? = nil,
        online: Bool = true,
        agents: [String]? = ["claude"],
        shared: [String] = [],
        version: String? = nil
    ) -> SteerDevice {
        SteerDevice(
            deviceId: id,
            deviceLabel: label ?? id,
            agents: agents,
            online: online,
            version: version,
            sharedTeamIds: shared
        )
    }

    func testCopyIsTheContract() {
        XCTAssertEqual(SessionDeviceMove.pickerTitle, "Move to device")
        XCTAssertEqual(
            SessionDeviceMove.note,
            "The session ends here and continues on that device on the same branch; its conversation goes with it, logins stay on each device."
        )
        XCTAssertEqual(SessionDeviceMove.reasonSteer, "Only people who can steer this session can move it.")
        XCTAssertEqual(SessionDeviceMove.reasonEnded, "This session has ended.")
        XCTAssertEqual(SessionDeviceMove.reasonAgent, "Agents move from Settings → Agents.")
        XCTAssertEqual(SessionDeviceMove.reasonOffline, "This device is offline.")
        XCTAssertEqual(
            SessionDeviceMove.reasonVisibility,
            "This device is shared differently, so the session can't move here."
        )
        XCTAssertEqual(SessionDeviceMove.reasonNoLogin, "This device has no signed-in login for this agent.")
        XCTAssertEqual(SessionDeviceMove.failureToast, "Couldn't move the session")
    }

    func testPinHiddenForAgents() {
        XCTAssertTrue(SessionDeviceMove.pinVisible(kind: "thread"))
        XCTAssertFalse(SessionDeviceMove.pinVisible(kind: "agent"))
    }

    func testPinBlockedSteerBeforeEnded() {
        XCTAssertNil(SessionDeviceMove.blockedReason(canSteer: true, ended: false))
        XCTAssertEqual(SessionDeviceMove.blockedReason(canSteer: false, ended: true), SessionDeviceMove.reasonSteer)
        XCTAssertEqual(SessionDeviceMove.blockedReason(canSteer: true, ended: true), SessionDeviceMove.reasonEnded)
    }

    func testRowReasonsInOrder() {
        let reason = { (d: SteerDevice, shared: Bool) in
            SessionDeviceMove.rowBlockedReason(d, sessionTeamId: "t1", sessionShared: shared, agent: "claude")
        }
        // Offline wins over every other refusal.
        XCTAssertEqual(reason(device("a", online: false, agents: [], shared: ["t1"]), false), SessionDeviceMove.reasonOffline)
        // A private session cannot land on a team-shared device, nor back.
        XCTAssertEqual(reason(device("b", agents: [], shared: ["t1"]), false), SessionDeviceMove.reasonVisibility)
        XCTAssertEqual(reason(device("c"), true), SessionDeviceMove.reasonVisibility)
        // Shared with ANOTHER team only = unshared for this session.
        XCTAssertNil(reason(device("d", shared: ["t2"]), false))
        // No version gate: an old or unreported build is never refused for
        // it (the CLIENT_MIN_VERSION floor does that job).
        XCTAssertNil(reason(device("d2", version: "0.14.72"), false))
        XCTAssertNil(reason(device("d3", version: nil), false))
        // No signed-in login for the session's agent.
        XCTAssertEqual(reason(device("e", agents: ["codex"]), false), SessionDeviceMove.reasonNoLogin)
        XCTAssertNil(reason(device("f", shared: ["t1"]), true))
        // No agent known = no login check.
        XCTAssertNil(SessionDeviceMove.rowBlockedReason(
            device("g", agents: []), sessionTeamId: "t1", sessionShared: false, agent: nil
        ))
    }

    func testRowsSelectTheCurrentDeviceAndNeverRefuseIt() {
        let rows = SessionDeviceMove.rows(
            devices: [device("here", online: false), device("there"), device("cold", online: false)],
            currentDeviceId: "here",
            sessionTeamId: "t1",
            sessionShared: false,
            agent: "claude"
        )
        XCTAssertEqual(rows.map(\.device.deviceId), ["here", "there", "cold"])
        XCTAssertEqual(rows.map(\.current), [true, false, false])
        XCTAssertEqual(rows.map(\.blockedReason), [nil, nil, SessionDeviceMove.reasonOffline])
    }

    /// A shared session running on a teammate's UNSHARED box: its device is
    /// not in the list, so a bare current row (named by its id, never
    /// refused) leads — the row a pending move is cancelled on.
    func testRowsSynthesiseTheMissingCurrentDevice() {
        let rows = SessionDeviceMove.rows(
            devices: [device("there"), device("cold", online: false)],
            currentDeviceId: "ghost",
            sessionTeamId: "t1",
            sessionShared: false,
            agent: "claude"
        )
        XCTAssertEqual(rows.map(\.device.deviceId), ["ghost", "there", "cold"])
        XCTAssertEqual(rows.map(\.device.deviceLabel), ["ghost", "there", "cold"])
        XCTAssertEqual(rows.map(\.current), [true, false, false])
        XCTAssertEqual(rows.map(\.blockedReason), [nil, nil, SessionDeviceMove.reasonOffline])
        XCTAssertNil(rows[0].device.kind)
        XCTAssertNil(rows[0].device.icon)
        // No current device at all = nothing to synthesise.
        let none = SessionDeviceMove.rows(
            devices: [device("there")], currentDeviceId: "", sessionTeamId: "t1", sessionShared: false, agent: nil
        )
        XCTAssertEqual(none.map(\.device.deviceId), ["there"])
        XCTAssertEqual(none.map(\.current), [false])
    }

    /// NEW-8: nothing is pending on the row: another device moves, the
    /// current one does nothing.
    func testPickMovesOrDoesNothing() {
        XCTAssertEqual(SessionDeviceMove.pick(deviceId: "b", currentDeviceId: "a"), .move("b"))
        XCTAssertEqual(SessionDeviceMove.pick(deviceId: "a", currentDeviceId: "a"), .none)
    }

    func testSendInputIsSessionIdPlusDeviceId() throws {
        XCTAssertEqual(
            SessionDeviceMove.sendInput(sessionId: "s-1", pick: .move("b")),
            .moveDevice(sessionId: "s-1", deviceId: "b")
        )
        XCTAssertNil(SessionDeviceMove.sendInput(sessionId: "s-1", pick: .none))

        let data = try JSONEncoder().encode(SessionsSendInput.moveDevice(sessionId: "s-1", deviceId: "b"))
        let wire = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        XCTAssertEqual(wire.count, 2)
        XCTAssertEqual(wire["sessionId"] as? String, "s-1")
        XCTAssertEqual(wire["deviceId"] as? String, "b")
    }

    /// NEW-8: the legacy `move_to_device_id` is ignored, present or not.
    func testSessionRowToleratesMoveToDeviceId() throws {
        let json = """
            {"id":"s1","team_id":"t1","user_id":"u1","device_id":"a",
             "created_at":"2026-10-17T00:00:00Z","updated_at":"2026-10-17T00:00:00Z",
             "move_to_device_id":"b"}
            """
        let row = try JSONDecoder().decode(SessionEntity.self, from: Data(json.utf8))
        XCTAssertEqual(row.deviceId, "a")
    }
}
