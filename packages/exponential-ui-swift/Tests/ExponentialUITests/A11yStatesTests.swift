import XCTest
import SwiftUI
import ExponentialUICore
@testable import ExponentialUI

/// #EXP-1629 (`a11y-states.json`, round-9 contract §5.1): the author's
/// `accessibility.selected` / `expanded` reach VoiceOver as `.isSelected` and
/// the value expanded / collapsed; an absent key reports neither; a
/// non-boolean is one reduce issue at its path.
@MainActor
final class A11yStatesTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    private func model(_ tree: JSONValue, data: JSONValue?) throws -> SurfaceModel {
        let m = try makeModel("a11y-states", theme: "neutral", mode: .dark, width: 390)
        let json = String(data: try JSONSerialization.data(withJSONObject: tree.any), encoding: .utf8)!
        try m.setNested(json: json)
        if let data { m.setData(path: "", value: data) }
        m.pass()
        return m
    }

    private func info(_ m: SurfaceModel, _ id: String) throws -> A11yInfo {
        let n = try XCTUnwrap(m.node(id: id), id)
        return A11yInfo.of(n, model: m, style: m.style(n.index))
    }

    private func states(_ value: String?) -> [String] {
        (value ?? "").split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { $0 == "expanded" || $0 == "collapsed" }
    }

    func testCasesReportSelectedAndExpanded() throws {
        let cases = try XCTUnwrap(Fixtures.json("a11y-states.json")["cases"]?.array)
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"]!.string!
            let m = try model(c["tree"]!, data: c["data"])
            XCTAssertEqual(m.issues.count, 0, "\(name): \(m.issues)")
            for (id, want) in c["expect"]?.object ?? [:] {
                let a = try info(m, id)
                if let selected = want["selected"]?.bool {
                    XCTAssertEqual(a.traits.contains(.isSelected), selected, "\(name) \(id): selected")
                }
                if let expanded = want["expanded"]?.bool {
                    XCTAssertEqual(states(a.value), [expanded ? "expanded" : "collapsed"], "\(name) \(id): \(a.value ?? "nil")")
                } else {
                    XCTAssertEqual(states(a.value), [], "\(name) \(id): no expanded state")
                }
            }
            for id in c["unset"]?.array?.compactMap(\.string) ?? [] {
                let a = try info(m, id)
                XCTAssertFalse(a.traits.contains(.isSelected), "\(name) \(id): unset selected")
                XCTAssertEqual(states(a.value), [], "\(name) \(id): unset expanded")
            }
        }
    }

    func testANonBooleanIsOneIssueAtItsPath() throws {
        let invalid = try XCTUnwrap(Fixtures.json("a11y-states.json")["invalid"]?.array)
        XCTAssertFalse(invalid.isEmpty)
        for c in invalid {
            let node = c["node"]!
            let id = node["id"]!.string!
            let m = try model(node, data: nil)
            XCTAssertEqual(m.issues.map { $0["path"]?.string ?? "" }, ["/components/\(id)\(c["path"]!.string!)"], "\(id): \(m.issues)")
        }
    }
}
