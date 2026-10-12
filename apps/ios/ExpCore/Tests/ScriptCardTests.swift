import Foundation
import XCTest
@testable import ExpCore

// EXP-1279 (EXP-1259 D13): the Script card's model, locked ×4 (TS
// `script-card.test.ts`, desktop `steer::script_card`, Android
// `ScriptCardTest`) against `packages/domain-contract/fixtures/script-card.json`
// — every case's WHOLE expected object — plus the wire's `tool_update.script`
// decode and the feed rule that a script call never joins a tool run.
final class ScriptCardTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/script-card.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    func testEveryFixtureCaseRendersTheWholeCard() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertGreaterThanOrEqual(cases.count, 6)
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let raw = try XCTUnwrap(c["input"] as? [String: Any])
            let calls = (raw["calls"] as? [[String: Any]]).map { rows in
                rows.map {
                    ScriptCall(
                        tool: $0["tool"] as? String ?? "",
                        subject: $0["subject"] as? String,
                        ok: $0["ok"] as? Bool ?? true,
                        ms: ($0["ms"] as? NSNumber)?.intValue
                    )
                }
            }
            let got = ScriptCards.card(ScriptCardInput(
                running: raw["running"] as? Bool ?? false,
                count: (raw["count"] as? NSNumber)?.intValue,
                calls: calls,
                elapsedMs: (raw["elapsedMs"] as? NSNumber)?.intValue
            ))
            let want = try XCTUnwrap(c["expected"] as? [String: Any])
            let groups = try (want["groups"] as? [[String: Any]] ?? []).map { group in
                ScriptCardGroup(
                    tool: try XCTUnwrap(group["tool"] as? String),
                    exponential: try XCTUnwrap(group["exponential"] as? Bool),
                    label: try XCTUnwrap(group["label"] as? String),
                    subject: group["subject"] as? String,
                    more: group["more"] as? String,
                    count: group["count"] as? String,
                    failed: group["failed"] as? String
                )
            }
            XCTAssertEqual(
                got,
                ScriptCard(
                    title: try XCTUnwrap(want["title"] as? String),
                    meta: want["meta"] as? String,
                    groups: groups,
                    overflow: want["overflow"] as? String
                ),
                name
            )
        }
    }

    func testTheDurationsReplayTheFixtureInIntegerTenths() throws {
        let durations = try XCTUnwrap(try fixture()["durations"] as? [[String: Any]])
        XCTAssertFalse(durations.isEmpty)
        for c in durations {
            let ms = try XCTUnwrap(c["ms"] as? Int)
            XCTAssertEqual(ScriptCards.duration(ms: ms), c["expected"] as? String, "\(ms) ms")
        }
    }

    func testTheConstantsAreTheFixtures() throws {
        let fixture = try fixture()
        XCTAssertEqual(fixture["logCap"] as? Int, ScriptCards.logCap)
        XCTAssertEqual(fixture["toolNameSuffix"] as? String, ScriptCards.toolNameSuffix)
        XCTAssertEqual(try XCTUnwrap(fixture["copy"] as? [String: String]), [
            "running": ScriptCards.Copy.running,
            "done": ScriptCards.Copy.done,
            "script": ScriptCards.Copy.script,
            "call": ScriptCards.Copy.call,
            "calls": ScriptCards.Copy.calls,
            "failed": ScriptCards.Copy.failed,
            "moreCall": ScriptCards.Copy.moreCall,
            "moreCalls": ScriptCards.Copy.moreCalls,
        ])
        XCTAssertEqual(try XCTUnwrap(fixture["testIds"] as? [String: String]), [
            "card": ScriptCards.TestIds.card,
            "group": ScriptCards.TestIds.group,
            "source": ScriptCards.TestIds.source,
        ])
        XCTAssertTrue(DomainContract.toolKindValues.contains(ScriptCards.toolKind))
    }

    /// Wire §1b: the kind, else the claude or codex name.
    func testAScriptCallIsTheKindOrTheExecName() {
        XCTAssertTrue(ScriptCards.isScriptCall(name: "anything", toolKind: "script"))
        XCTAssertTrue(ScriptCards.isScriptCall(name: "mcp__codemode__exec", toolKind: nil))
        XCTAssertTrue(ScriptCards.isScriptCall(name: "mcp.codemode.exec", toolKind: "other"))
        XCTAssertFalse(ScriptCards.isScriptCall(name: "Bash", toolKind: "execute"))
    }

    /// The settle's `script` decodes tolerantly: a malformed call is dropped,
    /// never the log; `ok` defaults true; nothing usable = nil.
    func testTheSettleDecodesTheLogTolerantly() throws {
        let update = try XCTUnwrap(AgentActivityDecoder.toolUpdate([
            "kind": "tool_update", "id": "tc-1", "status": "completed", "at": 1_760_000_000_000,
            "script": [
                "count": 3, "elapsedMs": 3040.4, "source": "await tools.exponential.x()",
                "calls": [
                    ["tool": " exponential.exponential_issues_get ", "subject": "EXP-1", "ok": true, "ms": 40],
                    ["tool": "", "ok": true],
                    "garbage",
                    ["tool": "computer.screenshot", "subject": "  "],
                ],
            ] as [String: Any],
        ]))
        XCTAssertEqual(update.at, 1_760_000_000_000)
        let log = try XCTUnwrap(update.script)
        XCTAssertEqual(log.count, 3)
        XCTAssertEqual(log.elapsedMs, 3040)
        XCTAssertEqual(log.source, "await tools.exponential.x()")
        XCTAssertEqual(log.calls, [
            ScriptCall(tool: "exponential.exponential_issues_get", subject: "EXP-1", ok: true, ms: 40),
            ScriptCall(tool: "computer.screenshot", subject: nil, ok: true, ms: nil),
        ])
        XCTAssertNil(AgentScriptLog.parse(["calls": []]))
        XCTAssertNil(AgentScriptLog.parse("nope"))
        XCTAssertEqual(AgentScriptLog.parse(["source": "x"])?.source, "x")
    }

    /// The fold keeps the log on the row and stamps the settle's time; a
    /// later update without one keeps it.
    func testTheFoldKeepsTheLogAndTheSettleTime() throws {
        let feed: [AgentFeedItem] = [
            .tool(id: 1, name: "mcp__codemode__exec", detail: nil, subagentId: nil, callId: "tc-1", toolKind: "script"),
        ]
        let log = AgentScriptLog(count: 1, calls: [ScriptCall(tool: "a.b", ok: true)])
        let settled = try XCTUnwrap(AgentFeed.applyToolUpdate(
            feed: feed, update: AgentToolUpdate(id: "tc-1", status: "completed", script: log, at: 42)
        ))
        let again = try XCTUnwrap(AgentFeed.applyToolUpdate(
            feed: settled, update: AgentToolUpdate(id: "tc-1", diff: "x")
        ))
        guard case let .tool(_, _, _, _, _, _, isSettled, _, _, _, _, script, settledAt) = again[0] else {
            return XCTFail("not a tool row")
        }
        XCTAssertTrue(isSettled)
        XCTAssertEqual(script, log)
        XCTAssertEqual(settledAt, 42)
    }

    /// A script call is its own row: it never joins (nor continues) a run of
    /// generic tool calls.
    func testAScriptCallNeverJoinsAToolRun() {
        let feed: [AgentFeedItem] = [
            .tool(id: 1, name: "Bash", detail: "ls", subagentId: nil, toolKind: "execute"),
            .tool(id: 2, name: "Bash", detail: "pwd", subagentId: nil, toolKind: "execute"),
            .tool(id: 3, name: "mcp__codemode__exec", detail: nil, subagentId: nil, toolKind: "script"),
            .tool(id: 4, name: "mcp__codemode__exec", detail: nil, subagentId: nil),
            .tool(id: 5, name: "Bash", detail: "ls", subagentId: nil, toolKind: "execute"),
        ]
        let rows = AgentFeed.rows(feed)
        XCTAssertEqual(rows.count, 4)
        guard case let .toolRun(items) = rows[0] else { return XCTFail("not a run") }
        XCTAssertEqual(items.map(\.id), [1, 2])
        XCTAssertEqual(rows[1], .single(feed[2]))
        XCTAssertEqual(rows[2], .single(feed[3]))
        XCTAssertEqual(rows[3], .single(feed[4]))
    }
}
