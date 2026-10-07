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

    func testScalesEveryTileDownByOneFactorWhenTheWidestOverflowsThePage() {
        let entries = [
            // 480pt wide at the pinned 320pt height — wider than a phone page.
            SessionResultEntry(
                topic: "t", label: "web", attachmentId: "a1", width: 1800, height: 1200
            ),
            SessionResultEntry(
                topic: "t", label: "ios", attachmentId: "a2", width: 828, height: 1800
            ),
        ]
        XCTAssertEqual(
            sessionResultTileHeightFitting(entries, availableWidth: 358), 238
        )
        // It fits: nothing scales.
        XCTAssertEqual(
            sessionResultTileHeightFitting(entries, availableWidth: 1000), 320
        )
        // Unmeasured page and no entries both keep the pinned height.
        XCTAssertEqual(sessionResultTileHeightFitting(entries, availableWidth: 0), 320)
        XCTAssertEqual(sessionResultTileHeightFitting([], availableWidth: 358), 320)
    }

    /// EXP-1128: the tall rule, the fixture's `tiles` cases ×4 — a full-page
    /// capture flags tall and takes the 4:3 frame; a phone shot never does.
    func testFramesATallCaptureAt43AndFlagsIt() throws {
        let tiles = try XCTUnwrap(try fixture()["tiles"] as? [String: Any])
        let cases = try XCTUnwrap(tiles["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let entry = SessionResultEntry(
                topic: "t",
                label: "l",
                attachmentId: "a",
                width: testCase["width"] as? Int,
                height: testCase["height"] as? Int
            )
            XCTAssertEqual(sessionResultIsTall(entry), try XCTUnwrap(testCase["tall"] as? Bool), name)
            XCTAssertEqual(
                sessionResultTileWidth(entry, height: 320),
                CGFloat(try XCTUnwrap(testCase["widthAt320"] as? Int)),
                name
            )
        }
    }

    /// EXP-1128: the tall viewer's strips stay under the texture cap.
    func testSplitsATallImageIntoStripRanges() {
        XCTAssertTrue(tallImageStripRanges(height: 0).isEmpty)
        let one = tallImageStripRanges(height: 4096)
        XCTAssertEqual(one.count, 1)
        XCTAssertEqual(one.first?.y, 0)
        XCTAssertEqual(one.first?.rows, 4096)
        let strips = tallImageStripRanges(height: 25094)
        XCTAssertEqual(strips.count, 7)
        XCTAssertEqual(strips.map(\.y), [0, 4096, 8192, 12288, 16384, 20480, 24576])
        XCTAssertEqual(strips.last?.rows, 518)
        XCTAssertEqual(strips.reduce(0) { $0 + $1.rows }, 25094)
    }

    // MARK: EXP-933 — the report fixture (`session-results.json`), same case
    // names ×4.

    private func fixture(_ name: String = "session-results.json") throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/\(name)")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    /// The fixture's `raw` as the entity's stored TEXT: a string stays as is,
    /// JSON null is nil, anything else is re-serialized.
    private func rawString(_ value: Any?) throws -> String? {
        guard let value, !(value is NSNull) else { return nil }
        if let string = value as? String { return string }
        let data = try JSONSerialization.data(withJSONObject: value, options: [.fragmentsAllowed])
        return String(data: data, encoding: .utf8)
    }

    func testGroupsPicturesAndTextPerTheFixture() throws {
        let cases = try XCTUnwrap(try fixture()["groups"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let expected = try XCTUnwrap(testCase["expected"] as? [[String: Any]])
            let groups = parseSessionResultGroups(try rawString(testCase["raw"]))
            XCTAssertEqual(groups.map(\.topic), expected.map { $0["topic"] as? String }, name)
            XCTAssertEqual(
                groups.map(\.text), expected.map { $0["text"] as? String }, name
            )
            XCTAssertEqual(
                groups.map { $0.entries.map { "\($0.label)|\($0.attachmentId)" } },
                expected.map { group in
                    (group["entries"] as? [[String: Any]] ?? []).map {
                        "\($0["label"] as? String ?? "")|\($0["attachmentId"] as? String ?? "")"
                    }
                },
                name
            )
            XCTAssertEqual(hasSessionResults(try rawString(testCase["raw"])), !expected.isEmpty, name)
        }
    }

    // MARK: EXP-1154 — files on a topic, and the Guide (`files` + `guide`).

    func testATextEntrysFilesRideItsGroupPerTheFixture() throws {
        let block = try XCTUnwrap(try fixture()["files"] as? [String: Any])
        XCTAssertEqual(maxSessionResultFiles, block["maxFiles"] as? Int)
        let cases = try XCTUnwrap(block["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let expected = try XCTUnwrap(testCase["expected"] as? [[String: Any]])
            let groups = parseSessionResultGroups(try rawString(testCase["raw"]))
            XCTAssertEqual(groups.map(\.topic), expected.map { $0["topic"] as? String }, name)
            XCTAssertEqual(
                groups.map(\.files), expected.map { $0["files"] as? [String] ?? [] }, name
            )
        }
    }

    func testCapsATopicsFilesAt40() {
        let paths = (0..<50).map { #""f\#($0).ts""# }.joined(separator: ",")
        let groups = parseSessionResultGroups(#"[{"topic":"t","text":"x","files":[\#(paths)]}]"#)
        XCTAssertEqual(groups.first?.files.count, 40)
        XCTAssertEqual(groups.first?.files.last, "f39.ts")
    }

    func testTheGuideLeadsWithTheFirstSummaryPerTheFixture() throws {
        let guide = try XCTUnwrap(try fixture()["guide"] as? [String: Any])
        let cases = try XCTUnwrap(guide["sections"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let topics = try XCTUnwrap(testCase["topics"] as? [String])
            let result = sessionResultsGuide(
                topics.map { SessionResultGroup(topic: $0, text: "t", entries: []) }
            )
            XCTAssertEqual(result.lead?.topic, testCase["lead"] as? String, name)
            let expected = try XCTUnwrap(testCase["sections"] as? [[Any]])
            XCTAssertEqual(
                result.sections.map { "\($0.group.topic)|\($0.index)|\($0.total)" },
                expected.map { "\($0[0] as? String ?? "")|\($0[1] as? Int ?? -1)|\($0[2] as? Int ?? -1)" },
                name
            )
        }
    }

    func testCaptionsASectionTwoDigitPerTheFixture() throws {
        let guide = try XCTUnwrap(try fixture()["guide"] as? [String: Any])
        let cases = try XCTUnwrap(guide["captions"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            XCTAssertEqual(
                guideSectionCaption(
                    try XCTUnwrap(testCase["index"] as? Int), try XCTUnwrap(testCase["total"] as? Int)
                ),
                testCase["text"] as? String
            )
        }
    }

    func testResolvesFileRowCountsByExactPathPerTheFixture() throws {
        let guide = try XCTUnwrap(try fixture()["guide"] as? [String: Any])
        let cases = try XCTUnwrap(guide["fileRows"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let paths = try XCTUnwrap(testCase["paths"] as? [String])
            let diff = (testCase["diff"] as? [[String: Any]])?.map {
                Diff.File(
                    path: $0["path"] as? String ?? "",
                    additions: $0["additions"] as? Int ?? 0,
                    deletions: $0["deletions"] as? Int ?? 0
                )
            }
            let expected = try XCTUnwrap(testCase["expected"] as? [[String: Any]]).map {
                GuideFileRow(
                    path: $0["path"] as? String ?? "",
                    additions: $0["additions"] as? Int,
                    deletions: $0["deletions"] as? Int
                )
            }
            XCTAssertEqual(guideFileRows(paths, files: diff), expected, name)
        }
    }

    func testTextEntriesDoNotCountTowardThe60PictureCap() {
        var rows: [String] = [#"{"topic":"Summary","text":"Report"}"#]
        for index in 0..<70 {
            rows.append(#"{"topic":"t","label":"l\#(index)","attachmentId":"a\#(index)"}"#)
        }
        rows.append(#"{"topic":"late","text":"after the cap"}"#)
        let groups = parseSessionResultGroups("[\(rows.joined(separator: ","))]")
        XCTAssertEqual(groups.map(\.topic), ["Summary", "t", "late"])
        XCTAssertEqual(sessionResultPictures(groups).count, maxSessionResults)
        XCTAssertEqual(groups.last?.text, "after the cap")
    }

    func testPicksTheIssueResultsRunPerTheFixture() throws {
        let block = try XCTUnwrap(try fixture()["issueResultsRun"] as? [String: Any])
        let now = try XCTUnwrap(WireTimestamps.parse(try XCTUnwrap(block["now"] as? String)))
        let cases = try XCTUnwrap(block["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let rows = try XCTUnwrap(testCase["rows"] as? [[String: Any]]).map { row in
                CodingSessionEntity(
                    id: row["id"] as? String ?? "",
                    issueId: row["issueId"] as? String,
                    teamId: "team-1",
                    userId: row["userId"] as? String ?? "",
                    deviceLabel: nil,
                    status: row["status"] as? String ?? "ended",
                    results: try rawString(row["results"]),
                    startedAt: row["startedAt"] as? String ?? "",
                    endedAt: nil,
                    createdAt: row["startedAt"] as? String ?? "",
                    updatedAt: row["updatedAt"] as? String ?? ""
                )
            }
            XCTAssertEqual(
                WorkFaces.issueResultsRun(
                    rows,
                    issueId: try XCTUnwrap(testCase["issueId"] as? String),
                    boundId: testCase["boundId"] as? String,
                    me: testCase["me"] as? String,
                    now: now
                )?.id,
                testCase["expected"] as? String,
                name
            )
        }
    }

    // MARK: EXP-1172 — inline pictures (`session-inline.json`), same case
    // names ×4.

    private func inlineEntry(_ entry: SessionResultEntry) -> [String: String] {
        [
            "label": entry.label,
            "attachmentId": entry.attachmentId,
            "inline": entry.inline ? "true" : "false",
            "caption": entry.caption ?? "<null>",
        ]
    }

    private func inlineExpected(_ value: Any?) -> [[String: String]] {
        (value as? [[String: Any]] ?? []).map { entry in
            [
                "label": entry["label"] as? String ?? "",
                "attachmentId": entry["attachmentId"] as? String ?? "",
                "inline": (entry["inline"] as? Bool ?? false) ? "true" : "false",
                "caption": entry["caption"] as? String ?? "<null>",
            ]
        }
    }

    func testInlineConstantsMatchTheFixture() throws {
        let fixture = try fixture("session-inline.json")
        XCTAssertEqual(sessionInlineTileHeight, CGFloat(try XCTUnwrap(fixture["inlineTileHeight"] as? Int)))
        XCTAssertEqual(sessionResultsEarlierLabel, fixture["earlierLabel"] as? String)
    }

    func testFoldsInlinePicturesUnderEarlierPerTheFixture() throws {
        let cases = try XCTUnwrap(try fixture("session-inline.json")["groups"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let expected = try XCTUnwrap(testCase["expected"] as? [[String: Any]])
            let raw = try rawString(testCase["raw"])
            let groups = parseSessionResultGroups(raw)
            XCTAssertEqual(groups.map(\.topic), expected.map { $0["topic"] as? String }, name)
            XCTAssertEqual(groups.map(\.text), expected.map { $0["text"] as? String }, name)
            XCTAssertEqual(
                groups.map { $0.entries.map(inlineEntry) },
                expected.map { inlineExpected($0["entries"]) },
                name
            )
            XCTAssertEqual(
                groups.map { $0.earlier.map(inlineEntry) },
                expected.map { inlineExpected($0["earlier"]) },
                name
            )
            // The pictures-only reader folds the same way.
            let pictureGroups = expected.filter {
                !inlineExpected($0["entries"]).isEmpty || !inlineExpected($0["earlier"]).isEmpty
            }
            let grouped = groupSessionResults(parseSessionResults(raw))
            XCTAssertEqual(
                grouped.map { $0.earlier.map(inlineEntry) },
                pictureGroups.map { inlineExpected($0["earlier"]) },
                name
            )
            // Tile sizing reads the folded pictures too.
            XCTAssertEqual(
                sessionResultPictures(groups).count,
                parseSessionResults(raw).count,
                name
            )
        }
    }

    func testLooksUpTheShownPicturePerTheFixture() throws {
        let cases = try XCTUnwrap(try fixture("session-inline.json")["lookup"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let picture = sessionResultPicture(
                try rawString(testCase["raw"]),
                attachmentId: testCase["attachmentId"] as? String
            )
            guard let expected = testCase["expected"] as? [String: Any] else {
                XCTAssertNil(picture, name)
                continue
            }
            let entry = try XCTUnwrap(picture, name)
            XCTAssertEqual(entry.label, expected["label"] as? String, name)
            XCTAssertEqual(entry.inline, expected["inline"] as? Bool, name)
            XCTAssertEqual(entry.caption, expected["caption"] as? String, name)
            XCTAssertEqual(entry.tileCaption, expected["tileCaption"] as? String, name)
        }
    }

    // MARK: EXP-1175 — the Run face's thread (`thread` cases ×4).

    func testReadsTheSessionThreadPerTheFixture() throws {
        let block = try XCTUnwrap(try fixture()["thread"] as? [String: Any])
        let cases = try XCTUnwrap(block["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let expected = try XCTUnwrap(testCase["expected"] as? [String: Any], name)
            let thread = sessionThread(try rawString(testCase["raw"]))
            let items: [[String: String]] = thread.items.map { item in
                switch item {
                case let .text(topic, text): return ["kind": "text", "topic": topic, "text": text]
                case let .picture(entry): return ["kind": "picture", "attachmentId": entry.attachmentId]
                }
            }
            let expectedItems = try XCTUnwrap(expected["items"] as? [[String: String]], name)
            XCTAssertEqual(items, expectedItems, name)
            XCTAssertEqual(thread.reply, expected["reply"] as? String, name)
        }
    }
}
