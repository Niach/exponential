@testable import ExpCore
import Foundation
import XCTest

// #EXP-1958: sealed-content.json `cardStamps` replayed: `tapCases` through
// the Swift twin of `cardItemWire`, `openCases` through the eNet binding's
// openCard (the Rust reader lays the clear stamps over the opened items).
final class CardStampsFixtureTests: XCTestCase {
    private func cardStamps() throws -> [String: Any] {
        try XCTUnwrap(try SealedContentFixtureTests.fixture()["cardStamps"] as? [String: Any])
    }

    func testTapCases() throws {
        let cases = try XCTUnwrap(try cardStamps()["tapCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"] as? String ?? "?"
            let action = try XCTUnwrap(CardItemWire.Action(rawValue: try XCTUnwrap(c["action"] as? String)), name)
            let wire = CardItemWire.wire(
                sealed: c["stored"] as? String == "sealed", opened: c["opened"] as? Bool == true, action: action
            )
            XCTAssertEqual(wire.wire.rawValue, c["wire"] as? String, name)
            XCTAssertEqual(wire.call, c["call"] as? String, name)
        }
    }

    /// A card's own seal status picks the same wire (`plain` = unsealed).
    func testSealStatusPicksTheWire() {
        XCTAssertEqual(CardItemWire.wire(.plain, action: .reply), .server)
        XCTAssertEqual(CardItemWire.wire(.opened, action: .reply), .sealed)
        XCTAssertEqual(CardItemWire.wire(.opened, action: .file), .stamp)
        XCTAssertEqual(CardItemWire.wire(.noKey, action: .dismiss), .refused)
        XCTAssertEqual(CardItemWire.wire(.malformed, action: .start), .refused)
    }

    func testOpenCasesThroughTheBinding() throws {
        #if ENET_NODE
        try replayOpenCases(EnetSealedCrypto())
        #else
        throw XCTSkip("the full ExponentialNetFFI.xcframework was not built (build-ios.sh)")
        #endif
    }

    private func replayOpenCases(_ crypto: SealedCrypto) throws {
        let f = try SealedContentFixtureTests.fixture()
        let keys = try SealedContentFixtureTests.keys(f)
        let card = try XCTUnwrap(f["card"] as? [String: Any])
        var envelopes: [String: [String: Any]] = [:]
        for case let split as [String: Any] in try XCTUnwrap(card["splits"] as? [Any]) {
            envelopes[try XCTUnwrap(split["name"] as? String)] = split["envelope"] as? [String: Any]
        }
        let cases = try XCTUnwrap(try cardStamps()["openCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"] as? String ?? "?"
            var stored = try XCTUnwrap(envelopes[try XCTUnwrap(c["stored"] as? String)], name)
            stored["stamps"] = c["stamps"]
            let opened = crypto.openCard(
                try SealedContentFixtureTests.json(stored), keys: c["keys"] as? String == "tree" ? keys : [:]
            )
            XCTAssertEqual(opened.status.rawValue, c["status"] as? String, name)
            let shown = try SealedContentFixtureTests.object(opened.json)
            if let items = c["items"] as? [Any] {
                XCTAssertEqual(shown["items"] as? NSArray, items as NSArray, name)
                XCTAssertNil(shown["stamps"], name)
            } else {
                XCTAssertEqual(shown, c["card"] as? NSDictionary, name)
            }
            // The reader shows it as handled (TreeMessageCard reads the stamps).
            if c["status"] as? String == "opened", let expected = c["items"] as? [[String: Any]] {
                let parsed = try XCTUnwrap(TreeMessageCard.parse(opened.json)?.items, name)
                XCTAssertEqual(parsed.map(\.sent), expected.map { $0["sent"] as? Bool == true }, name)
                XCTAssertEqual(parsed.map(\.here), expected.map { $0["here"] as? Bool == true }, name)
                XCTAssertEqual(parsed.map(\.dismissed), expected.map { $0["dismissed"] as? Bool == true }, name)
                XCTAssertEqual(parsed.map(\.sessionId), expected.map { $0["sessionId"] as? String }, name)
            }
        }
    }
}
