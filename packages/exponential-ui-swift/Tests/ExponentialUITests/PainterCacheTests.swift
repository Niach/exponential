import XCTest
import ExponentialUICore
@testable import ExponentialUI

/// VAPP-131 / VAPP-142 / VAPP-133: the model's caches answer what the
/// uncached reads answer. A freed slot's local tombstone = the core's
/// record for it; the measure memo's frames = a model that measures every
/// leaf; the cached paint styles = a fresh conversion.
@MainActor
final class PainterCacheTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    private static let template = #"[{"id":"root","component":"Box","style":{"display":"flex","flexDirection":"column","gap":2},"children":{"componentId":"item","path":"/items","key":"id"}},{"id":"item","component":"Button","label":{"path":"title"},"variant":"outline"}]"#

    private func items(_ prefix: String, _ n: Int) -> JSONValue {
        .array((0..<n).map { .object(["id": .string("\(prefix)\($0)"), "title": .string("\(prefix) item \($0 % 7)")]) })
    }

    /// The facts a tombstone carries, side by side.
    private func facts(_ n: NodeInfo) -> String {
        "\(n.index) \(n.id) \(n.component) \(n.part ?? "-") \(n.owner ?? "-") \(n.ownerComponent ?? "-") \(n.depth) \(n.parent.map(String.init) ?? "-") \(n.children) \(n.layer) \(n.isLeaf) \(n.props) \(n.lines.map(String.init) ?? "-") \(n.pressable) \(n.hidden) \(n.triggerFor ?? "-") \(n.accessibility.map { JSONValue.object($0).json } ?? "-") \(n.states) \(n.partStates) \(n.form ?? "-") \(n.live ?? "-") \(n.removed) \(n.macroName ?? "-") \(n.hoverStyled) \(n.hovered)"
    }

    func testFreedSlotsMatchTheCoresTombstones() throws {
        let m = try makeModel("tomb", theme: "neutral", mode: .light)
        try m.setComponents(json: Self.template)
        m.setData(path: "/items", value: items("a", 12))
        m.setData(path: "/items", value: items("b", 5))
        let dead = m.nodes.filter(\.removed)
        XCTAssertFalse(dead.isEmpty, "the rebind freed slots")
        let core = Dictionary(uniqueKeysWithValues: m.surface.nodesAt(indices: dead.map { UInt32($0.index) }).map { (Int($0.index), NodeInfo($0)) })
        for n in dead {
            guard let c = core[n.index] else { continue }
            XCTAssertEqual(facts(n), facts(c), "slot \(n.index)")
        }
    }

    func testMemoisedMeasuresLayOutLikeFreshOnes() throws {
        let memo = try makeModel("memo", theme: "neutral", mode: .light)
        let fresh = try makeModel("fresh", theme: "neutral", mode: .light)
        fresh.memoizeMeasures = false
        for m in [memo, fresh] { try m.setComponents(json: Self.template) }
        for round in ["a", "b", "a", "c"] {
            for m in [memo, fresh] { m.setData(path: "/items", value: items(round, 40)) }
            XCTAssertEqual(memo.frames, fresh.frames, "round \(round)")
            XCTAssertEqual(memo.styles, fresh.styles, "round \(round)")
        }
        // A theme switch empties the memo: the new recipe's sizes, not the old.
        for m in [memo, fresh] { m.setTheme(ThemeHandle.builtin("playful")!) }
        XCTAssertEqual(memo.frames, fresh.frames, "after the theme switch")
        for m in [memo, fresh] { m.setMode(.dark); m.setViewport(width: 300, height: 800) }
        XCTAssertEqual(memo.frames, fresh.frames, "after the mode and width change")
    }

    func testTheBenchListScrollsLikeAnUnmemoisedModel() throws {
        let fixture = try Fixtures.json("bench-list.json")
        let memo = try makeModel("list-memo", theme: "neutral", mode: .light)
        let fresh = try makeModel("list-fresh", theme: "neutral", mode: .light)
        fresh.memoizeMeasures = false
        let rows = JSONValue.array((0..<2000).map { i in .object(["id": .string("r\(i)"), "title": .string("Row \(i)"), "meta": .string("\(i % 97)")]) })
        for m in [memo, fresh] {
            if let nested = fixture["surface"]?["nested"] ?? fixture["nested"] { try m.setNested(json: nested.json) } else { try m.setComponents(json: (fixture["components"] ?? fixture["surface"]?["components"] ?? .array([])).json) }
            m.setData(path: "/rows", value: rows)
        }
        for step in [1, 2, 3, 2, 1, 7, 0] {
            for m in [memo, fresh] { m.scrollTo(id: "root", offset: CGPoint(x: 0, y: CGFloat(step) * 800)) }
            XCTAssertEqual(memo.frames, fresh.frames, "step \(step)")
            XCTAssertEqual(memo.order, fresh.order, "step \(step)")
        }
    }

    /// `hardLines`' short cut: a text without a break is its own one line,
    /// what joining its soft segments gives.
    func testALineWithoutBreaksIsItsOwnHardLine() {
        for text in ["", " ", "Item 42 of ten thousand", "https://ui.exponential.at/a/b c", "trailing  ", "\tlead", "日本語のテキスト", "a\u{00A0}b — c", "👩‍👩‍👧 family"] {
            let segs = TextShaper.segments(text)
            XCTAssertEqual(segs.map(\.text).joined(), text, text)
            XCTAssertFalse(segs.contains(where: \.hard), text)
            XCTAssertEqual(TextShaper.hardLines(text), [text], text)
        }
    }
}
