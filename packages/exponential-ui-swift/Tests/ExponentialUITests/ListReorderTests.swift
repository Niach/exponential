import Foundation
import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// EXP-1832 (fixtures/list-reorder.json): a `reorderable` List's row drags
/// through the core (`listDrag` = what the long-press drag forwards): the
/// paint follows the pointer, the release fires `move {id, before | after |
/// into}` and its press is swallowed; a cancel fires nothing.
@MainActor
final class ListReorderTests: XCTestCase {
    private func model(_ host: RecordingHost) throws -> SurfaceModel {
        var options = SurfaceOptions()
        options.mode = .light
        let m = try SurfaceModel(id: "lr", options: options, host: host)
        m.setViewport(width: 300, height: 800)
        let tree: JSONValue = .object([
            "id": .string("root"), "component": .string("Box"), "children": .array([
                .object([
                    "id": .string("l"), "component": .string("List"),
                    "props": .object(["reorderable": .bool(true), "maxDepth": .number(1)]),
                    "template": .object(["component": .string("row"), "path": .string("/items"), "key": .string("id")]),
                    "on": .object(["move": .object(["event": .object(["name": .string("move")])])]),
                ]),
                .object([
                    "id": .string("row"), "component": .string("Box"),
                    "style": .object(["height": .number(30), "flexShrink": .number(0)]),
                    "on": .object(["press": .object(["event": .object(["name": .string("open")])])]),
                    "children": .array([.object(["id": .string("label"), "component": .string("Text"), "props": .object(["text": .object(["@path": .string("id")])])])]),
                ]),
            ]),
        ])
        _ = try m.setNested(json: tree.json)
        m.setData(path: "/items", value: .array(["a", "b", "c"].map { .object(["id": .string($0)]) }))
        return m
    }

    func testARowDragsAndTheReleaseFiresMove() throws {
        let host = RecordingHost()
        let m = try model(host)
        let a = try XCTUnwrap(m.index(of: "row.a"))
        let c = try XCTUnwrap(m.index(of: "row.c"))
        XCTAssertTrue(m.isReorderRow(a))
        XCTAssertFalse(m.isReorderRow(try XCTUnwrap(m.index(of: "label.a"))), "a node inside a row is no row")
        XCTAssertFalse(m.isReorderRow(try XCTUnwrap(m.index(of: "l"))))

        let fa = m.frame(a), fc = m.frame(c)
        m.listDrag(a, phase: "start", at: CGPoint(x: fa.minX + 10, y: fa.midY))
        m.listDrag(a, phase: "move", at: CGPoint(x: fa.minX + 40, y: fc.maxY - 4))
        let paint = try XCTUnwrap(m.listDragPaints.first, "a live drag paints")
        XCTAssertEqual(paint.key, "a")
        XCTAssertEqual(Int(paint.row), a)
        XCTAssertTrue(m.isListDragRow(a), "the dragged row dims")
        XCTAssertFalse(m.isListDragRow(c))
        XCTAssertEqual(paint.dropKey, "c")
        let position = try XCTUnwrap(paint.dropPosition)
        if position == "into" {
            XCTAssertNotNil(paint.into)
        } else {
            XCTAssertNotNil(paint.line, "before / after draw the line")
            XCTAssertNotNil(paint.ring, "and its ring")
        }
        XCTAssertEqual(CGFloat(paint.lift.x), fa.minX + CGFloat(listDragStyle().liftOffset), accuracy: 0.01, "the lifted copy sits right of the row")

        m.listDrag(a, phase: "end", at: CGPoint(x: fa.minX + 40, y: fc.maxY - 4))
        XCTAssertTrue(m.listDragPaints.isEmpty, "nothing painted after the drop")
        let move = try XCTUnwrap(host.actions.last)
        XCTAssertEqual(move.name, "move")
        let context = move.payload ?? move.context
        XCTAssertEqual(context["id"]?.string, "a")
        XCTAssertEqual(context[position]?.string, "c")

        // The release's press never opens the row.
        m.press(a)
        XCTAssertFalse(host.actions.contains { $0.name == "open" })
    }

    func testACancelFiresNothing() throws {
        let host = RecordingHost()
        let m = try model(host)
        let a = try XCTUnwrap(m.index(of: "row.a"))
        let fa = m.frame(a), fc = m.frame(try XCTUnwrap(m.index(of: "row.c")))
        m.listDrag(a, phase: "start", at: CGPoint(x: fa.minX + 10, y: fa.midY))
        m.listDrag(a, phase: "move", at: CGPoint(x: fa.minX + 10, y: fc.maxY - 4))
        XCTAssertFalse(m.listDragPaints.isEmpty)
        m.listDrag(a, phase: "cancel", at: nil)
        XCTAssertTrue(m.listDragPaints.isEmpty)
        XCTAssertTrue(host.actions.isEmpty, "a cancel fires nothing")
    }

    func testThePaintNumbersMatchTheFixture() throws {
        let s = listDragStyle()
        XCTAssertEqual(s.dim, 0.4, accuracy: 0.001)
        XCTAssertEqual(s.line, 2, accuracy: 0.001)
        XCTAssertEqual(s.ring, 8, accuracy: 0.001)
        XCTAssertEqual(s.liftOffset, 12, accuracy: 0.001)
        XCTAssertEqual(s.liftOpacity, 0.95, accuracy: 0.001)
        XCTAssertEqual(s.intoFill, 0.14, accuracy: 0.001)
    }
}
