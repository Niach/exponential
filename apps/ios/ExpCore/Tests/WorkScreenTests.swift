import Foundation
import XCTest
@testable import ExpCore

// NEW-59: the work screen, locked ×4 (TS `work-screen.ts`, desktop
// `work_screen`, Android `WorkScreenTest`) against fixtures/work-screen.json:
// every `copy` key, the `testIds` and every `optionCases` entry (the gate,
// current and overlay cases are desktop-only). Plus the wire: the doctor's
// `displays` and `launchDefaults.computerUseDisplayId` (null = Auto; #FEED-161
// a stable display id, the number key is gone).
final class WorkScreenTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/work-screen.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testCopyAndTestIdsMatchTheFixture() throws {
        let fixture = try fixture()
        var copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        copy.removeValue(forKey: "refusals")  // desktop-only
        XCTAssertEqual(copy as? [String: String], [
            "row": WorkScreen.Copy.row,
            "auto": WorkScreen.Copy.auto,
            "display": WorkScreen.Copy.display,
            "primary": WorkScreen.Copy.primary,
            "notConnected": WorkScreen.Copy.notConnected,
        ])
        XCTAssertEqual(fixture["testIds"] as? [String: String], ["row": WorkScreen.TestIds.row])
    }

    func testOptionCases() throws {
        try replayOptions("optionCases")
    }

    /// #EXP-1514: the agent space options (`spaceOffered` per case).
    func testSpaceOptionCases() throws {
        try replayOptions("spaceOptionCases")
    }

    private func replayOptions(_ key: String) throws {
        let cases = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let displays = (entry["displays"] as? [[String: Any]])?.map { raw in
                WorkScreen.Display(
                    id: raw["id"] as? String ?? "?",
                    number: raw["number"] as? Int ?? -1,
                    width: raw["width"] as? Int ?? -1,
                    height: raw["height"] as? Int ?? -1,
                    primary: raw["primary"] as? Bool ?? false
                )
            }
            let stored = entry["stored"] as? String
            let expect = try XCTUnwrap(entry["expect"] as? [[String: Any]], name).map { raw in
                WorkScreen.Option(value: raw["value"] as? String, label: raw["label"] as? String ?? "")
            }
            let offered = entry["spaceOffered"] as? Bool ?? false
            XCTAssertEqual(
                WorkScreen.options(displays: displays, stored: stored, spaceOffered: offered), expect, name
            )
        }
    }

    /// #EXP-1514: the `space` words, and `spaceKeyCases` as iOS binds them:
    /// the picker's tag IS the stored value (no picker keys), so each case's
    /// value is exactly one option of an offering device's picker, and the
    /// space's key is its value.
    func testSpaceWordsAndKeyCases() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture["space"] as? [String: String], [
            "value": WorkScreen.Space.value,
            "label": WorkScreen.Space.label,
            "only": WorkScreen.Space.only,
        ])
        let keys = try XCTUnwrap(fixture["pickerKeys"] as? [String: String])
        XCTAssertEqual(keys["space"], WorkScreen.Space.value)
        let cases = try XCTUnwrap(fixture["spaceKeyCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        let ids = cases.compactMap { $0["value"] as? String }.filter { $0 != WorkScreen.Space.value }
        let displays = ids.enumerated().map { index, id in
            WorkScreen.Display(id: id, number: index + 1, width: 2560, height: 1440, primary: index == 0)
        }
        let options = WorkScreen.options(displays: displays, stored: nil, spaceOffered: true)
        for entry in cases {
            let value = entry["value"] as? String
            XCTAssertEqual(options.filter { $0.value == value }.count, 1, "\(entry["key"] ?? "?")")
        }
        XCTAssertEqual(options.first { $0.value == WorkScreen.Space.value }?.label, WorkScreen.Space.only)
    }

    /// #EXP-1514 (device-doctor.json): only an `agent_space` item in state
    /// `ok` offers a space.
    func testSpaceOfferedReadsTheAgentSpaceRow() {
        func doctor(_ space: String?) -> DeviceDoctor {
            var items = [
                DeviceDoctor.Item(key: "git", group: "required", state: "ok"),
                DeviceDoctor.Item(key: "computer_use", group: "computer_use", state: "ok"),
            ]
            if let space {
                items.append(DeviceDoctor.Item(
                    key: "agent_space", group: "computer_use", parent: "computer_use",
                    label: "Agent space", state: space
                ))
            }
            return DeviceDoctor(items: items)
        }
        XCTAssertTrue(WorkScreen.spaceOffered(doctor("ok")))
        XCTAssertFalse(WorkScreen.spaceOffered(doctor("missing")))
        XCTAssertFalse(WorkScreen.spaceOffered(doctor(nil)))
        XCTAssertFalse(WorkScreen.spaceOffered(DeviceDoctor(items: [])))
        XCTAssertFalse(WorkScreen.spaceOffered(nil))
    }

    func testDoctorDecodesDisplaysLeniently() throws {
        let doctor = try XCTUnwrap(DeviceDoctor.decode(json: #"""
        {"items":[],"displays":[
          {"id":"uuid-a","number":1,"width":2560,"height":1440,"primary":false},
          {"id":"uuid-b","number":2,"width":"wide"},
          {"number":3,"width":1920,"height":1080,"primary":false},
          {"id":"uuid-d","number":4,"width":3024,"height":1964,"primary":true}
        ]}
        """#))
        XCTAssertEqual(doctor.displays, [
            WorkScreen.Display(id: "uuid-a", number: 1, width: 2560, height: 1440, primary: false),
            WorkScreen.Display(id: "uuid-d", number: 4, width: 3024, height: 1964, primary: true),
        ])
        // #FEED-161: an entry without an `id` drops alone. Absent = not reported; a non-array degrades the key, not the report.
        XCTAssertNil(try XCTUnwrap(DeviceDoctor.decode(json: #"{"items":[]}"#)).displays)
        let odd = try XCTUnwrap(DeviceDoctor.decode(
            json: #"{"items":[{"key":"git","group":"required","state":"ok"}],"displays":7}"#
        ))
        XCTAssertNil(odd.displays)
        XCTAssertEqual(odd.items.count, 1)
    }

    func testComputerUseDisplayIdDecodesAndSurvivesTheRowRebuild() throws {
        let decode = { (raw: String) throws -> DeviceLaunchDefaults in
            try JSONDecoder().decode(DeviceLaunchDefaults.self, from: Data(raw.utf8))
        }
        XCTAssertEqual(
            try decode(#"{"computerUse":true,"computerUseDisplayId":"uuid-b"}"#).computerUseDisplayId,
            "uuid-b"
        )
        XCTAssertNil(try decode(#"{"computerUse":true}"#).computerUseDisplayId)
        XCTAssertNil(try decode(#"{"computerUseDisplayId":null}"#).computerUseDisplayId)
        // The #1295 number key is gone (no legacy): it reads as Auto.
        XCTAssertNil(try decode(#"{"computerUseDisplay":2}"#).computerUseDisplayId)
        let odd = try decode(#"{"defaultAgent":"codex","computerUseDisplayId":2}"#)
        XCTAssertNil(odd.computerUseDisplayId)
        XCTAssertEqual(odd.defaultAgent, "codex")

        let row = DeviceEntity(
            id: "row-1",
            userId: "me",
            deviceId: "box",
            label: "Box",
            launchDefaults: #"{"computerUse":true,"computerUseDisplayId":"uuid-c"}"#,
            lastSeenAt: "2026-10-05T11:59:00.000Z"
        )
        XCTAssertEqual(SteerDevice(entity: row, currentUserId: "me").launchDefaults?.computerUseDisplayId, "uuid-c")
    }

    /// Same carry-forward as `computerUse`: no key keeps the stored display,
    /// an explicit null clears it to Auto, an id picks it.
    func testComputerUseDisplayIdEncodesKeepAutoAndAnId() throws {
        func json(_ value: DeviceLaunchDefaultsInput) throws -> [String: Any] {
            let data = try JSONEncoder().encode(value)
            return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        }
        let kept = try json(DeviceLaunchDefaultsInput(agents: [:], computerUse: true))
        XCTAssertNil(kept.index(forKey: "computerUseDisplayId"))
        XCTAssertEqual(Set(kept.keys), ["agents", "computerUse"])

        let auto = try json(DeviceLaunchDefaultsInput(agents: [:], computerUseDisplayId: .init(nil)))
        XCTAssertNotNil(auto.index(forKey: "computerUseDisplayId"))
        XCTAssertTrue(auto["computerUseDisplayId"] is NSNull)

        let picked = try json(DeviceLaunchDefaultsInput(agents: [:], computerUseDisplayId: .init("uuid-b")))
        XCTAssertEqual(picked["computerUseDisplayId"] as? String, "uuid-b")
        XCTAssertNil(picked.index(forKey: "computerUseDisplay"))
    }
}
