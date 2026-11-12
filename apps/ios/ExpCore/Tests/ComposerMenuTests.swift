import Foundation
import XCTest
@testable import ExpCore

// EXP-1249: the composer's "+" menu, locked ×4 against
// `composer-menu.json` — same case names as web `composer-menu.test.ts`.
final class ComposerMenuTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/composer-menu.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    /// EXP-1635: the phones' Photo or file sub-choice.
    func testAddFilePhoneChoicesMatchTheFixture() throws {
        let addFile = try XCTUnwrap(try fixture()["addFile"] as? [String: Any])
        let choices = try XCTUnwrap(addFile["phoneChoices"] as? [[String: String]])
        XCTAssertEqual(choices.map { $0["id"] ?? "" }, ComposerMenu.AddFileChoice.allCases.map(\.rawValue))
        XCTAssertEqual(choices.map { $0["label"] ?? "" }, ComposerMenu.AddFileChoice.allCases.map(\.label))
        XCTAssertEqual(choices.map { $0["icon"] ?? "" }, ComposerMenu.AddFileChoice.allCases.map(\.icon))
    }

    private func wire(_ layout: [ComposerMenu.Entry]) -> [String] {
        layout.map { entry in
            switch entry {
            case let .row(row): row.id.rawValue
            case .separator: "-"
            }
        }
    }

    func testComposerMenuLayoutMatchesTheFixtureCases() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let surface = try XCTUnwrap(ComposerMenu.Surface(rawValue: try XCTUnwrap(testCase["surface"] as? String)), name)
            let c = try XCTUnwrap(testCase["conditions"] as? [String: Bool], name)
            // #EXP-1715: a case names the conditions it sets (absent = false).
            XCTAssertTrue(Set(c.keys).isSubset(of: Set(ComposerMenu.Condition.allCases.map(\.rawValue))), name)
            let layout = ComposerMenu.composerMenuLayout(ComposerMenu.Conditions(
                temporary: c["temporary"] ?? false,
                device: c["device"] ?? false,
                account: c["account"] ?? false,
                model: c["model"] ?? false,
                repositories: c["repositories"] ?? false,
                ultracode: c["ultracode"] ?? false,
                mcp: c["mcp"] ?? false,
                mcpPick: c["mcpPick"] ?? false,
                skills: c["skills"] ?? false,
                computerUse: c["computerUse"] ?? false,
                prompts: c["prompts"] ?? false,
                assistant: c["assistant"] ?? false,
                teams: c["teams"] ?? false
            ), surface: surface)
            XCTAssertEqual(wire(layout), testCase["expected"] as? [String], name)
        }
    }

    func testTheRowsCopyGlyphsKindsSurfacesAndConditionsAreTheFixtures() throws {
        let fixture = try fixture()
        XCTAssertEqual(
            Set((try XCTUnwrap(fixture["conditions"] as? [String: Any])).keys),
            Set(ComposerMenu.Condition.allCases.map(\.rawValue))
        )
        let all = try XCTUnwrap(fixture["rows"] as? [[String: Any]])
        // Separators sit where the fixture puts them.
        let layout = ComposerMenu.composerMenuLayout(ComposerMenu.Conditions(
            temporary: true, device: true, account: true, model: true, repositories: true,
            ultracode: true, mcp: true, mcpPick: true, skills: true, computerUse: true, prompts: true,
            assistant: true, teams: true
        ), surface: .launch)
        XCTAssertEqual(wire(layout), all.map { ($0["id"] as? String) ?? "-" })
        let rows = all.filter { $0["kind"] as? String != "separator" }
        XCTAssertEqual(ComposerMenu.allRows.count, rows.count)
        for (row, want) in zip(ComposerMenu.allRows, rows) {
            XCTAssertEqual(row.id.rawValue, want["id"] as? String)
            XCTAssertEqual(row.kind.rawValue, want["kind"] as? String)
            XCTAssertEqual(row.label, want["label"] as? String)
            XCTAssertEqual(row.codexLabel, want["codexLabel"] as? String)
            XCTAssertEqual(row.icon, want["icon"] as? String)
            XCTAssertEqual(row.surfaces.map(\.rawValue), want["surfaces"] as? [String], row.id.rawValue)
            let when: [String] = (want["when"] as? [String]) ?? (want["when"] as? String).map { [$0] } ?? []
            XCTAssertEqual(row.when.map(\.rawValue), when, row.id.rawValue)
        }
        XCTAssertEqual(ComposerMenu.row(.effort).label(codex: true), "Reasoning")
        XCTAssertNil(ComposerMenu.RowId(rawValue: "message-agent"))
    }

    func testNamesTheContractsComputerUsePayloadKeyAndCap() throws {
        let block = try XCTUnwrap(try fixture()["computerUse"] as? [String: Any])
        XCTAssertEqual(ComposerMenu.computerUsePayloadKey, block["payloadKey"] as? String)
        XCTAssertEqual(ComposerMenu.computerUseRunCap, block["cap"] as? String)
        XCTAssertTrue(DomainContract.codingSessionLaunchKeys.contains(ComposerMenu.computerUsePayloadKey))
    }

    func testSpellsTheRowTestIdsFromTheFixtureTemplate() throws {
        let ids = try XCTUnwrap(try fixture()["testIds"] as? [String: String])
        XCTAssertEqual(ComposerMenu.plusTestId, ids["plus"])
        XCTAssertEqual(ComposerMenu.menuTestId, ids["menu"])
        XCTAssertEqual(ComposerMenu.implementSubmitTestId, ids["implementSubmit"])
        XCTAssertEqual(ComposerMenu.submenuSearchTestId, ids["submenuSearch"])
        // EXP-1253/EXP-1270: the suggestion rows and the watermark are gone.
        XCTAssertNil(ids["suggestion"])
        XCTAssertNil(ids["brandMark"])
        for id in ComposerMenu.RowId.allCases {
            XCTAssertEqual(
                ComposerMenu.rowTestId(id),
                ids["row"]?.replacingOccurrences(of: "{id}", with: id.rawValue)
            )
        }
        XCTAssertEqual(ComposerMenu.plusLabel, try fixture()["plusLabel"] as? String)
        XCTAssertNil(try fixture()["suggestions"])
        XCTAssertEqual(ComposerMenu.submenuRowHeight, (try fixture()["submenuRowHeight"] as? NSNumber)?.doubleValue)
        XCTAssertEqual(ComposerMenu.submenuMaxRows, try fixture()["submenuMaxRows"] as? Int)
    }

    /// #EXP-1715: `teamsPick.valueCases` and its copy.
    func testTeamsPickValueReplaysEveryCase() throws {
        let block = try XCTUnwrap(try fixture()["teamsPick"] as? [String: Any])
        let copy = try XCTUnwrap(block["copy"] as? [String: String])
        XCTAssertEqual(ComposerMenu.TeamsPickCopy.all, copy["all"])
        XCTAssertEqual(ComposerMenu.TeamsPickCopy.many, copy["many"])
        let cases = try XCTUnwrap(block["valueCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let teams = try XCTUnwrap(testCase["teams"] as? [[String: String]]).map {
                ComposerMenu.PickTeam(id: $0["id"] ?? "", name: $0["name"] ?? "")
            }
            XCTAssertEqual(
                ComposerMenu.teamsPickValue(testCase["picked"] as? [String], teams: teams),
                testCase["expected"] as? String,
                name
            )
        }
    }

    /// #EXP-1715: every team (or none) stores as nil; chips only on a subset.
    func testTeamsPickNextAndChips() {
        let teams = [
            ComposerMenu.PickTeam(id: "t1", name: "Acme"),
            ComposerMenu.PickTeam(id: "t2", name: "Feedback"),
            ComposerMenu.PickTeam(id: "t3", name: "Ops"),
        ]
        XCTAssertNil(ComposerMenu.nextTeamsPick(["t3", "t1", "t2"], teams: teams))
        XCTAssertNil(ComposerMenu.nextTeamsPick([], teams: teams))
        XCTAssertEqual(ComposerMenu.nextTeamsPick(["t3", "t1"], teams: teams), ["t1", "t3"])
        XCTAssertEqual(ComposerMenu.pickedTeamIds(nil, teams: teams), ["t1", "t2", "t3"])
        XCTAssertEqual(ComposerMenu.teamsPickChips(nil, teams: teams), [])
        XCTAssertEqual(ComposerMenu.teamsPickChips(["t2"], teams: teams).map(\.id), ["t2"])
        XCTAssertEqual(ComposerMenu.removingTeamChip("t1", picked: ["t1", "t3"], teams: teams), ["t3"])
        XCTAssertNil(ComposerMenu.removingTeamChip("t2", picked: ["t2"], teams: teams))
        XCTAssertEqual(
            ComposerMenu.pickTeams([.init(id: "b", name: "ops"), .init(id: "a", name: "Acme")]).map(\.id),
            ["a", "b"]
        )
    }

    func testImplementButtonLabel() throws {
        let cases = try XCTUnwrap(try fixture()["implementLabels"] as? [[String: Any]])
        for testCase in cases {
            let count = try XCTUnwrap(testCase["count"] as? Int)
            XCTAssertEqual(ComposerMenu.implementButtonLabel(count), testCase["expected"] as? String)
        }
    }

    func testMcpServerRowsReadinessAndPreselect() throws {
        let rows = try JSONDecoder().decode([McpServerRow].self, from: Data("""
            [{"id":"a","name":"Linear","url":"https://mcp.linear.app/sse","enabledByDefault":true,"connection":{"status":"connected"}},
             {"id":"b","name":"Sentry","enabledByDefault":true,"connection":{"status":"expired"}},
             {"id":"c","name":"Local","command":"npx srv","connection":{"status":"not_needed"}}]
            """.utf8))
        XCTAssertNil(McpServers.notReadyLabel(rows[0]))
        XCTAssertEqual(McpServers.notReadyLabel(rows[1]), "Reconnect first")
        XCTAssertEqual(McpServers.notReadyLabel(McpServerRow(id: "d", name: "x", connection: .init(status: "missing"))), "Connect first")
        XCTAssertEqual(McpServers.preselect(rows, saved: nil), ["a"])
        XCTAssertEqual(McpServers.preselect(rows, saved: ["b", "c"]), ["c"])
        XCTAssertEqual(McpServers.location(rows[0]), "mcp.linear.app")
        XCTAssertEqual(McpServers.location(rows[2]), "npx srv")
        XCTAssertNil(McpServers.pickedValue([]))
        XCTAssertEqual(McpServers.pickedValue(["a", "c"]), "2")
    }

    // #EXP-1793: an agent chat's MCP pick rides a send only while it differs
    // from its seed (an empty pick clears), then becomes the seed.
    func testAgentChatMcpPickCarriesOnlyAChange() {
        var pick = AgentChatMcpPick(agentId: "g1", seed: ["a"])
        XCTAssertNil(pick.outgoing)
        pick.picked = ["a", "b"]
        XCTAssertEqual(pick.outgoing, ["a", "b"])
        pick.sent()
        XCTAssertNil(pick.outgoing)
        pick.picked = []
        XCTAssertEqual(pick.outgoing, [])
        // A pending pick survives a reseed; an untouched one follows it.
        pick.reseed(["c"])
        XCTAssertEqual(pick.picked, [])
        XCTAssertEqual(pick.outgoing, [])
        pick.picked = ["c"]
        pick.reseed(["d"])
        XCTAssertEqual(pick.picked, ["d"])
        XCTAssertNil(pick.outgoing)
    }

    func testAnAgentSendCarriesItsMcpPick() throws {
        let encode = { (input: SessionsSendInput) throws -> [String: Any] in
            try XCTUnwrap(JSONSerialization.jsonObject(with: try JSONEncoder().encode(input)) as? [String: Any])
        }
        let picked = try encode(.agent(agentId: "g1", text: "hi", mcpServerIds: ["m1"]))
        XCTAssertEqual(picked["mcpServerIds"] as? [String], ["m1"])
        let cleared = try encode(.agent(agentId: "g1", text: "hi", mcpServerIds: []))
        XCTAssertEqual(cleared["mcpServerIds"] as? [String], [])
        XCTAssertNil(try encode(.agent(agentId: "g1", text: "hi"))["mcpServerIds"])
    }
}
