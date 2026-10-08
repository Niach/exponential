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
            let p = try placeOverlay(anchorX: a["x"]!.number!, anchorY: a["y"]!.number!, anchorW: a["width"]!.number!, anchorH: a["height"]!.number!, sizeW: s["width"]!.number!, sizeH: s["height"]!.number!, viewportW: v["width"]!.number!, viewportH: v["height"]!.number!, side: c["side"]!.string!)
            let e = c["expected"]!
            XCTAssertEqual(p.x, e["x"]!.number!, c["name"]?.string ?? "")
            XCTAssertEqual(p.y, e["y"]!.number!, c["name"]?.string ?? "")
            XCTAssertEqual(p.side, e["side"]!.string!)
        }
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
