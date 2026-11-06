import Foundation
import XCTest
@testable import ExpCore

// #EXP-1642: the system search model, replayed off `system-search.json`
// (TS `system-search.test.ts`, Android `SystemSearchTest`, same case names).
final class SystemSearchTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/system-search.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    func testConstantsAreTheFixtures() throws {
        let f = try fixture()
        let copy = try XCTUnwrap(f["copy"] as? [String: String])
        XCTAssertEqual(copy["issueSubtitle"], SystemSearch.issueSubtitle)
        XCTAssertEqual(copy["chatSubtitle"], SystemSearch.chatSubtitle)
        XCTAssertEqual(f["issuePrefix"] as? String, SystemSearch.issuePrefix)
        XCTAssertEqual(f["chatPrefix"] as? String, SystemSearch.chatPrefix)
        XCTAssertEqual(f["domainPrefix"] as? String, SystemSearch.domainPrefix)
        XCTAssertEqual(f["debounceMs"] as? Int, SystemSearch.debounceMs)
        XCTAssertEqual(f["maxIssues"] as? Int, SystemSearch.fixtureCaps.maxIssues)
        XCTAssertEqual(f["maxChats"] as? Int, SystemSearch.fixtureCaps.maxChats)
        let production = try XCTUnwrap(f["productionCaps"] as? [String: Int])
        XCTAssertEqual(production["maxIssues"], SystemSearch.productionCaps.maxIssues)
        XCTAssertEqual(production["maxChats"], SystemSearch.productionCaps.maxChats)
    }

    func testItemCases() throws {
        let cases = try XCTUnwrap(try fixture()["itemCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = c["name"] as? String ?? ""
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let boards = try XCTUnwrap(input["boards"] as? [[String: Any]]).map {
                SystemSearch.Board(id: $0["id"] as! String, name: $0["name"] as! String)
            }
            let issues = try XCTUnwrap(input["issues"] as? [[String: Any]]).map {
                SystemSearch.Issue(
                    id: $0["id"] as! String, identifier: $0["identifier"] as! String, title: $0["title"] as! String,
                    boardId: $0["boardId"] as! String, updatedAt: $0["updatedAt"] as! String,
                    trashed: $0["trashed"] as? Bool ?? false, archived: $0["archived"] as? Bool ?? false
                )
            }
            let chats = try XCTUnwrap(input["chats"] as? [[String: Any]]).map {
                ShareTargets.Chat(
                    id: $0["id"] as! String, title: $0["title"] as! String, updatedAt: $0["updatedAt"] as! String,
                    agentId: $0["agentId"] as? String, sealed: $0["sealed"] as? Bool ?? false
                )
            }
            let expected = try XCTUnwrap(c["expected"] as? [[String: Any]]).map {
                SystemSearch.Item(
                    id: $0["id"] as! String, title: $0["title"] as! String,
                    subtitle: $0["subtitle"] as! String, keywords: $0["keywords"] as! [String]
                )
            }
            XCTAssertEqual(SystemSearch.items(boards: boards, issues: issues, chats: chats), expected, name)
        }
    }

    func testParseCases() throws {
        let cases = try XCTUnwrap(try fixture()["parseCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let expected: SystemSearch.Target? = (c["expected"] as? [String: String]).map {
                $0["kind"] == "issue" ? .issue($0["id"]!) : .chat($0["id"]!)
            }
            XCTAssertEqual(SystemSearch.parse(c["input"] as! String), expected, c["name"] as? String ?? "")
        }
    }

    func testDomainsRoundTripTheAccount() {
        XCTAssertEqual(SystemSearch.domain(accountId: "abc"), "account:abc")
        XCTAssertEqual(SystemSearch.accountId(domain: "account:abc"), "abc")
        XCTAssertNil(SystemSearch.accountId(domain: "account:"))
        XCTAssertNil(SystemSearch.accountId(domain: "other"))
    }
}
