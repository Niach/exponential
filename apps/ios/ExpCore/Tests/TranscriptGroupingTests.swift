import Foundation
import XCTest
@testable import ExpCore

// #EXP-1736: who sent each transcript row, the groups and their gaps,
// locked ×4 against `packages/domain-contract/fixtures/transcript-grouping.json`;
// every case compares the WHOLE expected array.
final class TranscriptGroupingTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/transcript-grouping.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func string(_ value: Any?) -> String? {
        value as? String
    }

    private func number(_ value: Any?) throws -> CGFloat {
        CGFloat(try XCTUnwrap(value as? NSNumber).doubleValue)
    }

    func testLayoutAndCopy() throws {
        let fixture = try fixture()
        let layout = try XCTUnwrap(fixture["layout"] as? [String: Any])
        let maxWidth = try XCTUnwrap(layout["maxWidth"] as? [String: Any])
        XCTAssertEqual(Set(layout.keys), ["avatar", "avatarGap", "gapInGroup", "gapBetween", "maxWidth", "radius", "nameSize"])
        XCTAssertEqual(try number(layout["avatar"]), TranscriptGrouping.Layout.avatar)
        XCTAssertEqual(try number(layout["avatarGap"]), TranscriptGrouping.Layout.avatarGap)
        XCTAssertEqual(try number(layout["gapInGroup"]), TranscriptGrouping.Layout.gapInGroup)
        XCTAssertEqual(try number(layout["gapBetween"]), TranscriptGrouping.Layout.gapBetween)
        XCTAssertEqual(try number(maxWidth["phone"]), TranscriptGrouping.Layout.maxWidthPhone)
        XCTAssertEqual(try number(maxWidth["phoneIn"]), TranscriptGrouping.Layout.maxWidthPhoneIn)
        XCTAssertEqual(try number(maxWidth["wide"]), TranscriptGrouping.Layout.maxWidthWide)
        XCTAssertEqual(try number(layout["radius"]), TranscriptGrouping.Layout.radius)
        XCTAssertEqual(try number(layout["nameSize"]), TranscriptGrouping.Layout.nameSize)
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(Set(copy.keys), ["formerMember", "linkChevron", "markers"])
        XCTAssertEqual(copy["formerMember"] as? String, TranscriptGrouping.Copy.formerMember)
        XCTAssertEqual(copy["linkChevron"] as? String, TranscriptGrouping.Copy.linkChevron)
        XCTAssertEqual(copy["markers"] as? [String: String], TranscriptGrouping.Copy.markers)
    }

    private func named(_ rows: Any?) throws -> [TranscriptGrouping.Named] {
        try XCTUnwrap(rows as? [[String: Any]]).map { row in
            TranscriptGrouping.Named(id: try XCTUnwrap(string(row["id"])), name: try XCTUnwrap(string(row["name"])))
        }
    }

    private func sender(_ value: Any?) throws -> TranscriptGrouping.Sender? {
        guard let row = value as? [String: Any] else { return nil }
        XCTAssertEqual(Set(row.keys), ["kind", "id", "name", "link"])
        return TranscriptGrouping.Sender(
            kind: try XCTUnwrap(TranscriptGrouping.SenderKind(rawValue: try XCTUnwrap(string(row["kind"])))),
            id: try XCTUnwrap(string(row["id"])),
            name: try XCTUnwrap(string(row["name"])),
            link: string(row["link"])
        )
    }

    func testReplaysEveryCase() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let chat = try XCTUnwrap(input["chat"] as? [String: Any])
            let sessions = try XCTUnwrap(input["sessions"] as? [[String: Any]]).map { row in
                TranscriptGrouping.KnownSession(
                    id: try XCTUnwrap(string(row["id"])),
                    title: try XCTUnwrap(string(row["title"])),
                    agentId: string(row["agentId"])
                )
            }
            let items = try XCTUnwrap(input["items"] as? [[String: Any]]).map { row in
                TranscriptGrouping.Item(
                    id: try XCTUnwrap(string(row["id"])),
                    kind: try XCTUnwrap(TranscriptGrouping.ItemKind(rawValue: try XCTUnwrap(string(row["kind"])))),
                    authorKind: string(row["authorKind"]),
                    authorId: string(row["authorId"]),
                    authorSessionId: string(row["authorSessionId"])
                )
            }
            let result = TranscriptGrouping.transcriptLayout(TranscriptGrouping.Input(
                viewerId: string(input["viewerId"]),
                chat: TranscriptGrouping.Chat(
                    sessionId: try XCTUnwrap(string(chat["sessionId"])),
                    title: try XCTUnwrap(string(chat["title"])),
                    agentId: string(chat["agentId"]),
                    originSessionId: string(chat["originSessionId"]),
                    fromStart: chat["fromStart"] as? Bool ?? true
                ),
                members: try named(input["members"]),
                agents: try named(input["agents"]),
                sessions: sessions,
                items: items
            ))
            let want = try XCTUnwrap(testCase["expected"] as? [[String: Any]]).map { row in
                XCTAssertEqual(Set(row.keys), ["id", "side", "sender", "avatar", "name", "gap"], name)
                return TranscriptGrouping.RowLayout(
                    id: try XCTUnwrap(string(row["id"])),
                    side: try XCTUnwrap(TranscriptGrouping.Side(rawValue: try XCTUnwrap(string(row["side"])))),
                    sender: try sender(row["sender"]),
                    avatar: try XCTUnwrap(row["avatar"] as? Bool),
                    name: try XCTUnwrap(row["name"] as? Bool),
                    gap: try number(row["gap"])
                )
            }
            XCTAssertEqual(result, want, name)
        }
    }

    func testReplaysEveryInputCase() throws {
        let cases = try XCTUnwrap(try fixture()["inputCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let text = try XCTUnwrap(string(testCase["text"]))
            let want: TranscriptGrouping.CrossSessionInput? = try (testCase["expected"] as? [String: Any]).map { row in
                XCTAssertEqual(Set(row.keys), ["kind", "name", "sessionId", "body"], name)
                return TranscriptGrouping.CrossSessionInput(
                    kind: try XCTUnwrap(TranscriptGrouping.CrossSessionInput.Kind(rawValue: try XCTUnwrap(string(row["kind"])))),
                    name: try XCTUnwrap(string(row["name"])),
                    sessionId: try XCTUnwrap(string(row["sessionId"])),
                    body: try XCTUnwrap(string(row["body"]))
                )
            }
            XCTAssertEqual(TranscriptGrouping.crossSessionInput(text), want, name)
        }
    }

    func testReplaysEveryFeedInputCase() throws {
        let cases = try XCTUnwrap(try fixture()["feedInputCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let text = try XCTUnwrap(string(testCase["text"]))
            let want: TranscriptGrouping.FeedInput? = try (testCase["expected"] as? [String: Any]).map { row in
                switch try XCTUnwrap(string(row["kind"])) {
                case "marker":
                    XCTAssertEqual(Set(row.keys), ["kind", "key", "label"], name)
                    return .marker(key: try XCTUnwrap(string(row["key"])), label: try XCTUnwrap(string(row["label"])))
                default:
                    XCTAssertEqual(Set(row.keys), ["kind", "name", "sessionId", "body", "report"], name)
                    return .relay(
                        name: try XCTUnwrap(string(row["name"])),
                        sessionId: try XCTUnwrap(string(row["sessionId"])),
                        body: try XCTUnwrap(string(row["body"])),
                        report: try XCTUnwrap(row["report"] as? Bool)
                    )
                }
            }
            XCTAssertEqual(TranscriptGrouping.feedInput(text), want, name)
        }
    }

    func testReplaysEveryMarkerCase() throws {
        let cases = try XCTUnwrap(try fixture()["markerCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let kinds = try XCTUnwrap(testCase["kinds"] as? [String]).map { kind in
                try XCTUnwrap(TranscriptGrouping.FeedRowKind(rawValue: kind))
            }
            XCTAssertEqual(TranscriptGrouping.markersShown(kinds), try XCTUnwrap(testCase["expected"] as? [Bool]), name)
        }
    }
}
