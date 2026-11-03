import Foundation
import XCTest
@testable import ExpCore

// VAPP-201: the team time zone ×4, replayed against
// packages/domain-contract/fixtures/team-zone.json (TS `team-zone.test.ts`,
// desktop `domain::team_zone`, Android `TeamZoneTest`).
final class TeamZoneTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/team-zone.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testCopyAndTestIds() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture["copy"] as? [String: String], ["row": TeamZone.Copy.row, "device": TeamZone.Copy.device])
        XCTAssertEqual(fixture["testIds"] as? [String: String], ["row": TeamZone.TestIds.row])
    }

    func testZoneCases() throws {
        let cases = try XCTUnwrap(try fixture()["zoneCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for item in cases {
            let zone = try XCTUnwrap(item["zone"] as? String)
            let ms = try XCTUnwrap(item["ms"] as? NSNumber).doubleValue
            let offset = try XCTUnwrap(item["offset"] as? Int)
            XCTAssertEqual(TeamZone.resolveZone(zone), zone, zone)
            XCTAssertEqual(TeamZone.zoneOffset(zone, ms: ms), offset, "\(zone) at \(item["at"] ?? "")")
        }
    }

    func testResolveCases() throws {
        let cases = try XCTUnwrap(try fixture()["resolveCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for item in cases {
            let name = try XCTUnwrap(item["name"] as? String)
            XCTAssertEqual(TeamZone.resolveZone(item["teamZone"] as? String), item["zone"] as? String, name)
        }
    }

    func testHostTimeZone() {
        XCTAssertEqual(TeamZone.hostTimeZone("Europe/Vienna").identifier, "Europe/Vienna")
        XCTAssertEqual(TeamZone.hostTimeZone(nil), .current)
        XCTAssertEqual(TeamZone.hostTimeZone("Mars/Olympus_Mons"), .current)
    }
}

final class UpdateTeamInputTimeZoneTests: XCTestCase {
    private func json(_ input: UpdateTeamInput) throws -> [String: Any] {
        try XCTUnwrap(try JSONSerialization.jsonObject(with: JSONEncoder().encode(input)) as? [String: Any])
    }

    // VAPP-201: nil = unchanged (absent), `.some(nil)` = JSON null (clear).
    func testTimeZoneEncoding() throws {
        XCTAssertNil(try json(UpdateTeamInput(teamId: "t"))["timeZone"])
        XCTAssertEqual(try json(UpdateTeamInput(teamId: "t", timeZone: .some("Europe/Vienna")))["timeZone"] as? String, "Europe/Vienna")
        let cleared = try json(UpdateTeamInput(teamId: "t", timeZone: .some(nil)))
        XCTAssertTrue(cleared.keys.contains("timeZone"))
        XCTAssertTrue(cleared["timeZone"] is NSNull)
    }
}

final class TeamZoneDayTests: XCTestCase {
    // Dogfood G: the records month view's `today` is the team zone's day.
    func testDayInZone() throws {
        let at = Date(timeIntervalSince1970: 1_793_704_500) // 2026-11-03T11:15:00Z
        XCTAssertEqual(TeamZone.day(at, in: try XCTUnwrap(TimeZone(identifier: "Pacific/Kiritimati"))), "2026-11-04")
        XCTAssertEqual(TeamZone.day(at, in: try XCTUnwrap(TimeZone(identifier: "UTC"))), "2026-11-03")
    }
}
