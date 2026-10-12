import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// EXP-1259 (the UI batch): the SearchList through the model's field keys
/// (↑/↓ move the core's highlighted row, Enter / Tab pick), a searchable
/// submenu's ← at the caret's start, the pending Button's width, Skeleton
/// rows, the Tabs close glyph and the feedback timing constants.
@MainActor
final class BatchUITests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    private func model(_ tree: String, host: HostPlugin? = nil) throws -> SurfaceModel {
        let host = host ?? NoHost()
        var options = SurfaceOptions()
        options.theme = ThemeHandle.builtin("neutral")
        let m = try SurfaceModel(id: "batch", options: options, host: host)
        try m.setNested(json: tree)
        m.setViewport(width: 400, height: 800)
        return m
    }

    private func selected(_ m: SurfaceModel, _ id: String) -> Bool {
        m.index(of: id).flatMap { m.node($0) }?.selected ?? false
    }

    func testTheSearchListFiltersMovesAndPicksFromItsField() async throws {
        let host = RecordingHost()
        let m = try model(#"{"id": "root", "component": "Box", "children": [{"id": "sl", "component": "SearchList", "props": {"items": [{"label": "Ship the search list", "value": "a", "identifier": "EXP-12"}, {"label": "Spinner sizes", "value": "b"}, {"label": "Spin the mark", "value": "c"}]}, "on": {"select": {"event": {"name": "pick"}}}}]}"#, host: host)
        let field = try XCTUnwrap(m.index(of: "sl.search"))
        XCTAssertTrue(try XCTUnwrap(m.node(field)).isTextField)
        m.fieldFocused(field, true)
        m.fieldEdited(field, text: "spin")
        try await eventually { m.index(of: "sl.item.0") == nil }
        XCTAssertTrue(selected(m, "sl.item.1"), "the top row is highlighted")
        XCTAssertTrue(m.fieldKey(field, key: "down"))
        XCTAssertTrue(selected(m, "sl.item.2"))
        XCTAssertTrue(m.fieldKey(field, key: "up"))
        XCTAssertTrue(m.fieldKey(field, key: "down"))
        m.fieldCommitted(field)
        XCTAssertEqual(host.actions.last?.name, "pick")
        XCTAssertEqual(host.actions.last?.context["value"], .string("c"))
        XCTAssertNotNil(m.index(of: "sl.check.2"), "the picked row wears the check")
        XCTAssertFalse(m.fieldKey(field, key: "pageup"), "other keys stay the field's")
    }

    func testTabPicksAndLeftAtTheStartLeavesASearchableSubmenu() throws {
        let host = RecordingHost()
        let m = try model(#"{"id": "root", "component": "Box", "children": [{"id": "m", "component": "Menu", "props": {"label": "Add", "items": [{"label": "Implement issue", "kind": "submenu", "searchable": true, "items": [{"label": "One", "value": "1"}, {"label": "Two", "value": "2"}]}]}, "on": {"select": {"event": {"name": "pick"}}}}]}"#, host: host)
        m.press(id: "m.trigger")
        m.press(id: "m.item.0")
        let field = try XCTUnwrap(m.index(of: "m.search.0"))
        XCTAssertEqual(m.layers.count, 2)
        XCTAssertTrue(m.fieldKey(field, key: "leftAtStart"))
        XCTAssertEqual(m.layers.count, 1, "back to the parent menu")
        m.press(id: "m.item.0")
        let again = try XCTUnwrap(m.index(of: "m.search.0"))
        XCTAssertTrue(m.fieldKey(again, key: "tab"))
        XCTAssertEqual(host.actions.last?.context["value"], .string("1"))
    }

    func testAPendingButtonNeverNarrows() throws {
        let m = try model(#"{"id": "root", "component": "Box", "style": {"display": "flex", "flexDirection": "column", "alignItems": "flex-start"}, "children": [{"id": "idle", "component": "Button", "props": {"label": "Start run", "icon": "ui-add"}}, {"id": "busy", "component": "Button", "props": {"label": "Start run", "icon": "ui-add", "loading": true, "pendingLabel": "Starting"}}, {"id": "long", "component": "Button", "props": {"label": "Go", "loading": true, "pendingLabel": "Starting the run"}}]}"#)
        let w = { (id: String) in m.frame(m.index(of: id)!).width }
        XCTAssertGreaterThanOrEqual(w("busy"), w("idle") - 0.5)
        XCTAssertGreaterThan(w("long"), m.frame(m.index(of: "long")!).height * 2, "the pending word fits")
    }

    func testSkeletonRowsAndTheHoverCloseGlyph() throws {
        let m = try model(#"{"id": "root", "component": "Box", "children": [{"id": "sk", "component": "Skeleton", "props": {"rows": 2, "rowHeight": 44}}, {"id": "t", "component": "Tabs", "props": {"tabs": [{"label": "One", "value": "1"}], "closable": "hover", "addable": true}, "children": [{"id": "p", "component": "Text", "props": {"text": "1"}}]}]}"#)
        XCTAssertEqual(m.frame(m.index(of: "sk.row.1")!).height, 44)
        XCTAssertNotNil(m.index(of: "sk.bar.1.1"))
        XCTAssertNotNil(m.index(of: "t.new"))
        let close = try XCTUnwrap(m.index(of: "t.close.0"))
        XCTAssertEqual(m.style(close).opacity ?? 1, 0, accuracy: 0.001, "hidden until its tab is hovered")
    }

    /// The mirrored timing equals `catalog/layout.json` (generated).
    func testFeedbackTimingIsTheLayoutJson() throws {
        let generated = try String(contentsOf: Fixtures.dir.deletingLastPathComponent().appendingPathComponent("generated/ExponentialUICatalog.generated.swift"), encoding: .utf8)
        func array(_ name: String) -> [String] {
            guard let line = generated.split(separator: "\n").first(where: { $0.contains("static let \(name):") }), let eq = line.firstIndex(of: "=") else { return [] }
            return (JSONValue.parse(String(line[line.index(after: eq)...]).trimmingCharacters(in: .whitespaces)).array ?? []).compactMap(\.string)
        }
        let layout = Dictionary(uniqueKeysWithValues: zip(array("layoutConstantNames"), array("layoutConstantValues"))).compactMapValues(Double.init)
        XCTAssertEqual(layout["pendingDelayMs"], FeedbackTiming.pendingDelayMs)
        XCTAssertEqual(layout["skeletonPulseMs"], FeedbackTiming.skeletonPulseMs)
        XCTAssertEqual(layout["skeletonOpacityLow"], FeedbackTiming.skeletonOpacityLow)
        XCTAssertEqual(layout["skeletonOpacityHigh"], FeedbackTiming.skeletonOpacityHigh)
        XCTAssertEqual(layout["skeletonReducedOpacity"], FeedbackTiming.skeletonReducedOpacity)
        XCTAssertEqual(ThemeHandle.builtin("exponential")?.scale("press"), 0.97)
        XCTAssertEqual(ThemeHandle.builtin("exponential")?.scale("popoverFrom"), 0.96)
    }
}
