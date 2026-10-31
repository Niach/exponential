import Foundation
import XCTest
@testable import ExpCore

// #VAPP-185: Settings › MCP, locked ×4 (TS `settings-mcp.test.ts`, desktop
// `settings/mcp.rs`, Android `McpSettingsTest`) against
// fixtures/settings-mcp.json: the copy, the sections in order, the test ids
// and every caption case.
final class McpSettingsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/settings-mcp.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testCopyMatchesTheFixture() throws {
        let f = try fixture()
        XCTAssertEqual(McpSettings.title, f["title"] as? String)
        XCTAssertEqual(McpSettings.Section.allCases.map(\.rawValue), f["sections"] as? [String])
        let labels = try XCTUnwrap(f["labels"] as? [String: String])
        for section in McpSettings.Section.allCases {
            XCTAssertEqual(section.label, labels[section.rawValue], section.rawValue)
        }
        XCTAssertEqual(McpSettings.copy, f["copy"] as? String)
        XCTAssertEqual(McpSettings.copied, f["copied"] as? String)
        XCTAssertEqual(McpSettings.endpointPath, f["endpointPath"] as? String)
        XCTAssertEqual(McpSettings.revoke, f["revoke"] as? String)
        XCTAssertEqual(McpSettings.activity, f["activity"] as? String)
        XCTAssertEqual(McpSettings.never, f["never"] as? String)
        XCTAssertEqual(McpSettings.builtinApp, f["builtinApp"] as? String)
        XCTAssertEqual(McpSettings.firstParty, f["firstParty"] as? String)
        XCTAssertEqual(McpSettings.kindLabels, f["kindLabels"] as? [String: String])
        XCTAssertEqual(McpSettings.accessLabels, f["accessLabels"] as? [String: String])
        XCTAssertEqual(McpSettings.statusLabels, f["statusLabels"] as? [String: String])

        let scopes = try XCTUnwrap(f["scopes"] as? [[String: String]])
        XCTAssertEqual(McpSettings.Scope.allCases.map(\.rawValue), scopes.map { $0["value"] ?? "" })
        XCTAssertEqual(McpSettings.Scope.allCases.map(\.label), scopes.map { $0["label"] ?? "" })
        let levels = try XCTUnwrap(f["levels"] as? [[String: String]])
        XCTAssertEqual(McpSettings.Level.allCases.map(\.rawValue), levels.map { $0["value"] ?? "" })
        XCTAssertEqual(McpSettings.Level.allCases.map(\.label), levels.map { $0["label"] ?? "" })

