import Foundation
import XCTest
@testable import ExpCore

// EXP-1196/1218/1219: the device readiness block, fixture-locked ×5 against
// `packages/domain-contract/fixtures/device-doctor.json`: groups, labels,
// states, actions, and per case the rendered rows, the runnable agents, the
// local primary pill and the pills a REMOTE device offers (a phone is always
// remote). Change the fixture, change every client.
final class DeviceReadinessTests: XCTestCase {
    private struct Fixture: Decodable {
        let groups: [FixtureGroup]
        let labels: [String: String]
        let states: [String: FixtureState]
        let actions: [String: FixtureAction]
        let copy: [String: String]
        let cases: [FixtureCase]
    }

    private struct FixtureGroup: Decodable {
        let key: String
        let label: String
        let tag: String?
    }

    private struct FixtureState: Decodable {
        let glyph: String
        let tone: String
    }

    private struct FixtureAction: Decodable {
        let label: String
        let remote: Bool
    }

    private struct FixtureCase: Decodable {
        let name: String
        let doctor: DeviceDoctor
        let runnable: [String]
        let localPrimary: String?
        let remotePills: [String: String]
        /// Agent → the ambient login's email the `import` pill offers (on
        /// the device and, with `agent-import`, on another one). Absent = none.
        let importPills: [String: String]?
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/device-doctor.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testLocksTheTables() throws {
        let f = try fixture()
        XCTAssertEqual(DeviceReadiness.groups.map(\.key), f.groups.map(\.key))
        XCTAssertEqual(DeviceReadiness.groups.map(\.label), f.groups.map(\.label))
        XCTAssertEqual(DeviceReadiness.groups.map(\.tag), f.groups.map(\.tag))
        // The band renders the tag verbatim: lowercase, never "Optional".
        XCTAssertEqual(Set(DeviceReadiness.groups.compactMap(\.tag)), ["optional"])
        XCTAssertEqual(DeviceReadiness.labels, f.labels)
        XCTAssertEqual(Set(DeviceReadiness.states.keys), Set(f.states.keys))
        for (key, state) in f.states {
            XCTAssertEqual(DeviceReadiness.states[key]?.glyph.rawValue, state.glyph, key)
            XCTAssertEqual(DeviceReadiness.states[key]?.tone.rawValue, state.tone, key)
        }
        XCTAssertEqual(Set(DeviceReadiness.actions.keys), Set(f.actions.keys))
        for (key, action) in f.actions {
            XCTAssertEqual(DeviceReadiness.actions[key]?.label, action.label, key)
            XCTAssertEqual(DeviceReadiness.actions[key]?.remote, action.remote, key)
        }
        // The Import confirm and the duplicate toast, byte-identical ×4.
        XCTAssertEqual(DeviceReadiness.importTitle(email: "{email}"), f.copy["importTitle"])
        XCTAssertEqual(DeviceReadiness.importBody, f.copy["importBody"])
        XCTAssertEqual(
            AgentAccountsRows.alreadyAddedToast(
                AgentAccountsRows.LoginLanding(profileId: "p", email: "{email}", duplicate: true)
            ),
            f.copy["alreadyAdded"]
        )
        XCTAssertEqual(DeviceReadiness.actions[DeviceReadiness.importAction]?.label, "Import")
    }

