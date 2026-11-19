@testable import ExpCore
import Foundation
import XCTest

/// #EDE-130: `packages/domain-contract/fixtures/find-in-chat.json` — every
/// case (menu, query, stops, counter, step, keep, land, status, keys) and
/// the copy, constants and test ids.
final class FindInChatTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/find-in-chat.json")
        return try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
    }

    private func cases(_ name: String) throws -> [[String: Any]] {
        try XCTUnwrap(try fixture()[name] as? [[String: Any]], name)
    }

    private func stop(_ any: Any?) throws -> FindInChat.Stop {
        let o = try XCTUnwrap(any as? [String: Any])
        return FindInChat.Stop(
            key: try XCTUnwrap(o["key"] as? String), local: try XCTUnwrap(o["local"] as? Int),
            loaded: try XCTUnwrap(o["loaded"] as? Bool), group: o["group"] as? String,
            output: try XCTUnwrap(o["output"] as? Bool)
        )
    }

    private func stops(_ any: Any?) throws -> [FindInChat.Stop] {
        try XCTUnwrap(any as? [Any]).map { try stop($0) }
    }

    func testTheConstantsCopyAndTestIdsAreTheFixtures() throws {
        let f = try fixture()
        XCTAssertEqual(f["queryMaxChars"] as? Int, FindInChat.queryMaxChars)
        XCTAssertEqual(f["hitsMax"] as? Int, FindInChat.hitsMax)
        XCTAssertEqual(f["debounceMs"] as? Int, FindInChat.debounceMs)
        let copy = try XCTUnwrap(f["copy"] as? [String: String])
        let ours: [String: String] = [
            "menu": FindInChat.Copy.menu, "placeholder": FindInChat.Copy.placeholder,
            "counter": FindInChat.Copy.counter, "counterCompact": FindInChat.Copy.counterCompact,
            "noResults": FindInChat.Copy.noResults, "searchingOlder": FindInChat.Copy.searchingOlder,
            "searchingDevice": FindInChat.Copy.searchingDevice, "deviceOffline": FindInChat.Copy.deviceOffline,
            "outputHintOne": FindInChat.Copy.outputHintOne, "outputHintMany": FindInChat.Copy.outputHintMany,
            "previous": FindInChat.Copy.previous, "next": FindInChat.Copy.next, "close": FindInChat.Copy.close,
        ]
        XCTAssertEqual(copy, ours)
        let ids = try XCTUnwrap(f["testIds"] as? [String: String])
        XCTAssertEqual(ids, [
            "bar": FindInChat.TestIds.bar, "input": FindInChat.TestIds.input,
            "counter": FindInChat.TestIds.counter, "previous": FindInChat.TestIds.previous,
            "next": FindInChat.TestIds.next, "close": FindInChat.TestIds.close,
            "status": FindInChat.TestIds.status, "outputHint": FindInChat.TestIds.outputHint,
        ])
        let shortcut = try XCTUnwrap(f["shortcut"] as? [String: String])
        XCTAssertEqual(shortcut["mac"], FindInChat.shortcut(apple: true))
        XCTAssertEqual(shortcut["other"], FindInChat.shortcut(apple: false))
        XCTAssertEqual(FindInChat.outputHint(1), "1 match in output")
        XCTAssertEqual(FindInChat.outputHint(3), "3 matches in output")
        XCTAssertEqual(FindInChat.eventKey(sessionId: "s1", seq: 4), "e:s1:4")
        XCTAssertEqual(FindInChat.messageKey("a1"), "m:a1")
    }

    func testMenuCases() throws {
        for c in try cases("menuCases") {
            let name = c["name"] as? String ?? ""
            let entries = try XCTUnwrap(c["entries"] as? [String], name)
            let got = FindInChat.menu(kind: try XCTUnwrap(c["kind"] as? String), entries: entries, search: "search", separator: "separator")
            XCTAssertEqual(got, c["expected"] as? [String], name)
        }
    }

    func testQueryCases() throws {
        for c in try cases("queryCases") {
            XCTAssertEqual(FindInChat.query(try XCTUnwrap(c["raw"] as? String)), c["query"] as? String, c["name"] as? String ?? "")
        }
    }

    func testMatchCases() throws {
        for c in try cases("matchCases") {
            let name = c["name"] as? String ?? ""
            let rows = try XCTUnwrap(c["rows"] as? [[String: Any]], name).map { r in
                FindInChat.Row(key: r["key"] as? String ?? "", count: r["count"] as? Int ?? 0, group: r["group"] as? String, outputHit: r["outputHit"] as? Bool ?? false)
            }
            let older = try XCTUnwrap(c["older"] as? [[String: Any]], name).map { h in
                FindInChat.OlderHit(key: h["key"] as? String ?? "", output: h["output"] as? Bool ?? false)
            }
            XCTAssertEqual(FindInChat.stops(rows: rows, older: older), try stops(c["expected"]), name)
        }
    }

    func testCounterCases() throws {
        for c in try cases("counterCases") {
            let got = FindInChat.counter(
                current: c["current"] as? Int, total: c["total"] as? Int ?? 0,
                done: c["done"] as? Bool ?? false, compact: c["compact"] as? Bool ?? false
            )
            XCTAssertEqual(got, c["expected"] as? String, c["name"] as? String ?? "")
        }
    }

    func testStepCases() throws {
        for c in try cases("stepCases") {
            let step = try XCTUnwrap(FindInChat.Step(rawValue: c["step"] as? String ?? ""))
            let got = FindInChat.step(current: c["current"] as? Int, total: c["total"] as? Int ?? 0, step)
            XCTAssertEqual(got, c["expected"] as? Int, c["name"] as? String ?? "")
        }
    }

    func testResultCases() throws {
        for c in try cases("resultCases") {
            let got = FindInChat.keep(before: try stops(c["before"]), current: c["current"] as? Int, after: try stops(c["after"]))
            XCTAssertEqual(got, c["expected"] as? Int, c["name"] as? String ?? "")
        }
    }

    func testLandCases() throws {
        for c in try cases("landCases") {
            let name = c["name"] as? String ?? ""
            let expected = try XCTUnwrap(c["expected"] as? [String: Any], name)
            let got = FindInChat.land(try stop(c["match"]))
            XCTAssertEqual(expected["scroll"] as? String, "centre", name)
            XCTAssertEqual(got.openGroup, expected["openGroup"] as? String, name)
            XCTAssertEqual(got.pageIn, expected["pageIn"] as? Bool, name)
            XCTAssertEqual(got.ring, expected["ring"] as? Bool, name)
            XCTAssertEqual(got.current, expected["current"] as? Int, name)
        }
    }

    func testStatusCases() throws {
        for c in try cases("statusCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = FindInChat.status(
                store: try XCTUnwrap(FindInChat.Phase(rawValue: input["store"] as? String ?? "")),
                device: try XCTUnwrap(FindInChat.Phase(rawValue: input["device"] as? String ?? "")),
                deviceName: input["deviceName"] as? String ?? ""
            )
            XCTAssertEqual(got, c["expected"] as? String, c["name"] as? String ?? "")
        }
    }

    func testKeyCases() throws {
        for c in try cases("keyCases") {
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = FindInChat.key(
                input["key"] as? String ?? "", mod: input["mod"] as? Bool ?? false, shift: input["shift"] as? Bool ?? false,
                inChat: input["inChat"] as? Bool ?? false, inField: input["inField"] as? Bool ?? false
            )
            XCTAssertEqual(got?.rawValue, c["expected"] as? String, c["name"] as? String ?? "")
        }
    }

    func testOlderCases() throws {
        for c in try cases("olderCases") {
            let name = c["name"] as? String ?? ""
            let store = try XCTUnwrap(c["store"] as? [[String: Any]], name).map { h in
                FindInChat.StoreHit(sessionId: h["sessionId"] as? String ?? "", seq: h["seq"] as? Int, messageId: h["messageId"] as? String, kind: h["kind"] as? String ?? "")
            }
            let device = try XCTUnwrap(c["device"] as? [[String: Any]], name).map { h in
                FindInChat.DeviceHit(seq: h["seq"] as? Int ?? 0, kind: h["kind"] as? String ?? "")
            }
            let synced = (c["synced"] as? [String: Any]).map { s in
                FindInChat.Synced(loaded: s["loaded"] as? [String] ?? [], drawable: s["drawable"] as? [String] ?? [], more: s["more"] as? Bool ?? false)
            }
            let expected = try XCTUnwrap(c["expected"] as? [[String: Any]], name).map { h in
                FindInChat.OlderHit(key: h["key"] as? String ?? "", output: h["output"] as? Bool ?? false)
            }
            let got = FindInChat.older(sessionId: c["sessionId"] as? String ?? "", lowestSeq: c["lowestSeq"] as? Int, store: store, device: device, synced: synced)
            XCTAssertEqual(got, expected, name)
        }
    }

    func testTheStoresAnswerKeepsTheShownSessionsHits() {
        let json = #"{"hits":[{"rootId":"r","sessionId":"s1","kind":"narration","at":3,"seq":12,"snippet":"x","matchStart":0,"matchLen":1},{"rootId":"r","sessionId":"s2","kind":"tool","at":2,"seq":4,"snippet":"x","matchStart":0,"matchLen":1},{"rootId":"r","sessionId":"s1","kind":"message","at":2,"messageId":"m9","snippet":"x","matchStart":0,"matchLen":1},{"rootId":"r","sessionId":"s1","kind":"tool_output","at":1,"seq":30,"snippet":"x","matchStart":0,"matchLen":1},{"rootId":"r","sessionId":"s1","kind":"later","at":1,"seq":31,"snippet":"x","matchStart":0,"matchLen":1}],"total":5}"#
        XCTAssertEqual(FindInChat.storeHits(json, sessionId: "s1"), [
            FindInChat.StoreHit(sessionId: "s1", seq: 12, kind: "narration"),
            FindInChat.StoreHit(sessionId: "s1", messageId: "m9", kind: "message"),
            FindInChat.StoreHit(sessionId: "s1", seq: 30, kind: "tool_output"),
        ])
        XCTAssertEqual(FindInChat.storeHits(nil, sessionId: "s1"), [])
        XCTAssertEqual(
            FindInChat.deviceHits([(seq: 8, kind: "tool_output"), (seq: 7, kind: "narration"), (seq: 6, kind: "later")]),
            [FindInChat.DeviceHit(seq: 8, kind: "tool_output"), FindInChat.DeviceHit(seq: 7, kind: "narration")]
        )
    }
}
