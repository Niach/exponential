import XCTest
import SwiftUI
import ExponentialUICore
@testable import ExponentialUI

/// Every shared fixture renders through the painter's model: the catalog
/// components, the macros, the basic-catalog map, the extension cases and
/// the kitchen sink, in every built-in theme and both modes.
@MainActor
final class FixtureReplayTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    func testKitchenSinkLaysOutEveryNode() throws {
        let m = try makeModel("ks")
        try m.setNested(json: Fixtures.text("kitchen-sink.json"))
        m.setData(path: "/draft", value: .object(["title": .string("")]))
        XCTAssertEqual(m.issues.count, 0, "\(m.issues)")
        XCTAssertGreaterThan(m.nodes.count, 150)
        XCTAssertEqual(m.frames.count, m.nodes.count)
        XCTAssertGreaterThan(m.surfaceSize.height, 1000)
        XCTAssertLessThanOrEqual(m.stats.upcalls, 3)
        // Every visible main-tree node has a frame inside the surface.
        for n in m.nodes where !n.hidden && n.layer == 0 {
            let f = m.frame(n.index)
            XCTAssertGreaterThanOrEqual(f.minX, -0.01, n.id)
            XCTAssertLessThanOrEqual(f.maxX, m.surfaceSize.width + 0.01, n.id)
        }
        // The header reads first (pre-order = accessibility order).
        XCTAssertEqual(m.nodes.first { $0.id == "hdr-title" }.map { $0.index < (m.index(of: "hdr-scan") ?? 0) }, true)
        // A warm pass makes no measure calls.
        m.layoutNeeded = true
        m.pass()
        XCTAssertEqual(m.stats.upcalls, 0)
        XCTAssertEqual(m.stats.measureCalls, 0)
    }

    func testKitchenSinkUnderEveryThemeAndMode() throws {
        for theme in builtinThemeIds() {
            for mode in Mode.allCases {
                let m = try makeModel("ks-\(theme)-\(mode)", theme: theme, mode: mode, width: 900)
                try m.setNested(json: Fixtures.text("kitchen-sink.json"))
                XCTAssertEqual(m.issues.count, 0)
                XCTAssertGreaterThan(m.surfaceSize.height, 500, "\(theme)/\(mode)")
                XCTAssertEqual(m.styles.count, m.nodes.count)
                // The theme's background reaches the root box.
                XCTAssertNotNil(m.style(0).background, "\(theme)/\(mode) root background")
            }
        }
    }

    func testCatalogComponentsRender() throws {
        let cases = try Fixtures.json("catalog-components.json")["cases"]?.array ?? []
        XCTAssertGreaterThan(cases.count, 100)
        var failures: [String] = []
        for c in cases {
            let name = c["name"]?.string ?? "?"
            let m = try makeModel("cc")
            do {
                let out = try m.setNested(json: c["node"]!.json)
                if out.issuesJson != "[]" { failures.append("\(name): \(out.issuesJson)") }
            } catch {
                failures.append("\(name): \(error)")
                continue
            }
            if m.nodes.isEmpty { failures.append("\(name): no nodes") }
            if m.frames.count != m.nodes.count { failures.append("\(name): frames") }
            // Every node resolves a box style and an ink; leaves a text style.
            for n in m.nodes {
                _ = m.style(n.index)
                _ = m.ink(n.index)
                if n.isLeaf { _ = m.textStyle(n.index) }
            }
        }
        XCTAssertEqual(failures, [])
    }

    func testMacrosAndBasicMapReduceLikeTheReference() throws {
        let macros = try Fixtures.json("catalog-macros.json")["cases"]?.array ?? []
        for c in macros {
            let got = try reduceNestedJson(nestedJson: c["input"]!.json, catalogId: coreCatalogId(), extensionsJson: nil)
            let expected = JSONValue.object(["root": c["expected"]!, "issues": .array([])]).json
            XCTAssertTrue(jsonEqual(a: got, b: expected), "macro \(c["name"]?.string ?? ""): \(jsonDiff(a: got, b: expected))")
        }
        let basic = try Fixtures.json("catalog-basic-map.json")["cases"]?.array ?? []
        for c in basic {
            let m = try makeModel("basic")
            let catalog = basicCatalogId()
            _ = catalog
            let out = try m.setComponents(json: c["components"]!.json)
            _ = out
            // A basic-catalog surface reduces on the CORE surface through the
            // free reducer; the model's surface is core-catalog, so compare
            // through the reducer directly.
            let got = try reduceSurfaceJson(componentsJson: c["components"]!.json, catalogId: basicCatalogId(), extensionsJson: nil)
            XCTAssertTrue(jsonEqual(a: got, b: c["expected"]!.json), "basic \(c["name"]?.string ?? ""): \(jsonDiff(a: got, b: c["expected"]!.json))")
        }
    }

    func testExtensionFixtureRendersThroughAPainter() throws {
        let fixture = try Fixtures.json("catalog-extension.json")
        let definition = fixture["extension"]!.json
        final class Probe: ExtensionPainter {
            var measured = 0
            var painted = 0
            func measure(_ leaf: ExtensionLeaf, wrap: CGFloat?) -> CGSize? {
                measured += 1
                return CGSize(width: 120, height: 40)
            }
            func paint(_ context: ExtensionContext) -> AnyView {
                painted += 1
                return AnyView(Color.red)
            }
        }
        let probe = Probe()
        let registry = ExtensionRegistry.shared
        registry.reset()
        try ExponentialUI.register(extension: definition, painters: ["TrendLine": probe, "StatCard": probe])
        defer { registry.reset() }
        for c in fixture["cases"]?.array ?? [] {
            var options = SurfaceOptions()
            options.catalogId = c["catalogId"]?.string ?? coreCatalogId()
            let m = try SurfaceModel(id: "ext", options: options, host: NoHost())
            m.setViewport(width: 390, height: 600)
            let out = try m.setComponents(json: c["components"]!.json)
            let expectedIssues = c["expected"]?["issues"]?.array?.count ?? 0
            XCTAssertEqual(JSONValue.parse(out.issuesJson).array?.count ?? 0, expectedIssues, "\(c["name"]?.string ?? ""): \(out.issuesJson)")
            let leaves = m.nodes.filter { $0.component == "Extension" }
            if expectedIssues == 0, c["expected"]!.json.contains("\"Extension\"") {
                XCTAssertFalse(leaves.isEmpty, "\(c["name"]?.string ?? ""): extension leaves")
            }
            for l in leaves {
                XCTAssertEqual(m.frame(l.index).height, 40, "\(l.id) measured by the painter")
            }
        }
        XCTAssertGreaterThan(probe.measured, 0)
    }
}
