import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// The EXACT taffy frames of `layout-geometry.json` (900/390, LTR/RTL)
/// reach the model unchanged, and the overlay placements match.
@MainActor
final class GeometryTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    func testLayoutGeometryFixture() throws {
        let fixture = try Fixtures.json("layout-geometry.json")
        let cases = fixture["cases"]?.object ?? [:]
        XCTAssertEqual(cases.count, 4)
        for (name, c) in cases {
            var tree = fixture["surface"]!.object!
            var style = tree["style"]?.object ?? [:]
            style["direction"] = c["direction"]
            tree["style"] = .object(style)
            var options = SurfaceOptions()
            options.theme = nil
            options.mode = .light
            let m = try SurfaceModel(id: "geometry", options: options, host: NoHost())
            m.fixedMeasure = true
            try m.setNested(json: JSONValue.object(tree).json)
            // The fixture's measure overrides apply through the facade.
            _ = m.surface.setViewport(width: Float(c["width"]!.number!), height: 0, maxHeight: nil)
            let out = try m.surface.layoutFixed(sizesJson: fixture["measures"]!.json, wrap: false)
            m.setViewport(width: CGFloat(c["width"]!.number!), height: 0)
            let expected = c["frames"]!.array!
            XCTAssertEqual(out.frames.count, expected.count, name)
            for (i, want) in expected.enumerated() where i < out.frames.count {
                let f = out.frames[i]
                XCTAssertEqual(m.nodes[Int(f.index)].id, want["id"]!.string!, "\(name): order at \(i)")
                XCTAssertEqual(Double(f.x), want["x"]!.number!, accuracy: 0.001, "\(name) \(want["id"]!.string!).x")
                XCTAssertEqual(Double(f.y), want["y"]!.number!, accuracy: 0.001, "\(name) \(want["id"]!.string!).y")
                XCTAssertEqual(Double(f.w), want["w"]!.number!, accuracy: 0.001, "\(name) \(want["id"]!.string!).w")
                XCTAssertEqual(Double(f.h), want["h"]!.number!, accuracy: 0.001, "\(name) \(want["id"]!.string!).h")
            }
        }
    }

    func testOverlayPlacement() throws {
        let cases = try Fixtures.json("overlay-geometry.json")["cases"]?.array ?? []
        for c in cases {
            let a = c["anchor"]!, s = c["size"]!, v = c["viewport"]!
            // VAPP-276: `align` along the side (centre without one).
            let p = try placeOverlayAligned(anchorX: a["x"]!.number!, anchorY: a["y"]!.number!, anchorW: a["width"]!.number!, anchorH: a["height"]!.number!, sizeW: s["width"]!.number!, sizeH: s["height"]!.number!, viewportW: v["width"]!.number!, viewportH: v["height"]!.number!, side: c["side"]!.string!, align: c["align"]?.string ?? "center")
            if c["align"] == nil {
                let q = try placeOverlay(anchorX: a["x"]!.number!, anchorY: a["y"]!.number!, anchorW: a["width"]!.number!, anchorH: a["height"]!.number!, sizeW: s["width"]!.number!, sizeH: s["height"]!.number!, viewportW: v["width"]!.number!, viewportH: v["height"]!.number!, side: c["side"]!.string!)
                XCTAssertEqual(p.x, q.x)
                XCTAssertEqual(p.y, q.y)
            }
            let e = c["expected"]!
            XCTAssertEqual(p.x, e["x"]!.number!, c["name"]?.string ?? "")
            XCTAssertEqual(p.y, e["y"]!.number!, c["name"]?.string ?? "")
            XCTAssertEqual(p.side, e["side"]!.string!)
        }
    }

    /// VAPP-276: a Popover / Menu `anchor`ed to a panel button elsewhere in
    /// the tree: the core makes the button its trigger (the focus return
    /// reads `trigger(of:)`) and places the layer against its frame; a press
    /// opens it, Escape closes it, a second press on the anchor toggles.
    func testAnAnchorByIdIsTheTriggerAndPlacesTheLayer() throws {
        let m = try makeModel("anchor", theme: nil, width: 800, fixed: true)
        m.setViewport(width: 800, height: 600)
        try m.setNested(json: """
        {"id": "root", "component": "Box", "style": {"position": "relative", "width": 800, "height": 600}, "children": [
          {"id": "panel", "component": "Box", "style": {"position": "absolute", "left": 0, "top": 556, "width": 800, "height": 44, "display": "flex", "flexDirection": "row", "justifyContent": "space-between"}, "children": [
            {"id": "start", "component": "Button", "props": {"label": "Start"}},
            {"id": "clock", "component": "Button", "props": {"label": "12:00"}}]},
          {"id": "startMenu", "component": "Menu", "props": {"anchor": "start", "side": "top", "align": "start", "items": [{"label": "Files", "value": "files"}]}},
          {"id": "calendar", "component": "Popover", "props": {"anchor": "clock", "side": "top", "align": "end"}, "children": [{"id": "month", "component": "Text", "props": {"text": "November"}}]}]}
        """)
        XCTAssertFalse(m.nodes.contains { $0.id == "startMenu.trigger" && !$0.removed }, "no default Menu trigger")
        XCTAssertEqual(m.trigger(of: "calendar")?.id, "clock")
        XCTAssertEqual(m.trigger(of: "startMenu")?.id, "start")
        m.press(id: "clock")
        let layer = try XCTUnwrap(m.layers.first { $0.owner == "calendar" })
        let clock = try XCTUnwrap(m.trigger(of: "calendar"))
        XCTAssertEqual(layer.placementSide, "top")
        XCTAssertEqual(layer.anchorFrame, m.frame(clock.index))
        XCTAssertEqual(layer.frame.maxX, min(m.frame(clock.index).maxX, 792), accuracy: 0.01, "end-aligned (shifted 8 px inside the surface)")
        XCTAssertLessThanOrEqual(layer.frame.maxY, m.frame(clock.index).minY, "above the clock")
        XCTAssertTrue(m.escape())
        XCTAssertFalse(m.layers.contains { $0.owner == "calendar" })
        m.press(id: "start")
        let menu = try XCTUnwrap(m.layers.first { $0.owner == "startMenu" })
        XCTAssertEqual(menu.placementSide, "top")
        XCTAssertEqual(menu.anchorFrame, m.frame(try XCTUnwrap(m.trigger(of: "startMenu")).index))
        m.press(id: "start")
        XCTAssertFalse(m.layers.contains { $0.owner == "startMenu" }, "the anchor toggles it closed")
    }

    func testOpenLayersCarryFrames() throws {
        let m = try makeModel("ov")
        try m.setNested(json: Fixtures.text("kitchen-sink.json"))
        let dialogs = m.nodes.filter { $0.component == "Dialog" }
        try XCTSkipIf(dialogs.isEmpty, "no dialog in the kitchen sink")
        let owner = dialogs[0]
        // Round 1: a Toast is OPEN unless its `open` says false (the sink
        // binds it to an unset `/ui/toastOpen`), so its TOAST layer is up
        // from the start and stacks above every overlay (base < overlay <
        // toast, contract "Toast").
        let toasts = m.nodes.filter { $0.component == "Toast" }.map(\.id)
        XCTAssertEqual(m.layers.map(\.owner), toasts)
        XCTAssertEqual(m.layers.map(\.kind), toasts.map { _ in "Toast" })
        m.setOpen(owner.id, true)
        XCTAssertEqual(m.layers.map(\.owner), [owner.id] + toasts)
        let layer = m.layers[0]
        XCTAssertEqual(layer.kind, "Dialog")
        XCTAssertGreaterThan(layer.frame.width, 100)
        XCTAssertEqual(layer.position, "centered")
        // The layer's nodes have frames and the content root is in layer 1.
        XCTAssertEqual(m.node(layer.root)?.layer, 1)
        XCTAssertEqual(m.frame(layer.root), layer.frame)
        XCTAssertGreaterThan(m.layers.last?.layer ?? 0, layer.layer, "the toast layer stacks above the dialog")
        m.dismissLayer(owner.id)
        XCTAssertEqual(m.layers.map(\.owner), toasts)
    }
}
