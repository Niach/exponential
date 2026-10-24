import Foundation
import XCTest
@testable import ExpCore

// VAPP-119: the `exp:` sources, replayed ×4 against
// packages/domain-contract/fixtures/app-sources.json (TS `app-sources.test.ts`,
// desktop `domain::app_sources`, Android `AppSourcesTest`): every case of
// `cases` over `store`, plus the constants.
final class AppSourcesTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/app-sources.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func store(_ raw: [String: Any]) throws -> AppSources.Store {
        func list(_ key: String) throws -> [[String: Any]] { try XCTUnwrap(raw[key] as? [[String: Any]], key) }
        func s(_ row: [String: Any], _ key: String) -> String { row[key] as? String ?? "" }
        func o(_ row: [String: Any], _ key: String) -> String? { row[key] as? String }
        func n(_ row: [String: Any], _ key: String) -> Double? { (row[key] as? NSNumber)?.doubleValue }
        return AppSources.Store(
            boards: try list("boards").map {
                .init(id: s($0, "id"), slug: s($0, "slug"), name: s($0, "name"), prefix: s($0, "prefix"),
                      icon: o($0, "icon"), color: o($0, "color"), sortOrder: n($0, "sortOrder") ?? 0, deletedAt: o($0, "deletedAt"))
            },
            issues: try list("issues").map {
                .init(id: s($0, "id"), boardId: s($0, "boardId"), identifier: s($0, "identifier"), title: s($0, "title"),
                      status: s($0, "status"), priority: o($0, "priority"), assigneeId: o($0, "assigneeId"),
                      creatorId: o($0, "creatorId"), dueDate: o($0, "dueDate"), sortOrder: n($0, "sortOrder") ?? 0,
                      createdAt: s($0, "createdAt"), updatedAt: s($0, "updatedAt"))
            },
            members: try list("members").map { .init(userId: s($0, "userId"), role: s($0, "role")) },
            users: try list("users").map { .init(id: s($0, "id"), name: s($0, "name"), email: o($0, "email"), image: o($0, "image")) },
            sessions: try list("sessions").map {
                .init(id: s($0, "id"), parentId: o($0, "parentId"), agentId: o($0, "agentId"), appId: o($0, "appId"),
                      userId: o($0, "userId"), status: s($0, "status"), busy: $0["busy"] as? Bool ?? false,
                      needsInput: $0["needsInput"] as? Bool ?? false, title: o($0, "title"),
                      createdAt: s($0, "createdAt"), updatedAt: s($0, "updatedAt"))
            },
            comments: try list("comments").map {
                .init(id: s($0, "id"), issueId: s($0, "issueId"), authorId: o($0, "authorId"), parentId: o($0, "parentId"),
                      source: s($0, "source"), body: s($0, "body"), createdAt: s($0, "createdAt"))
            },
            attachments: try list("attachments").map {
                .init(id: s($0, "id"), issueId: o($0, "issueId"), commentId: o($0, "commentId"), uploaderId: o($0, "uploaderId"),
                      filename: s($0, "filename"), contentType: s($0, "contentType"), sizeBytes: n($0, "sizeBytes") ?? 0,
                      width: n($0, "width"), height: n($0, "height"), createdAt: s($0, "createdAt"))
            }
        )
    }

    func testConstantsMatchTheFixture() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture["sources"] as? [String], AppSources.kinds)
        let limits = try XCTUnwrap(fixture["limits"] as? [String: Any])
        XCTAssertEqual(limits["default"] as? Int, AppSources.limitDefault)
        XCTAssertEqual(limits["max"] as? Int, AppSources.limitMax)
    }

    func testCases() throws {
        let fixture = try fixture()
        let store = try store(try XCTUnwrap(fixture["store"] as? [String: Any]))
        let cases = try XCTUnwrap(fixture["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for item in cases {
            let name = try XCTUnwrap(item["name"] as? String)
            let value = AppSources.sourceValue(
                name: try XCTUnwrap(item["source"] as? String),
                params: try XCTUnwrap(item["params"] as? [String: String], name),
                store: store,
                me: item["me"] as? String
            )
            // CardJSON objects compare key-order-insensitively.
            let expected = CardJSON(any: item["expected"]) ?? .null
            XCTAssertEqual(value ?? .null, expected, name)
            // The text the host dedupes on parses back to the same value.
            let reparsed = try JSONSerialization.jsonObject(with: Data((value ?? .null).compactString.utf8), options: [.fragmentsAllowed])
            XCTAssertEqual(CardJSON(any: reparsed), expected, name)
        }
    }

    func testAnUnknownNameIsNull() throws {
        let store = try store(try XCTUnwrap(try fixture()["store"] as? [String: Any]))
        XCTAssertNil(AppSources.sourceValue(name: "devices", params: [:], store: store, me: "u1"))
        XCTAssertNil(AppSources.sourceValue(name: "nope", params: [:], store: AppSources.Store(), me: nil))
    }

    func testTheLimitIsCapped() {
        let issues = (0..<600).map { n in
            AppSources.Issue(
                id: String(format: "i%03d", n), boardId: "b", identifier: "X-\(n)", title: "t", status: "backlog",
                priority: nil, assigneeId: nil, creatorId: nil, dueDate: nil, sortOrder: Double(n),
                createdAt: "", updatedAt: ""
            )
        }
        let store = AppSources.Store(issues: issues)
        func rows(_ limit: String) -> Int {
            guard case let .object(o)? = AppSources.sourceValue(name: "issues", params: ["limit": limit], store: store, me: nil),
                  case let .array(r)? = o["rows"] else { return -1 }
            return r.count
        }
        XCTAssertEqual(rows("1000"), 500)
        XCTAssertEqual(rows("nope"), 50)
        XCTAssertEqual(rows("2.5"), 50)
        XCTAssertEqual(rows("0"), 50)
        XCTAssertEqual(rows("7"), 7)
    }
}
