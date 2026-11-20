@testable import ExpCore
import Foundation
import XCTest

/// #EDE-99: `packages/domain-contract/fixtures/search-list.json` — the Chats
/// section's `sectionCases`, `rowCases`, `pickCases` and `startCases`, and its
/// constants, copy, icons and test ids.
final class SearchListChatsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/search-list.json")
        return try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
    }

    private func cases(_ name: String) throws -> [[String: Any]] {
        try XCTUnwrap(try fixture()[name] as? [[String: Any]], name)
    }

    private func hit(_ any: Any?) throws -> SearchList.ChatHit {
        try XCTUnwrap((any as? [String: Any]).flatMap(SearchList.ChatHit.init))
    }

    func testTheConstantsCopyIconsAndTestIdsAreTheFixtures() throws {
        let chats = try XCTUnwrap(try fixture()["chats"] as? [String: Any])
        typealias G = SearchList.GlobalChats
        XCTAssertEqual(chats["debounceMs"] as? Int, G.debounceMs)
        XCTAssertEqual(chats["chatsMax"] as? Int, G.chatsMax)
        let row = try XCTUnwrap(chats["row"] as? [String: Double])
        XCTAssertEqual(row, ["chatHeight": G.chatHeight, "chatHeightPhone": G.chatHeightPhone])
        XCTAssertEqual(chats["icons"] as? [String: String], ["chat": G.Icons.chat, "agent": G.Icons.agent])
        XCTAssertEqual(chats["copy"] as? [String: String], [
            "issues": G.Copy.issues, "chats": G.Copy.chats, "you": G.Copy.you,
            "noResults": G.Copy.noResults, "placeholder": G.Copy.placeholder,
        ])
        XCTAssertEqual(chats["testIds"] as? [String: String], [
            "issues": G.TestIds.issues, "chats": G.TestIds.chats, "chatRow": G.TestIds.chatRow,
        ])
    }

    func testSectionCases() throws {
        for c in try cases("sectionCases") {
            let name = c["name"] as? String ?? ""
            let input = try XCTUnwrap(c["input"] as? [String: Any], name)
            let chats = (input["chats"] as? [String: Any]).map {
                (rows: $0["rows"] as? Int ?? 0, loading: $0["loading"] as? Bool ?? false)
            }
            let got = SearchList.globalSections(
                query: try XCTUnwrap(input["query"] as? String, name),
                issues: try XCTUnwrap(input["issues"] as? Int, name),
                chats: chats
            )
            let expected = try XCTUnwrap(c["expected"] as? [String: Any], name)
            XCTAssertEqual(got.titled, expected["titled"] as? Bool, name)
            XCTAssertEqual(got.count, expected["count"] as? Int, name)
            XCTAssertEqual(got.empty, expected["empty"] as? Bool, name)
            let sections = try XCTUnwrap(expected["sections"] as? [[String: Any]], name).map {
                SearchList.GlobalSection(
                    key: SearchList.GlobalSectionKey(rawValue: $0["key"] as? String ?? "") ?? .issues,
                    count: $0["count"] as? Int ?? -1, loading: $0["loading"] as? Bool ?? false
                )
            }
            XCTAssertEqual(got.sections, sections, name)
        }
    }

    func testRowCases() throws {
        for c in try cases("rowCases") {
            let name = c["name"] as? String ?? ""
            let input = try XCTUnwrap(c["input"] as? [String: Any], name)
            let chats = try XCTUnwrap(input["chats"] as? [[String: Any]], name).map {
                SearchList.ChatHitChat(rootId: $0["rootId"] as? String ?? "", count: $0["count"] as? Int ?? 0, hit: try hit($0["hit"]))
            }
            let roots = try XCTUnwrap(input["roots"] as? [String: [String: Any]], name).mapValues {
                SearchList.GlobalChatRoot(title: $0["title"] as? String ?? "", agent: $0["agent"] as? Bool ?? false)
            }
            let now = try XCTUnwrap(ISO8601DateFormatter().date(from: try XCTUnwrap(input["now"] as? String)), name)
            let got = SearchList.globalChatRows(
                query: try XCTUnwrap(input["query"] as? String, name),
                now: now,
                utcOffsetMinutes: try XCTUnwrap(input["utcOffsetMinutes"] as? Int, name),
                chats: chats,
                roots: roots,
                messageAuthors: input["messageAuthors"] as? [String: String] ?? [:],
                limit: try XCTUnwrap(input["limit"] as? Int, name)
            )
            let expected = try XCTUnwrap(c["expected"] as? [[String: String]], name)
            XCTAssertEqual(got.map {
                [
                    "key": $0.key, "rootId": $0.rootId, "icon": $0.icon, "title": $0.title,
                    "subtitle": $0.subtitle, "highlight": $0.highlight, "trailing": $0.trailing,
                ]
            }, expected, name)
        }
    }

    func testPickCases() throws {
        for c in try cases("pickCases") {
            let name = c["name"] as? String ?? ""
            let input = try XCTUnwrap(c["input"] as? [String: Any], name)
            let got = SearchList.globalChatPick(query: try XCTUnwrap(input["query"] as? String, name), hit: try hit(input["hit"]))
            let expected = try XCTUnwrap(c["expected"] as? [String: Any], name)
            let find = try XCTUnwrap(expected["find"] as? [String: Any], name)
            let target = try XCTUnwrap(find["target"] as? [String: Any], name)
            XCTAssertEqual(got.sessionId, expected["sessionId"] as? String, name)
            XCTAssertEqual(got.query, find["query"] as? String, name)
            XCTAssertEqual(got.target, SearchList.GlobalChatTarget(
                key: try XCTUnwrap(target["key"] as? String, name), output: try XCTUnwrap(target["output"] as? Bool, name)
            ), name)
        }
    }

    func testStartCases() throws {
        for c in try cases("startCases") {
            let name = c["name"] as? String ?? ""
            let stops = try XCTUnwrap(c["stops"] as? [[String: Any]], name).map {
                FindInChat.Stop(
                    key: $0["key"] as? String ?? "", local: $0["local"] as? Int ?? 0, loaded: $0["loaded"] as? Bool ?? false,
                    group: $0["group"] as? String, output: $0["output"] as? Bool ?? false
                )
            }
            let target = try XCTUnwrap(c["target"] as? [String: Any], name)
            let got = SearchList.globalChatStart(stops, target: SearchList.GlobalChatTarget(
                key: target["key"] as? String ?? "", output: target["output"] as? Bool ?? false
            ))
            XCTAssertEqual(got, c["expected"] as? Int, name)
        }
    }

    func testSearchAllTextParses() {
        let json = #"{"chats":[{"rootId":"r1","count":2,"hit":{"rootId":"r1","sessionId":"c1","kind":"tool_output","at":5,"seq":7,"snippet":"x","matchStart":0,"matchLen":1}}],"total":1}"#
        XCTAssertEqual(SearchList.chatHits(json), [SearchList.ChatHitChat(
            rootId: "r1", count: 2,
            hit: SearchList.ChatHit(rootId: "r1", sessionId: "c1", kind: "tool_output", at: 5, seq: 7, snippet: "x", matchStart: 0, matchLen: 1)
        )])
        XCTAssertEqual(SearchList.chatHits(nil), [])
    }
}
