import Foundation
import XCTest
@testable import ExpCore

// VAPP-120: typed host actions, replayed ×4 against
// packages/domain-contract/fixtures/app-actions.json (TS `app-actions.test.ts`,
// desktop `domain::app_actions`, Android `AppActionsTest`): `parseCases`,
// `grantCases`, `grantableCases`, `declaredCases`, `consentCases`, VAPP-158
// `ownerOnlyCases`, plus the list, the owner-only list, the tool map
// and the copy.
final class AppActionsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/app-actions.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let list = try XCTUnwrap(try fixture()[key] as? [[String: Any]])
        XCTAssertFalse(list.isEmpty, key)
        return list
    }

    func testListToolsAndCopyMatchTheFixture() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture["prefix"] as? String, AppActions.prefix)
        XCTAssertEqual(fixture["functions"] as? [String], AppActions.all)
        let tools = try XCTUnwrap(fixture["tools"] as? [String: [String: Any]])
        XCTAssertEqual(Set(tools.keys), Set(AppActions.all))
        for (name, target) in tools {
            let parsed = try XCTUnwrap(AppActions.parseActionName(name), name)
            XCTAssertEqual(parsed.tool, target["tool"] as? String, name)
            XCTAssertEqual(parsed.op, target["op"] as? String, name)
        }
        let copy = try XCTUnwrap(fixture["copy"] as? [String: String])
        XCTAssertEqual(copy["title"], AppActions.Copy.title)
        XCTAssertEqual(copy["hint"], AppActions.Copy.hint)
        XCTAssertEqual(copy["none"], AppActions.Copy.none)
        XCTAssertEqual(copy["granted"], AppActions.Copy.granted)
        XCTAssertEqual(copy["always"], AppActions.Copy.always)
        XCTAssertEqual(fixture["grantRole"] as? String, AppActions.grantRole)
        XCTAssertEqual(Set(try XCTUnwrap(fixture["destructiveWords"] as? [String])), AppActions.destructiveWords)
        XCTAssertEqual(try XCTUnwrap(fixture["destructiveArgs"] as? [String: [String]]), AppActions.destructiveArgs)
        XCTAssertEqual(Set(try XCTUnwrap(fixture["ownerActions"] as? [String])), AppActions.ownerActions)
    }

    func testOwnerOnlyCases() throws {
        for item in try cases("ownerOnlyCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            XCTAssertEqual(AppActions.actionOwnerOnly(name), try XCTUnwrap(item["expected"] as? Bool), name)
        }
        // An owner-only action is never destructive: it asks with the plain card.
        for name in AppActions.ownerActions {
            XCTAssertNotNil(AppActions.parseActionName(name), name)
            XCTAssertFalse(AppActions.callDestructive(name, args: [:]), name)
            XCTAssertFalse(AppActions.actionGrantable(name), name)
        }
    }

    func testParseCases() throws {
        for item in try cases("parseCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let parsed = AppActions.parseActionName(name)
            if let expected = item["expected"] as? [String: Any] {
                XCTAssertEqual(parsed?.tool, expected["tool"] as? String, name)
                XCTAssertEqual(parsed?.op, expected["op"] as? String, name)
            } else {
                XCTAssertNil(parsed, name)
            }
        }
    }

    func testGrantCases() throws {
        for item in try cases("grantCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let granted = try XCTUnwrap(item["granted"] as? [String])
            let args = (item["args"] as? [String: Any] ?? [:]).compactMapValues { CardJSON(any: $0) }
            XCTAssertEqual(AppActions.grantAllows(granted, name, args: args), try XCTUnwrap(item["expected"] as? Bool), "\(granted) \(name)")
        }
    }

    func testGrantableCases() throws {
        for item in try cases("grantableCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            XCTAssertEqual(AppActions.actionGrantable(name), try XCTUnwrap(item["expected"] as? Bool), name)
        }
    }

    func testCanGrantCases() throws {
        for item in try cases("canGrantCases") {
            let role = try XCTUnwrap(item["role"] as? String)
            XCTAssertEqual(AppActions.canGrantActions(role), try XCTUnwrap(item["expected"] as? Bool), role)
        }
        XCTAssertFalse(AppActions.canGrantActions(nil))
    }

    func testDeclaredCases() throws {
        for item in try cases("declaredCases") {
            let functions = try XCTUnwrap(item["functions"] as? [String])
            XCTAssertEqual(AppActions.declaredActions(functions), try XCTUnwrap(item["expected"] as? [String]), "\(functions)")
        }
    }

    /// The fixture's args in their written key order: `JSONSerialization`
    /// drops the order, so the expected compact JSON's own top-level key
    /// order restores it (every case's args are flat-keyed objects).
    private func ordered(_ args: [String: Any], like expected: String?) -> [(key: String, value: CardJSON)] {
        let keys = args.keys.sorted { a, b in
            let text = expected ?? ""
            let ia = text.range(of: "\"\(a)\":")?.lowerBound ?? text.endIndex
            let ib = text.range(of: "\"\(b)\":")?.lowerBound ?? text.endIndex
            return ia == ib ? a < b : ia < ib
        }
        return keys.map { (key: $0, value: CardJSON(any: args[$0]) ?? .null) }
    }

    func testConsentCases() throws {
        for item in try cases("consentCases") {
            let name = try XCTUnwrap(item["name"] as? String)
            let args = try XCTUnwrap(item["args"] as? [String: Any])
            guard let expected = item["expected"] as? [String: Any] else {
                XCTAssertNil(AppActions.actionConsent(name, args: ordered(args, like: nil)), name)
                continue
            }
            let expectedArgs = expected["args"] as? String
            let consent = try XCTUnwrap(AppActions.actionConsent(name, args: ordered(args, like: expectedArgs)), name)
            XCTAssertEqual(consent.tool, expected["tool"] as? String, name)
            XCTAssertEqual(consent.title, expected["title"] as? String, name)
            XCTAssertEqual(consent.destructive, expected["destructive"] as? Bool, name)
            XCTAssertEqual(consent.args, expectedArgs, name)
            // The unordered (host) form says the same, keys sorted.
            let unordered = try XCTUnwrap(AppActions.actionConsent(name, args: args.compactMapValues { CardJSON(any: $0) }))
            XCTAssertEqual(unordered.title, consent.title, name)
            XCTAssertEqual(unordered.destructive, consent.destructive, name)
            XCTAssertEqual(
                unordered.args.flatMap { try? JSONSerialization.jsonObject(with: Data($0.utf8)) as? NSDictionary },
                expectedArgs.flatMap { try? JSONSerialization.jsonObject(with: Data($0.utf8)) as? NSDictionary },
                name
            )
        }
    }

    func testTheNamesOpWinsOverAForgedOpInTheArgs() {
        // Review of #1222: a granted `exponential.issues.get` can never run a delete.
        let get = AppActions.parseActionName("exponential.issues.get")
        XCTAssertEqual(
            AppActions.arguments(["op": "delete", "id": "APP-5"], op: get?.op) { $0 },
            ["op": "get", "id": "APP-5"]
        )
        XCTAssertEqual(AppActions.arguments(["id": "APP-5"], op: get?.op) { $0 }, ["id": "APP-5", "op": "get"])
        let send = AppActions.parseActionName("exponential.sessions.send")
        XCTAssertNil(send?.op)
        XCTAssertEqual(AppActions.arguments(["text": "hi"], op: send?.op) { $0 }, ["text": "hi"])
        XCTAssertEqual(
            AppActions.arguments(["op": CardJSON.string("delete")], op: "get") { CardJSON.string($0) },
            ["op": CardJSON.string("get")]
        )
    }

    func testActionTitle() {
        XCTAssertEqual(AppActions.actionTitle("exponential.issues.update_status"), "Set issue status")
        XCTAssertEqual(AppActions.actionTitle("exponential.sessions.send"), "Send to session")
        XCTAssertEqual(AppActions.actionTitle("harness.toast"), "harness.toast")
    }

    func testConsentPromptKeepsTheGenericActionsWithTheTitle() throws {
        let consent = try XCTUnwrap(AppActions.actionConsent("exponential.issues.delete", args: [(key: "id", value: .string("APP-5"))]))
        let prompt = Prompts.exponentialUiConsent(tool: consent.tool, destructive: consent.destructive, title: consent.title)
        let generic = Prompts.exponentialUiConsent(tool: consent.tool, destructive: true)
        XCTAssertEqual(prompt.title, "Allow this surface to delete issue APP-5?")
        XCTAssertEqual(prompt.actions, generic.actions)
        XCTAssertEqual(prompt.focus, generic.focus)
        XCTAssertEqual(prompt.body, generic.body)
        XCTAssertEqual(Prompts.exponentialUiConsent(tool: "harness.mcp", destructive: false, title: nil), Prompts.exponentialUiConsent(tool: "harness.mcp", destructive: false))
    }
}
