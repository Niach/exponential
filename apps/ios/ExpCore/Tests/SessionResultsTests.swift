import Foundation
import XCTest

@testable import ExpCore

// EXP-879: the Results face's pure rules. Test names are mirrored by web
// `lib/session-results.test.ts`, desktop `session_results.rs` and Android
// `SessionResultsTest.kt`.
final class SessionResultsTests: XCTestCase {

    func testParsesAFlatOrderedListAndDropsMalformedEntries() {
        let raw = """
            [
              { "topic": "chatui", "label": "web", "attachmentId": "a1",
                "width": 1600, "height": 900 },
              { "topic": "chatui", "attachmentId": "a2", "width": 10, "height": 10 },
              { "topic": "chatui", "label": "ios", "width": 10, "height": 10 },
              { "topic": "   ", "label": "android", "attachmentId": "a3" },
              { "topic": "chatui", "label": 7, "attachmentId": "a4" },
              { "topic": "chatui", "label": "ios", "attachmentId": "a5",
                "width": 0, "height": -3 },
              { "topic": "nav", "label": "web", "attachmentId": "a6",
                "width": "800", "height": null },
              null,
              "nope",
              []
            ]
            """
        XCTAssertEqual(
            parseSessionResults(raw),
            [
                SessionResultEntry(
                    topic: "chatui", label: "web", attachmentId: "a1",
                    width: 1600, height: 900
                ),
                SessionResultEntry(topic: "chatui", label: "ios", attachmentId: "a5"),
                SessionResultEntry(topic: "nav", label: "web", attachmentId: "a6"),
            ]
        )
        // Topic, label and id are trimmed.
        let trimmed = parseSessionResults(
            #"[{"topic":" t ","label":" l ","attachmentId":" a "}]"#
        )
        XCTAssertEqual(
            trimmed, [SessionResultEntry(topic: "t", label: "l", attachmentId: "a")]
        )
        // The derived, member-gated image URL.
        XCTAssertEqual(trimmed.first?.url, "/api/attachments/a")
    }

    func testIgnoresUnknownFieldsAndANullOrBlankBlob() {
        XCTAssertEqual(
            parseSessionResults("""
                [{ "topic": "t", "label": "l", "attachmentId": "a", "width": 4,
                   "height": 2, "url": "/api/attachments/a", "extra": { "nope": true } }]
                """),
            [
                SessionResultEntry(
                    topic: "t", label: "l", attachmentId: "a", width: 4, height: 2
                ),
            ]
        )
        XCTAssertEqual(parseSessionResults(nil), [])
        XCTAssertEqual(parseSessionResults(""), [])
        XCTAssertEqual(parseSessionResults("   "), [])
        XCTAssertEqual(parseSessionResults("{"), [])
        XCTAssertEqual(parseSessionResults(#"{"topic":"t"}"#), [])
        XCTAssertEqual(parseSessionResults("42"), [])
        XCTAssertEqual(parseSessionResults("null"), [])
    }

    func testGroupsByTopicInFirstSeenOrder() {
        let entries = parseSessionResults("""
            [
              { "topic": "chatui", "label": "web", "attachmentId": "a1" },
              { "topic": "nav", "label": "web", "attachmentId": "a2" },
              { "topic": "chatui", "label": "ios", "attachmentId": "a3" }
            ]
            """)
        XCTAssertEqual(
            groupSessionResults(entries),
            [
                SessionResultGroup(
                    topic: "chatui",
                    entries: [
                        SessionResultEntry(topic: "chatui", label: "web", attachmentId: "a1"),
                        SessionResultEntry(topic: "chatui", label: "ios", attachmentId: "a3"),
                    ]
                ),
                SessionResultGroup(
                    topic: "nav",
                    entries: [
                        SessionResultEntry(topic: "nav", label: "web", attachmentId: "a2"),
                    ]
                ),
            ]
        )
        XCTAssertEqual(groupSessionResults([]), [])
    }

    func testCapsAt60Entries() {
        XCTAssertEqual(maxSessionResults, 60)
        let rows = (0..<80).map { index in
            #"{"topic":"t","label":"l\#(index)","attachmentId":"a\#(index)"}"#
        }
        let parsed = parseSessionResults("[\(rows.joined(separator: ","))]")
        XCTAssertEqual(parsed.count, 60)
        XCTAssertEqual(parsed[59].label, "l59")
    }

    func testSizesATileFromTheProbedAspect43WithoutOne() {
        XCTAssertEqual(sessionResultTileHeight, 320)
        func entry(_ width: Int?, _ height: Int?) -> SessionResultEntry {
            SessionResultEntry(topic: "t", label: "l", attachmentId: "a", width: width, height: height)
        }
        XCTAssertEqual(sessionResultTileWidth(entry(1600, 900)), 569)
        XCTAssertEqual(sessionResultTileWidth(entry(1170, 2532)), 148)
        // Either dimension missing falls back to 4:3.
        XCTAssertEqual(sessionResultTileWidth(entry(nil, 900)), 427)
        XCTAssertEqual(sessionResultTileWidth(entry(1600, nil)), 427)
        XCTAssertEqual(sessionResultTileWidth(entry(nil, nil), height: 120), 160)
        XCTAssertEqual(sessionResultTileWidth(entry(1000, 1000), height: 200), 200)
    }
}
