import Foundation
import XCTest
@testable import ExpCore

// #EXP-1645: the team mark ×4, replayed against
// packages/domain-contract/fixtures/team-image.json (TS `team-image.test.ts`,
// desktop `domain::team_image`, Android `TeamImageTest`).
final class TeamImageTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/team-image.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testConstants() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture["copy"] as? [String: String], [
            "row": TeamImage.Copy.row, "upload": TeamImage.Copy.upload, "remove": TeamImage.Copy.remove,
        ])
        XCTAssertEqual(fixture["testIds"] as? [String: String], [
            "row": TeamImage.TestIds.row, "upload": TeamImage.TestIds.upload, "remove": TeamImage.TestIds.remove,
        ])
        let mark = try XCTUnwrap(fixture["mark"] as? [String: Any])
        XCTAssertEqual(mark["radiusFraction"] as? Double, TeamImage.MarkStyle.radiusFraction)
        XCTAssertEqual(mark["glyph"] as? String, TeamImage.MarkStyle.glyph)
        let row = try XCTUnwrap(fixture["row"] as? [String: Any])
        XCTAssertEqual(row["markSize"] as? Double, TeamImage.Row.markSize)
        let upload = try XCTUnwrap(fixture["upload"] as? [String: Any])
        XCTAssertEqual(upload["path"] as? String, TeamImage.Upload.path)
        XCTAssertEqual(upload["maxBytes"] as? Int, TeamImage.Upload.maxBytes)
        XCTAssertEqual(upload["side"] as? Int, TeamImage.Upload.side)
        XCTAssertEqual(upload["accept"] as? [String], TeamImage.Upload.accept)
    }

    func testMarkCases() throws {
        let cases = try XCTUnwrap(try fixture()["markCases"] as? [[String: Any]])
        XCTAssertEqual(cases.count, 5)
        for item in cases {
            let iconUrl = item["iconUrl"] as? String
            let mark = try XCTUnwrap(item["mark"] as? [String: String])
            let expected: TeamImage.Mark = mark["kind"] == "image"
                ? .image(src: try XCTUnwrap(mark["src"]))
                : .glyph
            XCTAssertEqual(TeamImage.mark(iconUrl: iconUrl), expected, "\(iconUrl ?? "null")")
        }
    }

    func testUploadPath() {
        XCTAssertEqual(TeamImage.uploadPath(teamId: "t1"), "/api/teams/t1/icon")
    }
}

final class UpdateTeamInputIconUrlTests: XCTestCase {
    private func json(_ input: UpdateTeamInput) throws -> [String: Any] {
        try XCTUnwrap(try JSONSerialization.jsonObject(with: JSONEncoder().encode(input)) as? [String: Any])
    }

    // #EXP-1645: nil = unchanged (absent), `.some(nil)` = JSON null (Remove).
    func testIconUrlEncoding() throws {
        XCTAssertFalse(try json(UpdateTeamInput(teamId: "t")).keys.contains("iconUrl"))
        let cleared = try json(UpdateTeamInput(teamId: "t", iconUrl: .some(nil)))
        XCTAssertTrue(cleared.keys.contains("iconUrl"))
        XCTAssertTrue(cleared["iconUrl"] is NSNull)
    }
}
