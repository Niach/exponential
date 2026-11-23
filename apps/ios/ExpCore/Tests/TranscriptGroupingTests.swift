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
        XCTAssertEqual(try number(maxWidth["own"]), TranscriptGrouping.Layout.maxWidthOwn)
        XCTAssertEqual(try number(layout["radius"]), TranscriptGrouping.Layout.radius)
        XCTAssertEqual(try number(layout["nameSize"]), TranscriptGrouping.Layout.nameSize)
        let copy = try XCTUnwrap(fixture["copy"] as? [String: Any])
        XCTAssertEqual(Set(copy.keys), ["formerMember", "linkChevron", "markers", "checkIn", "checkIns", "checkInTimeSeparator"])
        XCTAssertEqual(copy["checkIn"] as? String, TranscriptGrouping.Copy.checkIn)
        XCTAssertEqual(copy["checkIns"] as? String, TranscriptGrouping.Copy.checkIns)
        XCTAssertEqual(copy["checkInTimeSeparator"] as? String, TranscriptGrouping.Copy.checkInTimeSeparator)
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
                XCTAssertEqual(Set(row.keys), ["id", "side", "sender", "avatar", "name", "tail", "gap"], name)
                return TranscriptGrouping.RowLayout(
                    id: try XCTUnwrap(string(row["id"])),
                    side: try XCTUnwrap(TranscriptGrouping.Side(rawValue: try XCTUnwrap(string(row["side"])))),
                    sender: try sender(row["sender"]),
                    avatar: try XCTUnwrap(row["avatar"] as? Bool),
                    name: try XCTUnwrap(row["name"] as? Bool),
                    tail: try XCTUnwrap(row["tail"] as? Bool),
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
                case "member":
                    XCTAssertEqual(Set(row.keys), ["kind", "name", "userId", "body"], name)
                    return .member(
                        name: try XCTUnwrap(string(row["name"])),
                        userId: try XCTUnwrap(string(row["userId"])),
                        body: try XCTUnwrap(string(row["body"]))
                    )
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

    /// #EXP-1990 `memberInputCases`: the device's wording, read back whole.
    func testReplaysEveryMemberInputCase() throws {
        let cases = try XCTUnwrap(try fixture()["memberInputCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let input = try XCTUnwrap(testCase["input"] as? [String: Any])
            let member = try XCTUnwrap(string(input["name"]))
            let userId = try XCTUnwrap(string(input["userId"]))
            let body = try XCTUnwrap(string(input["body"]))
            let line = TranscriptGrouping.memberInput(name: member, userId: userId, body: body)
            XCTAssertEqual(line, string(testCase["expected"]), name)
            XCTAssertEqual(TranscriptGrouping.feedInput(line), .member(name: member, userId: userId, body: body), name)
        }
    }

    /// #EXP-1990: a team agent's group chat's LIVE feed with two authors:
    /// the viewer's own line is their own bubble (right), a teammate's sits
    /// left under their name (the synced member's, else the line's), each
    /// bubble holding the body alone; and the viewer's consumed row hands
    /// off to its member-worded echo (never a second own bubble).
    func testGroupChatFeedDrawsEachMembersLine() {
        let viewer = "u-alex"
        let feed: [(id: String, text: String?)] = [
            ("1", TranscriptGrouping.memberInput(name: "Alex Carter", userId: viewer, body: "ship it")),
            ("2", nil),
            ("3", TranscriptGrouping.memberInput(name: "Mira C.", userId: "u-mira", body: "Also check the changelog.\nThanks!")),
            ("4", TranscriptGrouping.memberInput(name: "Sam Okafor", userId: "u-sam", body: "+1")),
            ("5", TranscriptGrouping.memberInput(name: "Alex Carter", userId: viewer, body: "on it")),
        ]
        let synced = ["u-mira": "Mira Chen"]
        var members: [TranscriptGrouping.Named] = []
        let items: [TranscriptGrouping.Item] = feed.map { row in
            guard let text = row.text else {
                return TranscriptGrouping.Item(id: row.id, kind: .message, authorKind: DomainContract.messageAuthorKindAgent)
            }
            let prompt = TranscriptGrouping.feedPrompt(id: row.id, text: text, viewerId: viewer) { synced[$0] }
            XCTAssertNil(prompt.session)
            if let member = prompt.member { members.append(member) }
            return prompt.item
        }
        XCTAssertEqual(members, [
            TranscriptGrouping.Named(id: "u-mira", name: "Mira Chen"),
            TranscriptGrouping.Named(id: "u-sam", name: "Sam Okafor"),
        ])
        let layout = TranscriptGrouping.transcriptLayout(TranscriptGrouping.Input(
            viewerId: viewer,
            chat: TranscriptGrouping.Chat(sessionId: "chat", title: "Release agent", agentId: "a1", originSessionId: nil),
            members: members, agents: [], sessions: [], items: items
        ))
        XCTAssertEqual(layout.map(\.side), [.own, .other, .other, .other, .own])
        XCTAssertNil(layout[0].sender)
        XCTAssertEqual(layout[2].sender, TranscriptGrouping.Sender(kind: .member, id: "u-mira", name: "Mira Chen", link: nil))
        XCTAssertEqual(layout[3].sender, TranscriptGrouping.Sender(kind: .member, id: "u-sam", name: "Sam Okafor", link: nil))
        XCTAssertTrue(layout[2].name && layout[2].avatar && layout[3].name && layout[3].avatar)
        XCTAssertEqual(feed.compactMap(\.text).map(TranscriptGrouping.feedPromptBody), [
            "ship it", "Also check the changelog.\nThanks!", "+1", "on it",
        ])
        // A plain prompt and a relay keep their rules.
        XCTAssertEqual(TranscriptGrouping.feedPromptBody("Message from Mira: hello"), "Message from Mira: hello")
        let relay = TranscriptGrouping.feedPrompt(id: "r", text: "Message from Parent (session s1): go", viewerId: viewer) { _ in nil }
        XCTAssertEqual(relay.item.authorSessionId, "s1")
        XCTAssertEqual(relay.session, TranscriptGrouping.KnownSession(id: "s1", title: "Parent", agentId: nil))

        // HANDOFF: the viewer's two consumed rows against the feed's echoes;
        // a teammate's same words are never the viewer's echo.
        func row(_ id: String, _ body: String) -> MessageDelivery.HandoffRow {
            MessageDelivery.HandoffRow(
                id: id, authorKind: "user", authorId: viewer, body: body,
                createdAt: "2026-11-13T10:00:00.000Z", consumedAt: "2026-11-13T10:00:05.000Z", hasCard: false
            )
        }
        let echoes = feed.compactMap(\.text).map { MessageDelivery.HandoffEcho(text: $0, at: nil) }
        XCTAssertEqual(
            MessageDelivery.ownHandoff(
                viewerId: viewer, now: "2026-11-13T10:00:10.000Z", feedEmpty: false,
                rows: [row("m1", "ship it"), row("m2", "on it"), row("m3", "+1")], echoes: echoes
            ),
            ["m3"]
        )
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
    // #EXP-1873: a quiet check-in is one line.
    func testReplaysEveryCheckInCase() throws {
        let cases = try XCTUnwrap(try fixture()["checkInCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let kinds = try XCTUnwrap(testCase["kinds"] as? [String]).map { kind in
                try XCTUnwrap(TranscriptGrouping.FeedRowRole(rawValue: kind), name)
            }
            let expected = try XCTUnwrap(testCase["expected"] as? [Any], name).map { value -> TranscriptGrouping.FeedRowShown in
                if let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID() {
                    return .checkIn(number.intValue)
                }
                return (value as? Bool) == true ? .row : .hidden
            }
            XCTAssertEqual(TranscriptGrouping.feedRows(kinds), expected, name)
        }
        let labels = try XCTUnwrap(try fixture()["checkInLabelCases"] as? [[String: Any]])
        XCTAssertFalse(labels.isEmpty)
        for testCase in labels {
            let name = testCase["name"] as? String ?? ""
            let count = try XCTUnwrap(testCase["count"] as? Int, name)
            XCTAssertEqual(TranscriptGrouping.checkInLabel(count: count, time: string(testCase["time"])), testCase["expected"] as? String, name)
        }
    }
    // #FEED-256 / #FEED-254: the blank-text rule.
    func testReplaysEveryBlankCase() throws {
        let blank = try XCTUnwrap(try fixture()["blank"] as? [String: Any])
        let points = try XCTUnwrap(blank["codePoints"] as? [String]).map { hex in
            try XCTUnwrap(UInt32(hex, radix: 16), hex)
        }
        XCTAssertEqual(Set(points), TranscriptGrouping.blankCodePoints)
        let cases = try XCTUnwrap(try fixture()["blankCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let text = try XCTUnwrap(testCase["text"] as? String, name)
            XCTAssertEqual(TranscriptGrouping.isBlankText(text), try XCTUnwrap(testCase["expected"] as? Bool, name), name)
        }
    }
    func testCheckInSpecimenMatchesFixture() throws {
        let rows = try XCTUnwrap(try fixture()["checkInSpecimen"] as? [[String: Any]])
        let want = try rows.map { row in
            TranscriptGrouping.CheckInSpecimenRow(
                kind: try XCTUnwrap(TranscriptGrouping.FeedRowRole(rawValue: try XCTUnwrap(row["kind"] as? String))),
                time: string(row["time"]),
                text: string(row["text"])
            )
        }
        XCTAssertEqual(TranscriptGrouping.checkInSpecimen, want)
        let agent = try XCTUnwrap(try fixture()["checkInSpecimenAgent"] as? [String: Any])
        XCTAssertEqual(
            TranscriptGrouping.checkInSpecimenAgent,
            TranscriptGrouping.CheckInSpecimenAgent(
                name: try XCTUnwrap(agent["name"] as? String), icon: string(agent["icon"]), color: string(agent["color"])
            )
        )
    }
    // #EXP-1864: a tool row named in `notices` is THE inline marker.
    func testReplaysEveryNoticeCase() throws {
        let fixture = try fixture()
        let notices = try XCTUnwrap(fixture["notices"] as? [String: String])
        XCTAssertEqual(notices, TranscriptGrouping.notices)
        let contractURL = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/contract.json")
        let contract = try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: contractURL)) as? [String: Any])
        let display = try XCTUnwrap(contract["expToolDisplay"] as? [String: Any])
        XCTAssertEqual(display["prefix"] as? String, "exponential_")
        let rows = try XCTUnwrap(display["tools"] as? [[String: Any]])
        for (row, label) in notices {
            let entry = try XCTUnwrap(rows.first { $0["name"] as? String == row }, row)
            XCTAssertEqual(entry["done"] as? String, label, row)
        }
        let cases = try XCTUnwrap(fixture["noticeCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = testCase["name"] as? String ?? ""
            let toolName = try XCTUnwrap(string(testCase["toolName"]), name)
            XCTAssertEqual(TranscriptGrouping.traceNotice(toolName), string(testCase["expected"]), name)
        }
    }
}
