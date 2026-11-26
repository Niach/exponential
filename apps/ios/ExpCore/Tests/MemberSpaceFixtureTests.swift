@testable import ExpCore
import Foundation
import XCTest

// #EXP-2161: packages/domain-contract/fixtures/member-space.json, the cases
// the phone reads: the copy, `optInCases`, `teamCases`, `sectionCases` and
// `listed` (`pinCases` + `rootMarkCases` replay in InstallTrustFixtureTests,
// `knownSpaces` in its eNet half).
final class MemberSpaceFixtureTests: XCTestCase {
    typealias Object = [String: Any]

    private func fixture() throws -> Object {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/member-space.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? Object)
    }

    private func cases(_ key: String) throws -> [Object] {
        try XCTUnwrap(try fixture()[key] as? [Object], key)
    }

    func testTheConstantsAndTheCopy() throws {
        let f = try fixture()
        XCTAssertEqual(MemberSpace.doctorKey, f["doctorKey"] as? String)
        XCTAssertEqual(MemberSpace.deviceIdPrefix, f["deviceIdPrefix"] as? String)
        let copy = try XCTUnwrap(f["copy"] as? [String: String])
        let ours: [String: String] = [
            "optIn": MemberSpace.Copy.optIn, "optInTitle": MemberSpace.Copy.optInTitle, "optInBody": MemberSpace.Copy.optInBody,
            "optInConfirm": MemberSpace.Copy.optInConfirm, "pickTitle": MemberSpace.Copy.pickTitle, "pickBody": MemberSpace.Copy.pickBody,
            "pickConfirm": MemberSpace.Copy.pickConfirm, "members": MemberSpace.Copy.members, "remove": MemberSpace.Copy.remove,
            "waiting": MemberSpace.Copy.waiting, "noDevice": MemberSpace.Copy.noDevice, "ended": MemberSpace.Copy.ended,
        ]
        XCTAssertEqual(ours, copy)
        XCTAssertEqual(MemberSpace.Copy.pickTitle(device: "unraid"), "Run your Assistant on unraid?")
        XCTAssertEqual(DeviceReadiness.labels[MemberSpace.doctorKey], MemberSpace.Copy.optIn)
    }

    func testOptInCases() throws {
        for c in try cases("optInCases") {
            let name = try XCTUnwrap(c["name"] as? String)
            let got = MemberSpace.mayOptIn(
                caller: try XCTUnwrap(c["caller"] as? String), allowed: try XCTUnwrap(c["allowed"] as? Bool),
                kind: c["kind"] as? String, shared: try XCTUnwrap(c["shared"] as? Bool), doctor: c["doctor"] as? String
            )
            XCTAssertEqual(got, try XCTUnwrap(c["expected"] as? Bool), name)
        }
    }

    func testTeamCases() throws {
        for c in try cases("teamCases") {
            let name = try XCTUnwrap(c["name"] as? String)
            let got = MemberSpace.spaceTeams(
                assistantTeamIds: try XCTUnwrap(c["assistantTeamIds"] as? [String]),
                hostOwnerTeamIds: try XCTUnwrap(c["hostOwnerTeamIds"] as? [String]),
                memberTeamIds: try XCTUnwrap(c["memberTeamIds"] as? [String])
            )
            XCTAssertEqual(got, try XCTUnwrap(c["expected"] as? [String]), name)
        }
    }

    private func devices(_ c: Object) throws -> [MemberSpace.Device] {
        try XCTUnwrap(c["devices"] as? [Object]).map { d in
            MemberSpace.Device(
                rowId: try XCTUnwrap(d["rowId"] as? String), deviceId: try XCTUnwrap(d["deviceId"] as? String),
                userId: try XCTUnwrap(d["userId"] as? String), label: try XCTUnwrap(d["label"] as? String),
                online: try XCTUnwrap(d["online"] as? Bool), assistantTeamIds: try XCTUnwrap(d["assistantTeamIds"] as? [String]),
                spaceHostId: d["spaceHostId"] as? String
            )
        }
    }

    func testSectionCases() throws {
        let all = try cases("sectionCases")
        XCTAssertGreaterThanOrEqual(all.count, 3)
        for c in all {
            let name = try XCTUnwrap(c["name"] as? String)
            let teams = try XCTUnwrap(c["teams"] as? [[String: String]]).map { (id: $0["id"] ?? "", name: $0["name"] ?? "") }
            let got = MemberSpace.spaceSections(
                devices: try devices(c), teams: teams, boundId: c["boundId"] as? String, me: try XCTUnwrap(c["me"] as? String)
            )
            let want = try XCTUnwrap(c["expected"] as? [Object]).map { s in
                MemberSpace.Section(
                    teamId: try XCTUnwrap(s["teamId"] as? String), title: try XCTUnwrap(s["title"] as? String),
                    rows: try XCTUnwrap(s["rows"] as? [Object]).map { r in
                        MemberSpace.SectionRow(
                            id: try XCTUnwrap(r["id"] as? String), label: try XCTUnwrap(r["label"] as? String),
                            disabled: try XCTUnwrap(r["disabled"] as? Bool), selected: try XCTUnwrap(r["selected"] as? Bool)
                        )
                    }
                )
            }
            XCTAssertEqual(got, want, name)
        }
    }

    /// `listed`: a space row is never a device, in any composed list.
    func testASpaceIsNeverListed() throws {
        let c = try XCTUnwrap(try cases("sectionCases").first)
        let rows = try devices(c).map { d in
            DeviceEntity(
                id: d.rowId, userId: d.userId, deviceId: d.deviceId, label: d.label, kind: "server",
                sharedTeamIds: d.assistantTeamIds, assistantTeamIds: d.assistantTeamIds, spaceHostId: d.spaceHostId
            )
        }
        XCTAssertEqual(DeviceEntity.listed(rows).map(\.deviceId), ["mac", "srv-b", "srv-a", "srv-c"])
        let composed = DeviceQueries.compose(rows: rows, users: [], teamIds: ["t1", "t2"], userId: "u1")
        XCTAssertFalse(composed.contains { $0.deviceId == "space-1" })
        XCTAssertEqual(composed.first { $0.deviceId == "srv-a" }?.assistantTeamIds, ["t1", "t2"])
    }

    func testTheOptInRow() throws {
        let ok = try JSONDecoder().decode(DeviceDoctor.self, from: Data(#"{"items":[{"key":"member_space","group":"x","state":"ok"}]}"#.utf8))
        let missing = try JSONDecoder().decode(
            DeviceDoctor.self, from: Data(#"{"items":[{"key":"member_space","group":"x","state":"missing","detail":"No user namespaces"}]}"#.utf8)
        )
        XCTAssertEqual(MemberSpace.optInRow(checked: false, kind: "server", doctor: ok).disabled, false)
        XCTAssertNil(MemberSpace.optInRow(checked: false, kind: "server", doctor: ok).detail)
        XCTAssertEqual(MemberSpace.optInRow(checked: false, kind: "server", doctor: missing).detail, "No user namespaces")
        XCTAssertTrue(MemberSpace.optInRow(checked: false, kind: "server", doctor: missing).disabled)
        // Off is always one press.
        XCTAssertFalse(MemberSpace.optInRow(checked: true, kind: "server", doctor: missing).disabled)
        XCTAssertEqual(MemberSpace.optInRow(checked: false, kind: "server", doctor: nil).detail, DeviceReadiness.unavailable)
    }
}
