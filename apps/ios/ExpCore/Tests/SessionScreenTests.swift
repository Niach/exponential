import Foundation
import XCTest
@testable import ExpCore

// #EXP-1732 THE session screen, locked ×4 against
// `packages/domain-contract/fixtures/session-screen.json`: every case, the
// WHOLE expected value. Read through `#filePath`: the unit-test bundle
// carries no repo resources.
final class SessionScreenTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/session-screen.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func cases(_ key: String) throws -> [[String: Any]] {
        let cases = try XCTUnwrap(try fixture()[key] as? [[String: Any]], key)
        XCTAssertFalse(cases.isEmpty, key)
        return cases
    }

    private func string(_ raw: Any?) -> String? { raw as? String }

    private func json(_ screen: SessionScreen.Screen) -> NSDictionary {
        [
            "tabs": screen.tabs.map { entry -> [String: Any] in
                [
                    "id": entry.id.rawValue, "label": entry.label, "count": entry.count.map { $0 as Any } ?? NSNull(),
                    "disabled": entry.disabled,
                ]
            },
            "active": screen.active.rawValue,
            "settings": screen.settings,
            "open": screen.open.map { $0.rawValue as Any } ?? NSNull(),
            "layout": screen.layout.rawValue,
            "collapse": screen.collapse,
        ] as NSDictionary
    }

    func testConstantsMatchTheFixture() throws {
        let f = try fixture()
        let copy = try XCTUnwrap(f["copy"] as? [String: Any])
        XCTAssertEqual(copy["tabs"] as? [String: String], SessionScreen.Copy.tabs)
        XCTAssertEqual(copy["settings"] as? String, SessionScreen.Copy.settings)
        XCTAssertEqual(copy["back"] as? String, SessionScreen.Copy.back)
        XCTAssertEqual(copy["collapse"] as? String, SessionScreen.Copy.collapse)
        let menu = try XCTUnwrap(copy["menu"] as? [String: Any])
        XCTAssertEqual(menu["linkIssue"] as? String, SessionScreen.Copy.Menu.linkIssue)
        XCTAssertEqual(menu["publish"] as? String, SessionScreen.Copy.Menu.publish)
        XCTAssertEqual(menu["compact"] as? String, SessionScreen.Copy.Menu.compact)
        XCTAssertEqual(menu["newConversation"] as? String, SessionScreen.Copy.Menu.newConversation)
        XCTAssertEqual(menu["settings"] as? [String: String], SessionScreen.Copy.Menu.settings)
        XCTAssertEqual(menu["export"] as? String, SessionScreen.Copy.Menu.export)
        XCTAssertEqual(menu["delete"] as? [String: String], SessionScreen.Copy.Menu.delete)
        XCTAssertEqual(f["menuIcons"] as? [String: String], SessionScreen.menuIcons)
        XCTAssertEqual(f["menuSends"] as? [String: String], SessionScreen.menuSends)
        XCTAssertEqual(copy["separator"] as? String, SessionScreen.Copy.separator)
        XCTAssertEqual(copy["linkIssue"] as? String, SessionScreen.Copy.linkIssue)
        XCTAssertEqual(copy["status"] as? [String: String], [
            "needsInput": SessionScreen.Copy.Status.needsInput, "working": SessionScreen.Copy.Status.working,
            "queued": SessionScreen.Copy.Status.queued, "done": SessionScreen.Copy.Status.done,
            "idle": SessionScreen.Copy.Status.idle,
        ])
        XCTAssertEqual(f["testIds"] as? [String: String], [
            "tabs": SessionScreen.TestIds.tabs,
            "beside": SessionScreen.TestIds.beside, "pane": SessionScreen.TestIds.pane,
            "issues": SessionScreen.TestIds.issues, "sessions": SessionScreen.TestIds.sessions,
            "collapse": SessionScreen.TestIds.collapse, "menu": SessionScreen.TestIds.menu,
        ])
        let kinds = try XCTUnwrap(f["kinds"] as? [String: [String: Any]])
        XCTAssertEqual(Set(kinds.keys), Set(SessionScreen.Kind.allCases.map(\.rawValue)))
        for (name, raw) in kinds {
            let spec = try XCTUnwrap(SessionScreen.Kind(rawValue: name).flatMap { SessionScreen.kinds[$0] }, name)
            XCTAssertEqual(raw["tabs"] as? [String], spec.tabs.map(\.rawValue), name)
            XCTAssertEqual(raw["settings"] as? Bool, spec.settings, name)
            XCTAssertEqual(raw["wideChatTab"] as? Bool, spec.wideChatTab, name)
            XCTAssertEqual(raw["wideDefault"] as? String, spec.wideDefault.rawValue, name)
        }
        XCTAssertEqual(f["counted"] as? [String], SessionScreen.counted.map(\.rawValue))
        XCTAssertEqual(f["legacyTabs"] as? [String: String], SessionScreen.legacyTabs)
        let beside = try XCTUnwrap(f["beside"] as? [String: NSNumber])
        XCTAssertEqual(beside["breakpoint"]?.doubleValue, SessionScreen.Beside.breakpoint)
        XCTAssertEqual(beside["chat"]?.doubleValue, SessionScreen.Beside.chat)
        XCTAssertEqual(beside["pane"]?.doubleValue, SessionScreen.Beside.pane)
        XCTAssertEqual(beside["chatMinWidth"]?.doubleValue, SessionScreen.Beside.chatMinWidth)
    }

    func testReplaysEveryCase() throws {
        for testCase in try cases("cases") {
            let name = string(testCase["name"]) ?? ""
            let input = try XCTUnwrap(testCase["input"] as? [String: Any], name)
            let counts = try XCTUnwrap(input["counts"] as? [String: Int], name)
            let screen = SessionScreen.screen(
                kind: try XCTUnwrap(string(input["kind"]), name),
                wide: try XCTUnwrap(input["wide"] as? Bool, name),
                requested: string(input["requested"]),
                open: string(input["open"]),
                counts: SessionScreen.Counts(
                    issues: counts["issues"] ?? 0, sessions: counts["sessions"] ?? 0, results: counts["results"] ?? 0
                )
            )
            let expected = try XCTUnwrap(testCase["expected"] as? NSDictionary, name)
            XCTAssertEqual(json(screen), expected, name)
        }
    }

    func testReplaysEveryMenuCase() throws {
        for testCase in try cases("menuCases") {
            let name = string(testCase["name"]) ?? ""
            let input = try XCTUnwrap(testCase["input"] as? [String: Any], name)
            let entries = SessionScreen.menu(
                kind: try XCTUnwrap(string(input["kind"]), name),
                phone: try XCTUnwrap(input["phone"] as? Bool, name),
                member: try XCTUnwrap(input["member"] as? Bool, name),
                linkIssue: try XCTUnwrap(input["linkIssue"] as? Bool, name),
                settings: try XCTUnwrap(input["settings"] as? Bool, name),
                publish: try XCTUnwrap(input["publish"] as? Bool, name),
                export: try XCTUnwrap(input["export"] as? Bool, name),
                delete: try XCTUnwrap(input["delete"] as? Bool, name)
            )
            let expected = try XCTUnwrap(testCase["expected"] as? [String], name)
            XCTAssertEqual(entries.map(\.rawValue), expected, name)
        }
    }

    func testReplaysEveryKindCase() throws {
        for testCase in try cases("kindCases") {
            let name = string(testCase["name"]) ?? ""
            let input = try XCTUnwrap(testCase["input"] as? [String: Any], name)
            let kind = SessionScreen.kind(
                agentId: string(input["agentId"]),
                appId: string(input["appId"]),
                assistant: try XCTUnwrap(input["assistant"] as? Bool, name)
            )
            XCTAssertEqual(kind.rawValue, string(testCase["expected"]), name)
        }
    }

    func testReplaysEverySubtitleCase() throws {
        for testCase in try cases("subtitleCases") {
            let name = string(testCase["name"]) ?? ""
            let input = try XCTUnwrap(testCase["input"] as? [String: Any], name)
            let subtitle = SessionScreen.subtitle(
                status: string(input["status"]), device: string(input["device"]), trigger: string(input["trigger"])
            )
            XCTAssertEqual(subtitle, string(testCase["expected"]), name)
        }
    }

    func testReplaysEveryStatusCase() throws {
        for testCase in try cases("statusCases") {
            let name = string(testCase["name"]) ?? ""
            let input = try XCTUnwrap(testCase["input"] as? [[String: Any]], name)
            let sessions = try input.map { raw in
                SessionScreen.StatusInput(
                    status: try XCTUnwrap(string(raw["status"]), name),
                    busy: try XCTUnwrap(raw["busy"] as? Bool, name),
                    needsInput: try XCTUnwrap(raw["needsInput"] as? Bool, name),
                    root: try XCTUnwrap(raw["root"] as? Bool, name)
                )
            }
            XCTAssertEqual(SessionScreen.status(sessions), string(testCase["expected"]), name)
        }
    }
}
