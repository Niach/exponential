import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// #EXP-1781: `fixtures/forced-states.json` replayed through `SurfaceModel`
/// (the core resolves the visuals; the painters read them), plus the parts
/// this painter queries itself (the Segmented item).
@MainActor
final class ForcedStatesTests: XCTestCase {
    func testEveryForcedStatesCaseReplays() throws {
        try XCTSkipUnless(Fixtures.available())
        let cases = try Fixtures.json("forced-states.json")["cases"]?.array ?? []
        XCTAssertGreaterThanOrEqual(cases.count, 7)
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let host = RecordingHost()
            let m = try makeModel("forced", theme: "exponential", mode: .light, width: 480)
            m.host = host
            m.setViewport(width: 480, height: 900)
            try m.setNested(json: c["tree"]!.json)
            var dropped: [String] = []
            for step in c["steps"]?.array ?? [] {
                if let forced = step["forced"]?.object {
                    dropped = m.setForcedStates(forced.mapValues { ($0.array ?? []).compactMap(\.string) })
                }
                if let h = step["host"] {
                    _ = m.surface.setStates(id: h["id"]?.string ?? "", states: (h["states"]?.array ?? []).compactMap(\.string))
                    m.invalidate(structure: false)
                }
            }
            let e = c["expect"]!
            // The FFI takes a dictionary (no order): compare as sets.
            XCTAssertEqual(Set(dropped), Set((e["dropped"]?.array ?? []).compactMap(\.string)), "\(name): dropped")
            for (id, want) in e["states"]?.object ?? [:] {
                XCTAssertEqual(m.surface.forcedStatesOf(id: id), (want.array ?? []).compactMap(\.string), "\(name): states of \(id)")
            }
            for (id, want) in e["visuals"]?.object ?? [:] {
                guard let i = m.surface.indexOf(id: id), let v = m.surface.visual(index: i) else {
                    XCTFail("\(name): no node \(id)")
                    continue
                }
                for (key, value) in want.object ?? [:] {
                    switch key {
                    case "backgroundColor": XCTAssertEqual(v.backgroundColor?.lowercased(), value.string, "\(name): \(id).\(key)")
                    case "borderColor": XCTAssertEqual(v.borderColor?.lowercased(), value.string, "\(name): \(id).\(key)")
                    case "color": XCTAssertEqual(v.color?.lowercased(), value.string, "\(name): \(id).\(key)")
                    case "opacity": XCTAssertEqual(Double(v.opacity ?? 1), value.number ?? 1, accuracy: 1e-4, "\(name): \(id).\(key)")
                    default: XCTFail("\(name): unchecked visual key \(key)")
                    }
                }
            }
            // Paint only: no layer, no event, no focus move.
            XCTAssertEqual(m.layers.count, e["layers"]?.number.map(Int.init) ?? 0, "\(name): layers")
            XCTAssertTrue(host.actions.isEmpty && host.inputs.isEmpty, "\(name): events")
            XCTAssertNil(m.focusedId, "\(name): focus")
        }
    }

    func testAForcedHoverRestylesASegmentedItem() throws {
        let m = try makeModel("seg", theme: "exponential", mode: .light, width: 480)
        try m.setNested(json: #"{"id":"seg","component":"Segmented","props":{"items":[{"label":"Day","value":"day"},{"label":"Week","value":"week"},{"label":"Month","value":"month"}],"value":"day"}}"#)
        let i = try XCTUnwrap(m.byId["seg"])
        let props = m.ownerProps(i)
        func item(_ k: Int) -> PaintStyle {
            m.part("Segmented", "item", props: props, states: SegmentedLeaf.itemStates(m, id: "seg", k, selected: k == 0, disabled: false, ring: false)).style
        }
        let rest = item(1)
        XCTAssertEqual(m.setForcedStates(["seg.item.1": ["hover"]]), [])
        XCTAssertEqual(SegmentedLeaf.itemStates(m, id: "seg", 1, selected: false, disabled: false, ring: false), ["hover"])
        XCTAssertNotEqual(item(1).background, rest.background, "a forced hover fills the item")
        XCTAssertEqual(SegmentedLeaf.itemStates(m, id: "seg", 2, selected: false, disabled: false, ring: false), [], "its siblings keep their look")
        m.setForcedStates([:])
        XCTAssertEqual(item(1).background, rest.background)
    }
}
