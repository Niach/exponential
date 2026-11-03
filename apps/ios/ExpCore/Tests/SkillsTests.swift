import Foundation
import XCTest
@testable import ExpCore

// #EXP-1661: team skills on the phone, locked ×4 against fixtures/skills.json
// (TS `skills.test.ts`): sort, preselect, the + menu value, the agent row
// value, and the copy + test ids the read-only pages show.
final class SkillsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/skills.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func skill(_ id: String, _ name: String, enabledByDefault: Bool = false) -> SkillEntity {
        SkillEntity(id: id, teamId: "t", name: name, enabledByDefault: enabledByDefault, createdAt: "x", updatedAt: "x")
    }

    func testSortCases() throws {
        let cases = try XCTUnwrap(try fixture()["sortCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let names = try XCTUnwrap(c["names"] as? [String])
            let sorted = Skills.sortSkills(names.map { skill($0, $0) })
            XCTAssertEqual(sorted.map(\.name), c["expected"] as? [String], c["name"] as? String ?? "")
        }
    }

    func testPreselectCases() throws {
        let cases = try XCTUnwrap(try fixture()["preselectCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let rows = try XCTUnwrap(c["skills"] as? [[String: Any]])
            let skills = rows.map {
                skill($0["id"] as? String ?? "", $0["name"] as? String ?? "",
                      enabledByDefault: $0["enabledByDefault"] as? Bool ?? false)
            }
            // JSON null = no saved pick.
            let saved = c["saved"] as? [String]
            XCTAssertEqual(
                Skills.preselectSkillIds(skills, saved: saved),
                c["expected"] as? [String],
                c["name"] as? String ?? ""
            )
        }
    }

    func testMenuValueCases() throws {
        let cases = try XCTUnwrap(try fixture()["menuValueCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let count = try XCTUnwrap(c["count"] as? Int)
            XCTAssertEqual(Skills.skillMenuValue(count), c["expected"] as? String, c["name"] as? String ?? "")
        }
    }

    func testAgentValueCases() throws {
        let cases = try XCTUnwrap(try fixture()["agentValueCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let names = try XCTUnwrap(c["names"] as? [String])
            XCTAssertEqual(Skills.agentSkillsValue(names), c["expected"] as? String, c["name"] as? String ?? "")
        }
    }

    func testCopyAndTestIdsMatchTheFixture() throws {
        let f = try fixture()
        XCTAssertEqual(Skills.title, f["title"] as? String)
        let list = try XCTUnwrap(f["list"] as? [String: Any])
        XCTAssertEqual(Skills.defaultPill, list["defaultPill"] as? String)
        let empty = try XCTUnwrap(list["empty"] as? [String: String])
        XCTAssertEqual(EmptyStateCopy.State.skills.title, empty["title"])
        let ids = try XCTUnwrap(list["testIds"] as? [String: String])
        XCTAssertEqual(Skills.TestId.page, ids["page"])
        XCTAssertEqual(Skills.TestId.list, ids["list"])
        XCTAssertEqual(Skills.TestId.row, ids["row"])
        let agentRow = try XCTUnwrap(f["agentRow"] as? [String: String])
        XCTAssertEqual(Skills.agentRowLabel, agentRow["label"])
        XCTAssertEqual(Skills.agentRowNone, agentRow["none"])
        let fields = try XCTUnwrap((f["page"] as? [String: Any])?["fields"] as? [[String: Any]])
        func field(_ id: String) -> [String: Any]? { fields.first { $0["id"] as? String == id } }
        XCTAssertEqual(Skills.Field.name.label, field("name")?["label"] as? String)
        XCTAssertEqual(Skills.Field.name.placeholder, field("name")?["placeholder"] as? String)
        XCTAssertEqual(Skills.Field.description.label, field("description")?["label"] as? String)
        XCTAssertEqual(Skills.Field.description.placeholder, field("description")?["placeholder"] as? String)
        XCTAssertEqual(Skills.Field.body.label, field("body")?["label"] as? String)
        XCTAssertEqual(Skills.Field.body.placeholder, field("body")?["placeholder"] as? String)
        let menuRow = try XCTUnwrap(f["menuRow"] as? [String: Any])
        let row = ComposerMenu.row(.skills)
        XCTAssertEqual(row.label, menuRow["label"] as? String)
        XCTAssertEqual(row.icon, menuRow["icon"] as? String)
    }

    func testSkillsGetDecodesTheEnvelopeRow() throws {
        let dto = try JSONDecoder().decode(SkillDto.self, from: Data(#"""
            {"id":"s1","teamId":"t1","name":"review-checklist","description":"Run it","enabledByDefault":true,
             "createdById":"u1","createdAt":"x","updatedAt":"x","body":"# Review"}
            """#.utf8))
        XCTAssertEqual(dto, SkillDto(
            id: "s1", teamId: "t1", name: "review-checklist", description: "Run it",
            enabledByDefault: true, body: "# Review"
        ))
    }
}
