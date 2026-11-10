import XCTest
@testable import ExponentialUI

/// #EXP-1778: `fixtures/text-truncate.json` (TS, the Rust core and Compose
/// replay the same cases) through `middleTruncateWidth`.
final class TextTruncateTests: XCTestCase {
    override func setUp() async throws {
        try XCTSkipUnless(Fixtures.available(), "fixtures not in this checkout")
    }

    func testTheFixtureReplays() throws {
        let f = try Fixtures.json("text-truncate.json")
        let affix = f["affix"]?.string ?? "…"
        let cases = f["cases"]?.array ?? []
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let widths = c["widths"]
            let fallback = widths?["default"]?.number ?? .nan
            let shown = middleTruncateWidth(c["text"]?.string ?? "", width: c["width"]?.number ?? .nan, affix: affix) { ch in
                widths?["chars"]?[ch]?.number ?? fallback
            }
            XCTAssertEqual(shown, c["shown"]?.string, c["name"]?.string ?? c.json)
        }
    }
}
