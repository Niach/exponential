import Foundation
import XCTest
@testable import ExpCore

// #EXP-1493 THE plan page, locked ×4 against
// `packages/domain-contract/fixtures/plan-page.json`: every `*Cases` array,
// the WHOLE expected value. Read through `#filePath`.
final class PlanPageTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/plan-page.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let cases = try XCTUnwrap(try fixture()[key] as? [[String: Any]], key)
        XCTAssertFalse(cases.isEmpty, key)
        return cases
    }

    private func num(_ raw: Any?) -> Double? { (raw as? NSNumber)?.doubleValue }

    private func same(_ actual: Any, _ expected: Any?, _ name: String) {
        XCTAssertTrue(
            NSArray(array: [actual]).isEqual(to: [expected ?? NSNull()]),
            "\(name)\nactual: \(actual)\nexpected: \(expected ?? "nil")"
        )
    }

    func testConstantsMatchTheFixture() throws {
        let f = try fixture()
        XCTAssertEqual(f["route"] as? String, PlanPage.route)
        let specimen = try XCTUnwrap(f["specimen"] as? [String: Any])
        XCTAssertEqual(specimen["chatName"] as? String, PlanPage.Specimen.chatName)
        XCTAssertEqual(specimen["activeSection"] as? String, PlanPage.Specimen.activeSection)
        XCTAssertEqual(
            PlanPage.outline((specimen["card"] as? [String: Any])?["plan"] as? String ?? "").map(\.id)
                .contains(PlanPage.Specimen.activeSection), true
        )
        let copy = try XCTUnwrap(f["copy"] as? [String: Any])
        XCTAssertEqual(copy["title"] as? String, PlanPage.Copy.title)
        XCTAssertEqual(copy["openPlan"] as? String, PlanPage.Copy.openPlan)
        XCTAssertEqual(copy["openFull"] as? String, PlanPage.Copy.openFull)
        XCTAssertEqual(copy["openChat"] as? String, PlanPage.Copy.openChat)
        XCTAssertEqual(copy["outline"] as? String, PlanPage.Copy.outline)
        XCTAssertEqual(copy["questions"] as? String, PlanPage.Copy.questions)
        XCTAssertEqual(copy["counts"] as? [String: [String]], [
            "sections": PlanPage.Copy.sections, "pictures": PlanPage.Copy.pictures,
            "questions": PlanPage.Copy.questionsCount,
        ])
        let layout = try XCTUnwrap(f["layout"] as? [String: Any])
        XCTAssertEqual(num(layout["outlineWidth"]), PlanPage.Layout.outlineWidth)
        XCTAssertEqual(num(layout["columnMaxWidth"]), PlanPage.Layout.columnMaxWidth)
        XCTAssertEqual(num(layout["railWidth"]), PlanPage.Layout.railWidth)
        XCTAssertEqual(
            layout["columnPadding"] as? [String: Double],
            ["wide": PlanPage.Layout.columnPaddingWide, "phone": PlanPage.Layout.columnPaddingPhone]
        )
        XCTAssertEqual(num(layout["headerHeight"]), PlanPage.Layout.headerHeight)
        XCTAssertEqual(num(layout["blockGap"]), PlanPage.Layout.blockGap)
        XCTAssertEqual(num(layout["outlineRowHeight"]), PlanPage.Layout.outlineRowHeight)
        XCTAssertEqual(num(layout["outlineActiveBar"]), PlanPage.Layout.outlineActiveBar)
        XCTAssertEqual(f["mosaic"] as? [String: Double], [
            "gap": PlanPage.Mosaic.gap, "largeFraction": PlanPage.Mosaic.largeFraction,
            "aspect": PlanPage.Mosaic.aspect, "radius": PlanPage.Mosaic.radius,
        ])
        XCTAssertEqual(f["testIds"] as? [String: String], [
            "page": PlanPage.TestIds.page, "outline": PlanPage.TestIds.outline,
            "outlineRow": PlanPage.TestIds.outlineRow, "document": PlanPage.TestIds.document,
            "rail": PlanPage.TestIds.rail, "mosaic": PlanPage.TestIds.mosaic,
            "carousel": PlanPage.TestIds.carousel, "openPlan": PlanPage.TestIds.openPlan,
            "outlineToggle": PlanPage.TestIds.outlineToggle,
        ])
    }

    func testBlockCases() throws {
        for entry in try cases("blockCases") {
            let name = entry["name"] as? String ?? "?"
            let actual = PlanPage.blocks(try XCTUnwrap(entry["markdown"] as? String, name)).map { block -> [String: Any] in
                switch block {
                case let .markdown(text): ["kind": "markdown", "text": text]
                case let .section(id, title): ["kind": "section", "id": id, "title": title]
                case let .pictures(pictures):
                    ["kind": "pictures", "pictures": pictures.map { ["alt": $0.alt, "src": $0.src] }]
                }
            }
            same(actual, entry["expected"], name)
        }
    }

    func testOutlineCases() throws {
        for entry in try cases("outlineCases") {
            let name = entry["name"] as? String ?? "?"
            let actual = PlanPage.outline(try XCTUnwrap(entry["markdown"] as? String, name)).map {
                ["id": $0.id, "title": $0.title, "pictures": $0.pictures] as [String: Any]
            }
            same(actual, entry["expected"], name)
        }
    }

    func testCountCases() throws {
        for entry in try cases("countCases") {
            let name = entry["name"] as? String ?? "?"
            let counts = PlanPage.counts((entry["card"] as? [String: Any]).map(TreeMessageCard.init(object:)))
            let actual: [String: Any] = [
                "sections": counts.sections, "pictures": counts.pictures, "questions": counts.questions,
            ]
            same(actual, entry["expected"], name)
        }
    }

    func testCountsLineCases() throws {
        for entry in try cases("countsLineCases") {
            let raw = try XCTUnwrap(entry["counts"] as? [String: Int])
            let line = PlanPage.countsLine(
                PlanPage.Counts(sections: raw["sections"] ?? 0, pictures: raw["pictures"] ?? 0, questions: raw["questions"] ?? 0)
            )
            XCTAssertEqual(line, entry["expected"] as? String)
        }
    }

    func testMosaicCases() throws {
        for entry in try cases("mosaicCases") {
            let count = try XCTUnwrap(entry["count"] as? Int)
            let actual = PlanPage.mosaicTiles(count).map {
                ["index": $0.index, "size": $0.size.rawValue, "more": $0.more] as [String: Any]
            }
            same(actual, entry["expected"], "mosaic \(count)")
        }
    }

    func testTargetCases() throws {
        for entry in try cases("targetCases") {
            let name = entry["name"] as? String ?? "?"
            let messages = try XCTUnwrap(entry["messages"] as? [[String: Any]], name).map { raw in
                PlanPage.Message(
                    id: raw["id"] as? String ?? "",
                    sessionId: raw["sessionId"] as? String ?? "",
                    authorSessionId: raw["authorSessionId"] as? String,
                    createdAt: num(raw["createdAt"]) ?? 0,
                    card: (raw["card"] as? [String: Any]).map(TreeMessageCard.init(object:))
                )
            }
            let actual = PlanPage.planPageFor(try XCTUnwrap(entry["sessionId"] as? String, name), messages)
            same(actual ?? NSNull(), entry["expected"], name)
        }
        XCTAssertEqual(
            PlanPage.path(teamSlug: "acme", sessionId: "s1", messageId: "m1"), "/t/acme/sessions/s1/plan/m1"
        )
    }
}
