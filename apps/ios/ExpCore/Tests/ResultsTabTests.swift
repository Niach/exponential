import Foundation
import XCTest
@testable import ExpCore

// #FEED-191 the Results tab, locked ×4 against
// `packages/domain-contract/fixtures/results-tab.json`: every case, the WHOLE
// expected array. Read through `#filePath`: the unit-test bundle carries no
// repo resources.
final class ResultsTabTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/results-tab.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func num(_ raw: Any?) -> Double? { (raw as? NSNumber)?.doubleValue }

    private func json(_ result: ResultsTab.Result) -> [String: Any] {
        func orNull(_ value: Any?) -> Any { value ?? NSNull() }
        return [
            "id": result.id,
            "sessionId": result.sessionId,
            "sessionName": result.sessionName,
            "createdAt": result.createdAt,
            "age": result.age,
            "title": result.title,
            "description": orNull(result.description),
            "pictures": result.pictures.map { picture -> [String: Any] in
                [
                    "label": picture.label,
                    "attachmentId": picture.attachmentId,
                    "width": orNull(picture.width),
                    "height": orNull(picture.height),
                    "caption": orNull(picture.caption),
                ]
            },
            "surface": result.surface,
        ]
    }

    func testConstantsMatchTheFixture() throws {
        let f = try fixture()
        XCTAssertEqual(f["copy"] as? [String: String], ["now": ResultsTab.Copy.now, "untitled": ResultsTab.Copy.untitled])
        let tab = try XCTUnwrap(f["tab"] as? [String: Any])
        XCTAssertEqual(tab["copy"] as? [String: String], [
            "label": ResultsTab.Tab.Copy.label, "empty": ResultsTab.Tab.Copy.empty,
            "changes": ResultsTab.Tab.Copy.changes, "stack": ResultsTab.Tab.Copy.stack,
        ])
        XCTAssertEqual(tab["testIds"] as? [String: String], [
            "tab": ResultsTab.Tab.TestIds.tab, "list": ResultsTab.Tab.TestIds.list,
            "section": ResultsTab.Tab.TestIds.section, "changes": ResultsTab.Tab.TestIds.changes,
            "stack": ResultsTab.Tab.TestIds.stack,
        ])
        let tile = try XCTUnwrap(tab["tile"] as? [String: Any])
        XCTAssertEqual((tile["thumbHeight"] as? [String: Double])?["phone"], ResultsTab.Tab.Tile.thumbHeight)
        XCTAssertEqual((tile["thumbMaxWidth"] as? [String: Double])?["phone"], ResultsTab.Tab.Tile.thumbMaxWidth)
        XCTAssertEqual(num(tile["gap"]), ResultsTab.Tab.Tile.gap)
        XCTAssertEqual(num(tile["textGap"]), ResultsTab.Tab.Tile.textGap)
        XCTAssertEqual(num(tile["radius"]), ResultsTab.Tab.Tile.radius)
        XCTAssertEqual(num(tile["listGap"]), ResultsTab.Tab.Tile.listGap)
        XCTAssertEqual(num(tile["leadGap"]), ResultsTab.Tab.Tile.leadGap)
        XCTAssertEqual(num((tab["specimen"] as? [String: Any])?["case"]).map(Int.init), ResultsTab.Tab.specimenCase)
    }

    func testCases() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let name = entry["name"] as? String ?? "?"
            let input = try XCTUnwrap(entry["input"] as? [String: Any], name)
            let sessions = try XCTUnwrap(input["sessions"] as? [[String: Any]], name).map { raw in
                ResultsTab.SessionInput(
                    id: raw["id"] as? String ?? "",
                    rootId: raw["rootId"] as? String ?? "",
                    name: raw["name"] as? String ?? ""
                )
            }
            let messages = try XCTUnwrap(input["messages"] as? [[String: Any]], name).map { raw in
                ResultsTab.MessageInput(
                    id: raw["id"] as? String ?? "",
                    sessionId: raw["sessionId"] as? String ?? "",
                    authorSessionId: raw["authorSessionId"] as? String,
                    createdAt: num(raw["createdAt"]) ?? 0,
                    card: (raw["card"] as? [String: Any]).map(TreeMessageCard.init(object:))
                )
            }
            let actual = ResultsTab.results(
                now: try XCTUnwrap(num(input["now"]), name),
                rootId: try XCTUnwrap(input["rootId"] as? String, name),
                sessions: sessions,
                messages: messages
            ).map(json)
            let expected = try XCTUnwrap(entry["expected"] as? [Any], name)
            XCTAssertTrue(
                NSArray(array: actual).isEqual(to: expected),
                "\(name)\nactual: \(actual)\nexpected: \(expected)"
            )
        }
    }
}
