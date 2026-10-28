import Foundation
import XCTest
@testable import ExpCore

// NEW-59: the work screen, locked ×4 (TS `work-screen.ts`, desktop
// `work_screen`, Android `WorkScreenTest`) against fixtures/work-screen.json:
// every `copy` key, the `testIds` and every `optionCases` entry (the gate,
// current and overlay cases are desktop-only). Plus the wire: the doctor's
// `displays` and `launchDefaults.computerUseDisplay` (null = Auto).
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
        let cases = try XCTUnwrap(try fixture()["optionCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let displays = (entry["displays"] as? [[String: Any]])?.map { raw in
                WorkScreen.Display(
                    number: raw["number"] as? Int ?? -1,
                    width: raw["width"] as? Int ?? -1,
                    height: raw["height"] as? Int ?? -1,
                    primary: raw["primary"] as? Bool ?? false
                )
            }
            let stored = entry["stored"] as? Int
            let expect = try XCTUnwrap(entry["expect"] as? [[String: Any]], name).map { raw in
                WorkScreen.Option(value: raw["value"] as? Int, label: raw["label"] as? String ?? "")
            }
            XCTAssertEqual(WorkScreen.options(displays: displays, stored: stored), expect, name)
        }
    }

    func testDoctorDecodesDisplaysLeniently() throws {
        let doctor = try XCTUnwrap(DeviceDoctor.decode(json: #"""
        {"items":[],"displays":[
          {"number":1,"width":2560,"height":1440,"primary":false},
          {"number":2,"width":"wide"},
          {"number":3,"width":3024,"height":1964,"primary":true}
        ]}
        """#))
        XCTAssertEqual(doctor.displays, [
            WorkScreen.Display(number: 1, width: 2560, height: 1440, primary: false),
            WorkScreen.Display(number: 3, width: 3024, height: 1964, primary: true),
        ])
        // Absent = not reported; a non-array degrades the key, not the report.
        XCTAssertNil(try XCTUnwrap(DeviceDoctor.decode(json: #"{"items":[]}"#)).displays)
        let odd = try XCTUnwrap(DeviceDoctor.decode(
            json: #"{"items":[{"key":"git","group":"required","state":"ok"}],"displays":7}"#
        ))
        XCTAssertNil(odd.displays)
        XCTAssertEqual(odd.items.count, 1)
    }

    func testComputerUseDisplayDecodesAndSurvivesTheRowRebuild() throws {
        let decode = { (raw: String) throws -> DeviceLaunchDefaults in
            try JSONDecoder().decode(DeviceLaunchDefaults.self, from: Data(raw.utf8))
        }
        XCTAssertEqual(try decode(#"{"computerUse":true,"computerUseDisplay":2}"#).computerUseDisplay, 2)
        XCTAssertNil(try decode(#"{"computerUse":true}"#).computerUseDisplay)
        XCTAssertNil(try decode(#"{"computerUseDisplay":null}"#).computerUseDisplay)
        let odd = try decode(#"{"defaultAgent":"codex","computerUseDisplay":"two"}"#)
        XCTAssertNil(odd.computerUseDisplay)
        XCTAssertEqual(odd.defaultAgent, "codex")

        let row = DeviceEntity(
            id: "row-1",
            userId: "me",
            deviceId: "box",
            label: "Box",
            launchDefaults: #"{"computerUse":true,"computerUseDisplay":3}"#,
            lastSeenAt: "2026-10-05T11:59:00.000Z"
        )
        XCTAssertEqual(SteerDevice(entity: row, currentUserId: "me").launchDefaults?.computerUseDisplay, 3)
    }

    /// Same carry-forward as `computerUse`: no key keeps the stored display,
    /// an explicit null clears it to Auto, a number picks it.
    func testComputerUseDisplayEncodesKeepAutoAndANumber() throws {
        func json(_ value: DeviceLaunchDefaultsInput) throws -> [String: Any] {
            let data = try JSONEncoder().encode(value)
            return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        }
        let kept = try json(DeviceLaunchDefaultsInput(agents: [:], computerUse: true))
        XCTAssertNil(kept.index(forKey: "computerUseDisplay"))
        XCTAssertEqual(Set(kept.keys), ["agents", "computerUse"])

        let auto = try json(DeviceLaunchDefaultsInput(agents: [:], computerUseDisplay: .init(nil)))
        XCTAssertNotNil(auto.index(forKey: "computerUseDisplay"))
        XCTAssertTrue(auto["computerUseDisplay"] is NSNull)

        let picked = try json(DeviceLaunchDefaultsInput(agents: [:], computerUseDisplay: .init(2)))
        XCTAssertEqual(picked["computerUseDisplay"] as? Int, 2)
    }
}
