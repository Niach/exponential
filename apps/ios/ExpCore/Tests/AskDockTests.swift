import Foundation
import XCTest
@testable import ExpCore

// THE ask dock, locked ×4 against
// `packages/domain-contract/fixtures/ask-dock.json` — every case array, the
// WHOLE expected object. Read through `#filePath`: the unit-test bundle
// carries no repo resources.
final class AskDockTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/ask-dock.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func list(_ key: String) throws -> [[String: Any]] {
        try XCTUnwrap(try fixture()[key] as? [[String: Any]])
    }

    private func pending(_ raw: [String: Any]) throws -> AskDock.Pending {
        AskDock.Pending(
            id: try XCTUnwrap(raw["id"] as? String),
            kind: try XCTUnwrap((raw["kind"] as? String).flatMap(AskDock.Kind.init(rawValue:))),
            text: try XCTUnwrap(raw["text"] as? String),
            sessionName: raw["sessionName"] as? String,
            createdAt: try XCTUnwrap((raw["createdAt"] as? NSNumber)?.doubleValue)
        )
    }

    func testConstantsMatchTheFixture() throws {
        let f = try fixture()
        let copy = try XCTUnwrap(f["copy"] as? [String: String])
        XCTAssertEqual(copy["planReady"], AskDock.Copy.planReady)
        XCTAssertEqual(copy["question"], AskDock.Copy.question)
        XCTAssertEqual(copy["back"], AskDock.Copy.back)
        XCTAssertEqual(copy["feedbackPlaceholder"], AskDock.Copy.feedbackPlaceholder)
        let ids = try XCTUnwrap(f["testIds"] as? [String: String])
        XCTAssertEqual(ids["dock"], AskDock.TestIds.dock)
        XCTAssertEqual(ids["row"], AskDock.TestIds.row)
        XCTAssertEqual(ids["back"], AskDock.TestIds.back)
        XCTAssertEqual(ids["card"], AskDock.TestIds.card)
        XCTAssertEqual((f["bodyMaxHeightFraction"] as? NSNumber)?.doubleValue, AskDock.bodyMaxHeightFraction)
        XCTAssertEqual((f["stackGap"] as? NSNumber)?.doubleValue, AskDock.stackGap)
        let options = try list("planOptions").map { raw in
            AskDock.PlanOption(
                key: try XCTUnwrap(raw["key"] as? String),
                label: try XCTUnwrap(raw["label"] as? String),
                description: raw["description"] as? String,
                decision: try XCTUnwrap(raw["decision"] as? String),
                fresh: try XCTUnwrap(raw["fresh"] as? Bool)
            )
        }
        XCTAssertEqual(options, AskDock.planOptions)
        XCTAssertTrue(AskDock.planOptions.contains { $0.key == AskDock.planRejectKey && $0.decision == "rejected" })
        let icons = try XCTUnwrap(f["icons"] as? [String: String])
        XCTAssertEqual(icons["plan"], "coding-plan")
        XCTAssertEqual(icons["question"], "ui-help")
        XCTAssertEqual(icons["back"], "ui-back")
        XCTAssertEqual(icons["open"], "ui-chevron-right")
    }

    func testCases() throws {
        let all = try list("cases")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let name = c["name"] as? String ?? ""
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let got = AskDock.dock(
                pending: try XCTUnwrap(input["pending"] as? [[String: Any]]).map(pending),
                focusedId: input["focusedId"] as? String
            )
            let e = try XCTUnwrap(c["expected"] as? [String: Any])
            let expected = AskDock.Layout(
                mode: try XCTUnwrap((e["mode"] as? String).flatMap(AskDock.Mode.init(rawValue:))),
                composerHidden: try XCTUnwrap(e["composerHidden"] as? Bool),
                back: try XCTUnwrap(e["back"] as? Bool),
                cardId: e["cardId"] as? String,
                rows: try XCTUnwrap(e["rows"] as? [[String: Any]]).map { r in
                    AskDock.Row(
                        id: try XCTUnwrap(r["id"] as? String),
                        kind: try XCTUnwrap((r["kind"] as? String).flatMap(AskDock.Kind.init(rawValue:))),
                        label: try XCTUnwrap(r["label"] as? String),
                        title: try XCTUnwrap(r["title"] as? String),
                        caption: r["caption"] as? String
                    )
                }
            )
            XCTAssertEqual(got, expected, name)
        }
    }

    func testTitleCases() throws {
        let all = try list("titleCases")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let text = try XCTUnwrap(c["text"] as? String)
            XCTAssertEqual(AskDock.title(text), try XCTUnwrap(c["expected"] as? String), text)
        }
    }

    func testDecisionLabelCases() throws {
        let all = try list("decisionLabelCases")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let got = AskDock.planDecisionLabel(
                decision: try XCTUnwrap(c["decision"] as? String),
                fresh: try XCTUnwrap(c["fresh"] as? Bool)
            )
            XCTAssertEqual(got, try XCTUnwrap(c["expected"] as? String))
        }
    }

    private func row(_ raw: [String: Any]) throws -> ThreadRow {
        let id = try XCTUnwrap(raw["id"] as? String)
        switch raw["type"] as? String {
        case "plan":
            return .plan(ThreadPlanRow(
                id: id,
                sessionId: try XCTUnwrap(raw["sessionId"] as? String),
                plan: try XCTUnwrap(raw["plan"] as? String),
                decision: raw["decision"] as? String,
                fresh: raw["fresh"] as? Bool ?? false
            ))
        case "question":
            return .question(ThreadQuestionRow(
                id: id,
                sessionId: try XCTUnwrap(raw["sessionId"] as? String),
                question: try XCTUnwrap(raw["question"] as? String),
                options: [],
                custom: raw["custom"] as? Bool ?? false,
                answer: raw["answer"] as? String,
                answered: try XCTUnwrap(raw["answered"] as? Bool)
            ))
        default:
            return .message(id: id, author: "user", authorId: nil, body: "", queued: false, authorSessionId: nil)
        }
    }

    func testThreadCases() throws {
        let all = try list("threadCases")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let rows = try XCTUnwrap(c["rows"] as? [[String: Any]]).map(row)
            let names = c["sessionNames"] as? [String: String] ?? [:]
            let got = AskDock.threadPendingAsks(rows) { names[$0] }
            let expected = try XCTUnwrap(c["expected"] as? [[String: Any]]).map(pending)
            XCTAssertEqual(got, expected, c["name"] as? String ?? "")
        }
    }

    func testSessionNameCases() throws {
        let all = try list("sessionNameCases")
        XCTAssertFalse(all.isEmpty)
        for c in all {
            let tabs = try XCTUnwrap(c["tabs"] as? [[String: Any]]).compactMap { raw -> ThreadTab? in
                switch raw["kind"] as? String {
                case "session":
                    let nested = (raw["nested"] as? [[String: Any]] ?? []).map { n in
                        ThreadTabNestedSession(
                            sessionId: n["sessionId"] as? String ?? "",
                            name: n["name"] as? String ?? "",
                            markState: (n["markState"] as? String).flatMap(SessionState.init(rawValue:)) ?? .waiting,
                            depth: (n["depth"] as? NSNumber)?.intValue ?? 1
                        )
                    }
                    return .session(
                        sessionId: raw["sessionId"] as? String ?? "",
                        name: raw["name"] as? String ?? "",
                        markState: (raw["markState"] as? String).flatMap(SessionState.init(rawValue:)) ?? .waiting,
                        nested: nested
                    )
                case "guide": return .guide(dot: raw["dot"] as? Bool ?? false)
                default: return .conversation
                }
            }
            let name = AskDock.threadSessionNames(tabs)
            let lookups = try XCTUnwrap(c["lookups"] as? [String: Any])
            for (id, expected) in lookups {
                XCTAssertEqual(name(id), expected as? String, "\(c["name"] ?? "") \(id)")
            }
        }
    }

    /// EXP-1280 (web `anchorToolCards`): a tool-posted card sits after the
    /// settled `plan_propose` / `sessions_ask` call that posted it, pairing
    /// from the newest back; a failed or running call posted nothing; a card
    /// older than the window is unanchored.
    func testAnchorToolCardsPairsNewestBack() {
        let feed: [AgentFeedItem] = [
            .tool(id: 1, name: "mcp__exponential__exponential_session_self_ask", detail: nil, subagentId: nil, settled: true),
            .tool(id: 2, name: "Edit", detail: "a.ts", subagentId: nil, settled: true),
            .tool(id: 3, name: "exponential_session_self_plan", detail: nil, subagentId: nil, settled: true, failed: true),
            .tool(id: 4, name: "exponential_session_self_plan", detail: nil, subagentId: nil, settled: true),
            .tool(id: 5, name: "exponential_session_self_ask", detail: nil, subagentId: nil, settled: false),
        ]
        let paired = AskDock.anchorToolCards(feed: feed, cardIds: ["m-old", "m-1", "m-4"])
        XCTAssertEqual(paired.anchors, [1: ["m-1"], 4: ["m-4"]])
        XCTAssertEqual(paired.unanchored, ["m-old"])
        let none = AskDock.anchorToolCards(feed: [], cardIds: ["a", "b"])
        XCTAssertEqual(none.anchors, [:])
        XCTAssertEqual(none.unanchored, ["a", "b"])
        XCTAssertEqual(AskDock.cardTools, ["session_self_plan", "session_self_ask"])
    }
}