        let toolCount = try XCTUnwrap(f["toolCount"] as? [String: String])
        XCTAssertEqual(McpSettings.toolCountLabel(1), toolCount["one"]?.replacingOccurrences(of: "{n}", with: "1"))
        XCTAssertEqual(McpSettings.toolCountLabel(3), toolCount["other"]?.replacingOccurrences(of: "{n}", with: "3"))
        let usedBy = try XCTUnwrap(f["usedBy"] as? String)
        XCTAssertEqual(McpSettings.usedByCaption(["X"]), usedBy.replacingOccurrences(of: "{apps}", with: "X"))
    }

    func testTestIdsMatchTheFixture() throws {
        let ids = try XCTUnwrap(try fixture()["testIds"] as? [String: String])
        XCTAssertEqual(McpSettings.TestId.page, ids["page"])
        XCTAssertEqual(McpSettings.TestId.activity, ids["activity"])
        for section in McpSettings.Section.allCases {
            XCTAssertEqual(section.testId, ids[section.rawValue], section.rawValue)
        }
    }

    func testUsedByCases() throws {
        let cases = try XCTUnwrap(try fixture()["usedByCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let apps = try XCTUnwrap(c["apps"] as? [String])
            XCTAssertEqual(McpSettings.usedByCaption(apps), c["expected"] as? String, c["name"] as? String ?? "")
        }
    }

    func testClientCaptionCases() throws {
        let cases = try XCTUnwrap(try fixture()["clientCaptionCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let client = try XCTUnwrap(c["client"] as? [String: Any])
            let scope = try XCTUnwrap(McpSettings.Scope(rawValue: c["scope"] as? String ?? ""))
            XCTAssertEqual(
                McpSettings.clientCaption(
                    scope: scope,
                    userName: client["userName"] as? String ?? "",
                    kinds: client["kinds"] as? [String] ?? [],
                    appsAccess: client["appsAccess"] as? String,
                    lastUsed: client["lastUsed"] as? String
                ),
                c["expected"] as? String,
                c["name"] as? String ?? ""
            )
        }
    }

    func testActivityCaptionCases() throws {
        let cases = try XCTUnwrap(try fixture()["activityCaptionCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let call = try XCTUnwrap(c["call"] as? [String: Any])
            XCTAssertEqual(
                McpSettings.activityCaption(
                    owner: c["owner"] as? Bool ?? false,
                    appName: call["appName"] as? String,
                    userName: call["userName"] as? String ?? "",
                    clientName: call["clientName"] as? String,
                    when: call["when"] as? String ?? ""
                ),
                c["expected"] as? String,
                c["name"] as? String ?? ""
            )
        }
    }

    func testBuiltinToolsFilter() {
        let tools = McpSettings.builtinTools
        XCTAssertEqual(tools.count, DomainContract.expToolNames.count)
        XCTAssertTrue(tools.allSatisfy { $0.name.hasPrefix(DomainContract.expToolPrefix) })
        XCTAssertEqual(McpSettings.filterBuiltinTools(tools, query: "  "), tools)
        XCTAssertTrue(McpSettings.filterBuiltinTools(tools, query: "zzzz-no-such-tool").isEmpty)
        XCTAssertFalse(McpSettings.filterBuiltinTools(tools, query: "ISSUES_CREATE").isEmpty)
    }

    func testEndpoint() {
        XCTAssertEqual(McpSettings.endpoint(instanceUrl: "https://app.exponential.at/"), "https://app.exponential.at/api/mcp/apps")
        XCTAssertEqual(McpSettings.endpoint(instanceUrl: "http://localhost:3000"), "http://localhost:3000/api/mcp/apps")
    }

    func testWireDecoding() throws {
        let clients = try JSONDecoder().decode(McpClientsList.self, from: Data("""
        {"canSeeEveryone":true,"clients":[{"clientId":"c1","name":"Claude Code","icon":null,"userId":"u1","userName":"Mia","userEmail":"m@x","kinds":["apps","builtin"],"appsAccess":"full","connectedAt":"2026-10-31T10:00:00.000Z","lastUsedAt":null}]}
        """.utf8))
        XCTAssertEqual(clients.clients.first?.id, "c1:u1")
        let access = try JSONDecoder().decode(McpAppAccess.self, from: Data("""
        {"canEdit":false,"apps":[{"id":"a1","slug":"m5","name":"m5 admin","icon":null,"color":null,"toolCount":3}],"rows":[{"userId":"u1","name":"Mia","email":"m@x","image":null,"role":"member","levels":{"a1":"read"}}]}
        """.utf8))
        XCTAssertEqual(access.rows.first?.level("a1"), .read)
        XCTAssertEqual(access.rows.first?.level("other"), .write)
        let server = try JSONDecoder().decode(McpServerRow.self, from: Data("""
        {"id":"s1","name":"Linear","url":"https://mcp.linear.app/sse","auth":"oauth","connection":{"status":"connected"},"usedBy":["m5 admin"]}
        """.utf8))
        XCTAssertEqual(server.usedBy, ["m5 admin"])
        let revoke = try JSONEncoder().encode(McpClientRevokeInput(clientId: "c1", teamId: "t1", userId: nil))
        XCTAssertFalse(String(decoding: revoke, as: UTF8.self).contains("userId"))
    }
}
