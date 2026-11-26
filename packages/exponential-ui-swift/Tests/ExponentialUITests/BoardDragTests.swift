import Foundation
import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// genUI round 4 (fixtures/board-drop.json, Compose's BoardDragTest): a
/// Board card drags between its columns through the core (`listDrag`): the
/// paint moves to the List under the pointer (its path draws on top), the
/// release fires the TARGET column's `move` with the macro's `column`.
@MainActor
final class BoardDragTests: XCTestCase {
    private func model(_ host: RecordingHost) throws -> SurfaceModel {
        var options = SurfaceOptions()
        options.mode = .light
        let m = try SurfaceModel(id: "bd", options: options, host: host)
        m.fixedMeasure = true
        m.setViewport(width: 1000, height: 700)
        let tree: JSONValue = .object([
            "id": .string("root"), "component": .string("Box"), "children": .array([
                .object([
                    "id": .string("b"), "component": .string("Board"),
                    "props": .object(["columns": .object(["@path": .string("/cols")])]),
                    "on": .object(["move": .object(["event": .object(["name": .string("move")])])]),
                ]),
            ]),
        ])
        _ = try m.setNested(json: tree.json)
        func card(_ v: String) -> JSONValue { .object(["label": .string("Card \(v)"), "value": .string(v)]) }
        func col(_ v: String, _ cards: [String]) -> JSONValue { .object(["value": .string(v), "title": .string(v), "cards": .array(cards.map(card))]) }
        m.setData(path: "/cols", value: .array([col("todo", ["a", "b"]), col("doing", ["d"]), col("done", [])]))
        return m
    }

    func testACardDropsInAnotherColumn() throws {
        let host = RecordingHost()
        let m = try model(host)
        let a = try XCTUnwrap(m.index(of: "b.column.cards.card.todo.a"))
        let d = m.frame(try XCTUnwrap(m.index(of: "b.column.cards.card.doing.d")))
        let doing = try XCTUnwrap(m.index(of: "b.column.cards.doing"))
        let fa = m.frame(a)
        m.listDrag(a, phase: "start", at: CGPoint(x: fa.minX + 10, y: fa.midY))
        m.listDrag(a, phase: "move", at: CGPoint(x: d.minX + 30, y: d.minY + 4))
        let paint = try XCTUnwrap(m.listDragPaints.first)
        XCTAssertEqual(Int(paint.listIndex), doing, "the paint moves to the column under the pointer")
        XCTAssertTrue(m.isListDragRow(a), "the dragged card dims")
        XCTAssertTrue(m.isListDragPath(try XCTUnwrap(m.index(of: "b.column.doing"))), "the target column draws on top")
        XCTAssertEqual(paint.dropKey, "d")
        m.listDrag(a, phase: "end", at: CGPoint(x: d.minX + 30, y: d.minY + 4))
        let move = try XCTUnwrap(host.actions.last)
        XCTAssertEqual(move.name, "move")
        XCTAssertEqual(move.context["column"]?.string, "doing")
        XCTAssertEqual(move.context["id"]?.string, "a")
        XCTAssertEqual(move.context["before"]?.string, "d")
    }

    func testAnEmptyColumnTakesTheCard() throws {
        let host = RecordingHost()
        let m = try model(host)
        let a = try XCTUnwrap(m.index(of: "b.column.cards.card.todo.a"))
        let done = m.frame(try XCTUnwrap(m.index(of: "b.column.cards.done")))
        let fa = m.frame(a)
        m.listDrag(a, phase: "start", at: CGPoint(x: fa.minX + 10, y: fa.midY))
        m.listDrag(a, phase: "move", at: CGPoint(x: done.minX + 20, y: done.minY + 6))
        XCTAssertNotNil(m.listDragPaints.first?.into, "the into highlight")
        m.listDrag(a, phase: "end", at: CGPoint(x: done.minX + 20, y: done.minY + 6))
        let move = try XCTUnwrap(host.actions.last)
        XCTAssertEqual(move.context["column"]?.string, "done")
        XCTAssertNil(move.context["before"])
    }
}

/// genUI round 4: fixtures/map-tile.json against `SurfaceModel.imageSource`.
@MainActor
final class MapTileTests: XCTestCase {
    private final class MapHost: HostPlugin {
        let src: String
        let resolved: String?
        init(src: String, resolved: String?) { self.src = src; self.resolved = resolved }
        func resolveUrl(_ src: String) -> String { src == self.src ? (resolved ?? src) : src }
    }

    func testEveryCaseReplays() throws {
        try XCTSkipUnless(Fixtures.available())
        for c in try Fixtures.json("map-tile.json")["cases"]?.array ?? [] {
            let src = try XCTUnwrap(c["src"]?.string)
            let host = MapHost(src: src, resolved: c["resolveUrl"]?.string)
            let m = try SurfaceModel(id: "map", options: SurfaceOptions(), host: host)
            let got: String
            switch m.imageSource(src, alt: "Map") {
            case .load: got = "load"
            case let .text(t): got = t.isEmpty ? "nothing" : "other"
            default: got = "other"
            }
            XCTAssertEqual(got, c["paints"]?.string, c["name"]?.string ?? "")
        }
    }
}
