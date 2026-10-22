import Foundation
import XCTest
@testable import ExpCore

// N-12: THE Dock, locked ×4 against `packages/domain-contract/fixtures/dock.json`
// — every case array, the WHOLE expected object.
final class DockTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/dock.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func list(_ key: String) throws -> [[String: Any]] {
        try XCTUnwrap(try fixture()[key] as? [[String: Any]])
    }

    private func number(_ value: Any?) throws -> Double {
        try XCTUnwrap((value as? NSNumber)?.doubleValue)
    }

    private func source(_ raw: [String: Any]) throws -> Dock.Source {
        switch raw["kind"] as? String {
        case "plan":
            return .plan(
                id: try XCTUnwrap(raw["id"] as? String), text: try XCTUnwrap(raw["text"] as? String),
                sessionName: raw["sessionName"] as? String, questions: Int(try number(raw["questions"])),
                createdAt: try number(raw["createdAt"])
            )
        case "question":
            return .question(
                id: try XCTUnwrap(raw["id"] as? String), text: try XCTUnwrap(raw["text"] as? String),
                sessionName: raw["sessionName"] as? String, createdAt: try number(raw["createdAt"])
            )
        case "tasks":
            guard let queue = raw["queue"] as? [String: Any] else { return .tasks(nil) }
            let todo = try XCTUnwrap(queue["todo"] as? [String: Any])
            return .tasks(Dock.QueueSummary(
                todoTitle: try XCTUnwrap(todo["title"] as? String),
                items: try XCTUnwrap(todo["items"] as? [[String: Any]]).map {
                    Dock.QueueSummary.Item(label: $0["label"] as? String ?? "", description: $0["description"] as? String)
                },
                completedTitle: (queue["completed"] as? [String: Any])?["title"] as? String
            ))
        case "suggestions":
            return .suggestions(
                id: try XCTUnwrap(raw["id"] as? String), label: try XCTUnwrap(raw["label"] as? String),
                caption: raw["caption"] as? String, title: try XCTUnwrap(raw["title"] as? String),
                open: Int(try number(raw["open"]))
            )
        case "queued":
            return .queued(try XCTUnwrap(raw["messages"] as? [[String: Any]]).map { $0["text"] as? String ?? "" })
        default:
            return .banner(
                id: try XCTUnwrap(raw["id"] as? String), icon: try XCTUnwrap(raw["icon"] as? String),
                label: try XCTUnwrap(raw["label"] as? String), text: try XCTUnwrap(raw["text"] as? String),
                tone: try XCTUnwrap((raw["tone"] as? String).flatMap(Dock.Tint.init(rawValue:))),
                action: raw["action"] as? String
            )
        }
    }

    private func row(_ r: [String: Any]) throws -> Dock.Row {
        Dock.Row(
            id: try XCTUnwrap(r["id"] as? String),
            kind: try XCTUnwrap((r["kind"] as? String).flatMap(Dock.Kind.init(rawValue:))),
            tint: try XCTUnwrap((r["tint"] as? String).flatMap(Dock.Tint.init(rawValue:))),
            icon: try XCTUnwrap(r["icon"] as? String),
            label: try XCTUnwrap(r["label"] as? String),
            caption: r["caption"] as? String,
            meta: r["meta"] as? String,
            title: try XCTUnwrap(r["title"] as? String),
            expandable: try XCTUnwrap(r["expandable"] as? Bool),
            action: r["action"] as? String
        )
    }

    func testConstantsMatchTheFixture() throws {
        let f = try fixture()
        let copy = try XCTUnwrap(f["copy"] as? [String: Any])
        XCTAssertEqual(copy["queued"] as? [String: String], ["one": Dock.Copy.queuedOne, "many": Dock.Copy.queuedMany])
        XCTAssertEqual(
            copy["questions"] as? [String: String], ["one": Dock.Copy.questionsOne, "many": Dock.Copy.questionsMany]
        )
        XCTAssertEqual(copy["close"] as? String, Dock.Copy.close)
        XCTAssertEqual(copy["showAll"] as? String, Dock.Copy.showAll)
        XCTAssertEqual(f["icons"] as? [String: String], [
            "plan": Dock.Icons.plan, "question": Dock.Icons.question, "tasks": Dock.Icons.tasks,
            "queued": Dock.Icons.queued, "open": Dock.Icons.open, "close": Dock.Icons.close,
            "suggestions": Dock.Icons.suggestions,
        ])
        XCTAssertEqual(f["testIds"] as? [String: String], [
            "dock": Dock.TestIds.dock, "pile": Dock.TestIds.pile, "card": Dock.TestIds.card,
            "sheet": Dock.TestIds.sheet, "sheetBody": Dock.TestIds.sheetBody,
            "sheetFooter": Dock.TestIds.sheetFooter, "close": Dock.TestIds.close,
        ])
        let stack = try XCTUnwrap(f["stack"] as? [String: Any])
        XCTAssertEqual(
            Dock.stack,
            StackConstants(
                peek: try number(stack["peek"]), scaleStep: try number(stack["scaleStep"]),
                visible: Int(try number(stack["visible"])), gap: try number(stack["gap"])
            )
        )
        let sheet = try XCTUnwrap(f["sheet"] as? [String: Any])
        XCTAssertEqual(try number(sheet["maxHeightFraction"]), Dock.sheetMaxHeightFraction)
        XCTAssertEqual(try number(sheet["radius"]), Dock.sheetRadius)
        XCTAssertEqual(f["order"] as? [String], Dock.order)
    }

    func testRowCases() throws {
        let all = try list("rowCases")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let got = Dock.rows(try XCTUnwrap(c["sources"] as? [[String: Any]]).map(source))
            let expected = try XCTUnwrap(c["expected"] as? [[String: Any]]).map(row)
            XCTAssertEqual(got, expected, c["name"] as? String ?? "")
        }
    }

    /// `suggestionSources`: the rows (any type) in, the WHOLE sources out.
    func testSuggestionCases() throws {
        let all = try list("suggestionCases")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let rows = try XCTUnwrap(c["rows"] as? [[String: Any]]).compactMap { r -> Dock.SuggestionInput? in
                guard r["type"] as? String == "suggestions" else { return nil }
                return Dock.SuggestionInput(
                    id: r["id"] as? String ?? "",
                    header: r["header"] as? String ?? "",
                    items: (r["items"] as? [[String: Any]] ?? []).map {
                        (state: $0["state"] as? String ?? "", title: $0["title"] as? String ?? "")
                    }
                )
            }
            let expected = try XCTUnwrap(c["expected"] as? [[String: Any]]).map(source)
            XCTAssertEqual(Dock.suggestionSources(rows), expected, c["name"] as? String ?? "")
        }
    }

    func testCases() throws {
        let all = try list("cases")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = Dock.view(
                rows: try XCTUnwrap(input["rows"] as? [[String: Any]]).map(row),
                expanded: try XCTUnwrap(input["expanded"] as? Bool),
                openId: input["openId"] as? String
            )
            let e = try XCTUnwrap(c["expected"] as? [String: Any])
            let expected = Dock.View(
                mode: try XCTUnwrap((e["mode"] as? String).flatMap(Dock.Mode.init(rawValue:))),
                rows: try XCTUnwrap(e["rows"] as? [[String: Any]]).map(row),
                frontId: e["frontId"] as? String,
                openId: e["openId"] as? String,
                hidden: Int(try number(e["hidden"]))
            )
            XCTAssertEqual(got, expected, c["name"] as? String ?? "")
        }
    }

    func testAutoOpenCases() throws {
        let all = try list("autoOpenCases")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let rows = try XCTUnwrap(c["rowIds"] as? [[String]]).map { pair in
                (id: pair[0], kind: Dock.Kind(rawValue: pair[1])!)
            }
            let got = Dock.autoOpen(previous: Set(try XCTUnwrap(c["previous"] as? [String])), rows: rows)
            XCTAssertEqual(got, c["expected"] as? String, c["name"] as? String ?? "")
        }
    }

    /// The pile shares the toast's geometry with its own gap.
    func testPileLayout() {
        let pile = Dock.layout(heights: [40, 40, 40, 40], expanded: false)
        XCTAssertEqual(pile.height, 40 + 14 * 3)
        XCTAssertEqual(pile.items.map(\.scale), [0.9, 0.9, 0.95, 1])
        XCTAssertEqual(pile.items.map(\.visible), [false, true, true, true])
        let fan = Dock.layout(heights: [40, 40], expanded: true)
        XCTAssertEqual(fan.height, 86)
        XCTAssertEqual(fan.items.map(\.offset), [0, 46])
    }
}
