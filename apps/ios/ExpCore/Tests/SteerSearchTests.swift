@testable import ExpCore
import Foundation
import XCTest

/// #EDE-96: `packages/domain-contract/fixtures/steer-enet.json` SEARCH — the
/// ask's shape, the answer's parse (every case's expected answer), the
/// constants.
final class SteerSearchTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/steer-enet.json")
        return try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
    }

    func testTheConstantsAndFramesAreTheFixtures() throws {
        let f = try fixture()
        let search = try XCTUnwrap(f["search"] as? [String: Any])
        XCTAssertEqual(search["hitsMax"] as? Int, SteerSearch.hitsMax)
        XCTAssertEqual(f["sendTimeoutMs"] as? Int, SteerSearch.timeoutMs)
        XCTAssertTrue((f["viewerFrames"] as? [String] ?? []).contains("search"))
        XCTAssertTrue((f["deviceFrames"] as? [String] ?? []).contains("search_result"))
        let ask = SteerSearch.frame(requestId: "s1", query: "login", beforeSeq: 40)
        XCTAssertEqual(ask["t"] as? String, "search")
        XCTAssertEqual(ask["requestId"] as? String, "s1")
        XCTAssertEqual(ask["query"] as? String, "login")
        XCTAssertEqual(ask["beforeSeq"] as? Int, 40)
        XCTAssertEqual(ask["limit"] as? Int, 50)
        XCTAssertNil(SteerSearch.frame(requestId: "s2", query: "x", beforeSeq: nil)["beforeSeq"])
    }

    func testEveryCasesAnswerParses() throws {
        let search = try XCTUnwrap(try fixture()["search"] as? [String: Any])
        for c in try XCTUnwrap(search["cases"] as? [[String: Any]]) {
            let name = c["name"] as? String ?? ""
            var body = try XCTUnwrap(c["expect"] as? [String: Any])
            body["t"] = "search_result"
            body["requestId"] = "r"
            let data = try JSONSerialization.data(withJSONObject: body)
            let frame = try XCTUnwrap(SteerWireFrame.parse(String(decoding: data, as: UTF8.self)), name)
            let result = try XCTUnwrap(frame.searchResult, name)
            XCTAssertEqual(frame.requestId, "r", name)
            XCTAssertEqual(result.hits.count, (body["hits"] as? [Any])?.count, name)
            XCTAssertEqual(result.done, body["done"] as? Bool, name)
            XCTAssertEqual(result.scannedToSeq, body["scannedToSeq"] as? Int, name)
            XCTAssertEqual(result.total, body["total"] as? Int, name)
        }
    }

    func testAnUnknownHitKindIsSkipped() throws {
        let frame = try XCTUnwrap(SteerWireFrame.parse(
            #"{"t":"search_result","requestId":"r","hits":[{"seq":3,"kind":"tool","snippet":"Bash","matchStart":0,"matchLen":4},{"seq":2,"kind":"later","snippet":"x","matchStart":0,"matchLen":1}],"done":true,"scannedToSeq":2}"#
        ))
        XCTAssertEqual(frame.searchResult?.hits, [SteerSearchHit(seq: 3, kind: "tool", snippet: "Bash", matchStart: 0, matchLen: 4)])
        XCTAssertNil(try XCTUnwrap(SteerWireFrame.parse(#"{"t":"keepalive"}"#)).searchResult)
    }
}
