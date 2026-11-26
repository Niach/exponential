import Foundation
import XCTest
@testable import ExpCore

// packages/domain-contract/fixtures/header-actions.json.
final class HeaderActionsTests: XCTestCase {
    func testReplay() throws {
        var url = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { url.deleteLastPathComponent() }
        url.appendPathComponent("packages/domain-contract/fixtures/header-actions.json")
        let fixture = try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
        XCTAssertEqual(fixture["order"] as? [String], HeaderActions.Action.allCases.map(\.rawValue))
        XCTAssertEqual(fixture["circle"] as? Int, HeaderActions.circle)
        XCTAssertEqual(fixture["glyph"] as? Int, HeaderActions.glyph)
        XCTAssertEqual(fixture["gap"] as? Int, HeaderActions.gap)
        XCTAssertEqual(Set(try XCTUnwrap(fixture["menuDrops"] as? [String])), HeaderActions.menuDrops)
        for c in try XCTUnwrap(fixture["orderCases"] as? [[String: Any]]) {
            let actions = try XCTUnwrap(c["actions"] as? [String]).compactMap(HeaderActions.Action.init)
            XCTAssertEqual(HeaderActions.ordered(actions).map(\.rawValue), c["expected"] as? [String])
        }
        for c in try XCTUnwrap(fixture["menuCases"] as? [[String: Any]]) {
            XCTAssertEqual(HeaderActions.menu(try XCTUnwrap(c["entries"] as? [String])), c["expected"] as? [String])
        }
        let collapse = try XCTUnwrap(fixture["collapse"] as? [String: Any])
        XCTAssertEqual(collapse["titleMinShare"] as? Double, HeaderActions.titleMinShare)
        XCTAssertEqual(collapse["order"] as? [String], HeaderActions.collapseOrder.map(\.rawValue))
        let cases = try XCTUnwrap(fixture["collapseCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let chips = try XCTUnwrap(c["chips"] as? [[String: Any]]).map { chip in
                HeaderActions.CollapseChip(
                    action: HeaderActions.Action(rawValue: chip["action"] as? String ?? "") ?? .more,
                    full: (chip["full"] as? NSNumber)?.doubleValue ?? 0,
                    compact: (chip["compact"] as? NSNumber)?.doubleValue ?? 0
                )
            }
            let folded = HeaderActions.collapse(
                bar: (c["bar"] as? NSNumber)?.doubleValue ?? 0,
                title: (c["title"] as? NSNumber)?.doubleValue ?? 0,
                fixed: (c["fixed"] as? NSNumber)?.doubleValue ?? 0,
                chips: chips
            )
            XCTAssertEqual(folded.map(\.rawValue), c["expected"] as? [String], c["name"] as? String ?? "")
        }
    }
}