    func testRendersEveryCase() throws {
        let f = try fixture()
        XCTAssertFalse(f.cases.isEmpty)
        for c in f.cases {
            let groups = DeviceReadiness.groups(c.doctor, remote: false)
            // Groups in fixture order, only those the report names, with tags.
            let named = Set(c.doctor.items.map(\.group))
            let expectedGroups = f.groups.filter { named.contains($0.key) }
            XCTAssertEqual(groups.map(\.key), expectedGroups.map(\.key), c.name)
            XCTAssertEqual(groups.map(\.label), expectedGroups.map(\.label), c.name)
            XCTAssertEqual(groups.map(\.tag), expectedGroups.map(\.tag), c.name)

            let rows = groups.flatMap(\.rows)
            let offParents = Set(c.doctor.items.filter { $0.state == "off" }.map(\.key))
            let expectedItems = c.doctor.items.filter { item in
                guard let parent = item.parent else { return true }
                return !offParents.contains(parent)
            }
            XCTAssertEqual(Set(rows.map(\.key)), Set(expectedItems.map(\.key)), c.name)
            for item in expectedItems {
                guard let row = rows.first(where: { $0.key == item.key }) else { continue }
                XCTAssertEqual(row.label, f.labels[item.key], "\(c.name) \(item.key)")
                XCTAssertEqual(row.indented, item.parent != nil, "\(c.name) \(item.key)")
                if item.key == "computer_use" {
                    // The switch row: label + switch, no glyph, no detail.
                    XCTAssertTrue(row.isSwitch, c.name)
                    XCTAssertNil(row.glyph, c.name)
                    XCTAssertNil(row.detail, c.name)
                    XCTAssertNil(row.action, c.name)
                    XCTAssertEqual(row.switchOn, item.state != "off", c.name)
                } else {
                    XCTAssertFalse(row.isSwitch, c.name)
                    XCTAssertEqual(row.detail, item.detail, "\(c.name) \(item.key)")
                    XCTAssertEqual(row.glyph?.rawValue, f.states[item.state]?.glyph, "\(c.name) \(item.key)")
                    XCTAssertEqual(row.tone?.rawValue, f.states[item.state]?.tone, "\(c.name) \(item.key)")
                    // On the device itself every action is offered.
                    XCTAssertEqual(row.action, item.action, "\(c.name) \(item.key)")
                    XCTAssertEqual(
                        row.actionLabel, item.action.flatMap { f.actions[$0]?.label },
                        "\(c.name) \(item.key)"
                    )
                }
            }
            // One filled pill, on the fixture's local primary.
            XCTAssertEqual(rows.filter(\.primary).map(\.key), c.localPrimary.map { [$0] } ?? [], c.name)

            // Another device (a phone): only remote actions get a pill.
            let remoteRows = DeviceReadiness.groups(c.doctor, remote: true).flatMap(\.rows)
            var pills: [String: String] = [:]
            for row in remoteRows { if let action = row.action { pills[row.key] = action } }
            XCTAssertEqual(pills, c.remotePills, c.name)
            XCTAssertLessThanOrEqual(remoteRows.filter(\.primary).count, 1, c.name)

            // The import pill: on the device, and on another device that
            // declares `agent-import` — never on one that does not.
            let expectedImports = c.importPills ?? [:]
            func imports(_ rows: [DeviceReadiness.Row]) -> [String: String] {
                var out: [String: String] = [:]
                for row in rows { if let email = row.importEmail { out[row.key] = email } }
                return out
            }
            XCTAssertEqual(imports(rows), expectedImports, c.name)
            XCTAssertEqual(imports(remoteRows), expectedImports, c.name)
            XCTAssertEqual(
                imports(DeviceReadiness.groups(c.doctor, remote: true, canImport: false).flatMap(\.rows)),
                [:],
                c.name
            )

            XCTAssertEqual(DeviceReadiness.runnableAgents(c.doctor), c.runnable, c.name)
        }
    }

    func testComposerShowsOnlyTheFailingRow() throws {
        let f = try fixture()
        for c in f.cases {
            for agent in ["claude", "codex"] {
                let row = DeviceReadiness.failingRow(c.doctor, agent: agent, remote: true)
                if c.runnable.contains(agent) {
                    XCTAssertNil(row, "\(c.name) \(agent)")
                    continue
                }
                let gitFails = c.doctor.items.contains { $0.key == "git" && $0.state != "ok" }
                XCTAssertEqual(row?.key, gitFails ? "git" : agent, "\(c.name) \(agent)")
                // Same row, same (remote-filtered) action as the block.
                let blockRow = DeviceReadiness.groups(c.doctor, remote: true)
                    .flatMap(\.rows).first { $0.key == row?.key }
                XCTAssertEqual(row?.action, blockRow?.action, "\(c.name) \(agent)")
                XCTAssertEqual(row?.detail, blockRow?.detail, "\(c.name) \(agent)")
            }
        }
    }

