import Foundation
import XCTest
@testable import ExpCore

// SLOP-28 (EXP-1259 D4): THE search list, locked ×4 against
// `search-list.json` — the `typeaheadPrefill` cases every client replays,
// plus the geometry and test ids the phone sheet draws with.
final class SearchListTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/search-list.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testTheTypeaheadPrefillReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try fixture()["typeaheadPrefill"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let text = try XCTUnwrap(c["text"] as? String)
            let caret = try XCTUnwrap(c["caret"] as? Int)
            let got = SearchList.typeaheadPrefill(text: text, caret: caret)
            if let expected = c["expected"] as? [String: Any] {
                XCTAssertEqual(
                    got,
                    SearchList.Prefill(
                        start: try XCTUnwrap(expected["start"] as? Int),
                        query: try XCTUnwrap(expected["query"] as? String)
                    ),
                    name
                )
            } else {
                XCTAssertNil(got, name)
            }
        }
    }

    func testAClampedCaretAndAnEmptyText() {
        XCTAssertNil(SearchList.typeaheadPrefill(text: "", caret: 0))
        XCTAssertEqual(SearchList.typeaheadPrefill(text: "#a", caret: 99), SearchList.Prefill(start: 0, query: "a"))
        XCTAssertNil(SearchList.typeaheadPrefill(text: "#", caret: 99))
        XCTAssertEqual(
            SearchList.typeaheadPrefill(text: "line\n#ab", caret: 8),
            SearchList.Prefill(start: 5, query: "ab")
        )
    }

    func testTheGeometryAndTestIdsAreTheFixtures() throws {
        let f = try fixture()
        let input = try XCTUnwrap(f["input"] as? [String: Any])
        XCTAssertEqual(SearchList.inputHeight, (input["height"] as? NSNumber)?.doubleValue)
        XCTAssertEqual(SearchList.inputIcon, input["icon"] as? String)
        let row = try XCTUnwrap(f["row"] as? [String: NSNumber])
        XCTAssertEqual(SearchList.rowHeight, row["height"]?.doubleValue)
        XCTAssertEqual(SearchList.rowHeightPhone, row["heightPhone"]?.doubleValue)
        XCTAssertEqual(SearchList.maxVisibleRows, f["maxVisibleRows"] as? Int)
        XCTAssertEqual(SearchList.fadeHeight, (f["fadeHeight"] as? NSNumber)?.doubleValue)
        XCTAssertEqual(SearchList.popoverWidth, (f["popoverWidth"] as? NSNumber)?.doubleValue)
        let ids = try XCTUnwrap(f["testIds"] as? [String: String])
        XCTAssertEqual(SearchList.TestId.list, ids["list"])
        XCTAssertEqual(SearchList.TestId.input, ids["input"])
        XCTAssertEqual(SearchList.TestId.row, ids["row"])
        XCTAssertEqual(SearchList.TestId.count, ids["count"])
    }
}