    func testSwitchDraftOverridesTheReport() throws {
        let doctor = try fixture().cases[1].doctor
        let off = DeviceReadiness.groups(doctor, remote: true, computerUseOn: false)
        let cu = off.first { $0.key == "computer_use" }
        XCTAssertEqual(cu?.rows.map(\.key), ["computer_use"])
        XCTAssertEqual(cu?.rows.first?.switchOn, false)
        let on = DeviceReadiness.groups(doctor, remote: true, computerUseOn: true)
        XCTAssertEqual(
            on.first { $0.key == "computer_use" }?.rows.map(\.key),
            ["computer_use", "screen_recording", "accessibility"]
        )
    }

    func testAttentionRows() throws {
        let f = try fixture()
        XCTAssertEqual(DeviceReadiness.attentionRows(f.cases[0].doctor, remote: true).map(\.key), [])
        XCTAssertEqual(
            DeviceReadiness.attentionRows(f.cases[1].doctor, remote: true).map(\.key),
            ["claude", "accessibility"]
        )
        XCTAssertEqual(
            DeviceReadiness.attentionRows(f.cases[2].doctor, remote: true).map(\.key),
            ["git", "claude"]
        )
    }

    // Lenient decode: unknown keys/states/groups survive as strings, a bad
    // item drops alone, a NULL column / non-object is nil.
    func testDecodesLeniently() {
        XCTAssertNil(DeviceDoctor.decode(json: nil))
        XCTAssertNil(DeviceDoctor.decode(json: "null"))
        XCTAssertNil(DeviceDoctor.decode(json: "[1]"))
        let doctor = DeviceDoctor.decode(json: """
            {"checkedAt":"2026-10-05T19:00:00.000Z","future":1,"items":[
              {"key":"git","group":"required","state":"ok","detail":"2.55.0","extra":true},
              {"group":"agents","state":"ok"},
              {"key":"gemini","group":"agents","state":"sleeping","action":"wake"},
              {"key":"gpu","group":"hardware","state":"ok"}
            ]}
            """)
        XCTAssertEqual(doctor?.items.map(\.key), ["git", "gemini", "gpu"])
        guard let doctor else { return }
        let groups = DeviceReadiness.groups(doctor, remote: false)
        XCTAssertEqual(groups.map(\.key), ["required", "agents", "hardware"])
        XCTAssertEqual(groups.last?.label, "hardware")
        let gemini = groups[1].rows[0]
        XCTAssertEqual(gemini.label, "gemini")
        XCTAssertEqual(gemini.glyph, .dash)
        XCTAssertEqual(gemini.tone, .muted)
        XCTAssertEqual(gemini.actionLabel, "wake")
        // An unknown action is never offered remotely.
        XCTAssertNil(DeviceReadiness.groups(doctor, remote: true)[1].rows[0].action)
    }

    func testDeviceRowCarriesTheReport() {
        let entity = DeviceEntity(
            id: "row-1", userId: "u1", deviceId: "d1", label: "Mac",
            doctor: #"{"checkedAt":null,"items":[{"key":"git","group":"required","state":"ok"}]}"#
        )
        let device = SteerDevice(entity: entity, currentUserId: "u1")
        XCTAssertEqual(device.doctor?.items.map(\.key), ["git"])
        let bare = SteerDevice(
            entity: DeviceEntity(id: "row-2", userId: "u1", deviceId: "d2", label: "Old"),
            currentUserId: "u1"
        )
        XCTAssertNil(bare.doctor)
    }

    func testWireRowDecodesTheJsonbObject() throws {
        let json = """
            {"id":"row-1","user_id":"u1","device_id":"d1","label":"Mac",
             "doctor":{"checkedAt":"2026-10-05T19:00:00.000Z","items":[{"key":"git","group":"required","state":"ok"}]}}
            """
        let entity = try JSONDecoder().decode(DeviceEntity.self, from: Data(json.utf8))
        XCTAssertEqual(DeviceDoctor.decode(json: entity.doctor)?.items.first?.key, "git")
        let null = try JSONDecoder().decode(
            DeviceEntity.self,
            from: Data(#"{"id":"r","user_id":"u","device_id":"d","doctor":null}"#.utf8)
        )
        XCTAssertNil(null.doctor)
    }
}
