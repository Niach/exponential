import Foundation
import XCTest
@testable import ExpCore

/// EXP-948: the fixture is the contract. `fixtures/feed/exp-tool-groups.json`
/// is replayed here through the REAL row projection (`AgentFeed.rows` for the
/// main lane, `AgentFeed.laneRows` for a `lane` case), exactly the way the TS,
/// desktop and Android suites replay it through theirs.
///
/// A case: `feed` (items with `id`, `kind`, and for tools `name`, `toolKind`,
/// `detail`, `settled`, `failed`, `workflowId`, `subagentId`), an optional
/// `start` (the render window's first index, EXP-783), an optional `lane`
/// (project THAT subagent's items instead of the main lane) and `expected` =
/// one string per row.
enum ExpToolGroupFixture {
    struct Item: Decodable {
        let id: Int
        let kind: String
        let name: String?
        let toolKind: String?
        let workflowId: String?
        let subagentId: String?
        let detail: String?
        let settled: Bool?
        let failed: Bool?
        let text: String?
    }

    struct Case: Decodable {
        let name: String
        let feed: [Item]
        let start: Int?
        let lane: String?
        let expected: [String]
    }

    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    static func cases() throws -> [Case] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent(
                "packages/domain-contract/fixtures/feed/exp-tool-groups.json"
            )
        return try JSONDecoder().decode([Case].self, from: Data(contentsOf: url))
    }

    /// One fixture item → one feed item. A tool always carries a call id, so a
    /// fixture `workflowId` can name it the way the iOS projection does (a
    /// workflow call is a member of `workflowIds`, not a field on the row).
    static func item(_ entry: Item) -> AgentFeedItem {
        switch entry.kind {
        case "tool":
            return .tool(
                id: entry.id,
                name: entry.name ?? "",
                detail: entry.detail,
                subagentId: entry.subagentId,
                callId: "tc-\(entry.id)",
                toolKind: entry.toolKind,
                settled: entry.settled ?? false,
                failed: entry.failed ?? false
            )
        case "user_message":
            return .userMessage(
                id: entry.id, text: entry.text ?? "", subagentId: entry.subagentId
            )
        case "subagent":
            return .subagent(
                id: entry.id,
                subagentId: entry.subagentId ?? "",
                agentType: AgentFeed.subagentFallbackType,
                status: .started,
                detail: nil
            )
        default:
            return .narration(
                id: entry.id, text: entry.text ?? "", subagentId: entry.subagentId
            )
        }
    }

    static func feed(_ entry: Case) -> [AgentFeedItem] {
        entry.feed.map(item)
    }

    /// The calls the projection must treat as WORKFLOW cards (EXP-850 §3).
    static func workflowIds(_ entry: Case) -> Set<String> {
        Set(entry.feed.filter { $0.workflowId != nil }.map { "tc-\($0.id)" })
    }
}

final class ExpToolGroupTests: XCTestCase {
    private func call(
        _ id: Int, _ name: String = "exponential_issues_get",
        settled: Bool = true, failed: Bool = false, subagentId: String? = nil
    ) -> AgentFeedItem {
        .tool(
            id: id, name: name, detail: nil, subagentId: subagentId,
            callId: "tc-\(id)", toolKind: "other", settled: settled, failed: failed
        )
    }

    /// `every fixture case projects byte-exact` — the contract's own fixture
    /// replayed through the REAL projection. The row strings are the fixture's
    /// vocabulary: `expRun@id[ids]: <caption>` · `run@id[ids]` · `tool@id` ·
    /// `narration@id` · `user@id` · `subagent@id(lane)`.
    func testEveryFixtureCaseProjectsByteExact() throws {
        for entry in try ExpToolGroupFixture.cases() {
            let feed = ExpToolGroupFixture.feed(entry)
            let workflowIds = ExpToolGroupFixture.workflowIds(entry)
            let rows: [AgentFeedRow]
            if let lane = entry.lane {
                rows = AgentFeed.laneRows(
                    feed.filter { $0.subagentKey == lane }, workflowIds: workflowIds
                )
            } else {
                rows = AgentFeed.rows(
                    feed, from: entry.start ?? 0, workflowIds: workflowIds
                )
            }
            XCTAssertEqual(rows.map(project), entry.expected, entry.name)
        }
    }

    /// One projected row in the fixture's own notation.
    private func project(_ row: AgentFeedRow) -> String {
        switch row {
        case let .expRun(items):
            let ids = items.map { String($0.id) }.joined(separator: ",")
            return "expRun@\(row.id)[\(ids)]: \(ExpToolGroup.caption(items))"
        case let .toolRun(items):
            let ids = items.map { String($0.id) }.joined(separator: ",")
            return "run@\(row.id)[\(ids)]"
        case let .edits(items):
            let ids = items.map { String($0.id) }.joined(separator: ",")
            return "card@\(row.id)[\(ids)]"
        case let .subagentRun(run):
            return "subagent@\(run.id)(\(run.subagentId))"
        case let .ask(group):
            return "ask@\(group.id)"
        case let .single(item):
            switch item {
            case .tool: return "tool@\(item.id)"
            case .userMessage: return "user@\(item.id)"
            case .narration: return "narration@\(item.id)"
            case .subagent: return "subagent@\(item.id)"
            default: return "other@\(item.id)"
            }
        }
    }

    /// `the fixture covers a lone call, a break, a lane and a window`
    func testTheFixtureCoversALoneCallABreakALaneAndAWindow() throws {
        let names = try ExpToolGroupFixture.cases().map(\.name).joined(separator: "\n")
        XCTAssertTrue(names.contains("single"), names)
        XCTAssertTrue(names.contains("failed"), names)
        XCTAssertTrue(names.contains("command run"), names)
        XCTAssertTrue(names.contains("subagent"), names)
        XCTAssertTrue(names.contains("window"), names)
    }

    /// `the rule reads only kind, name and workflowId` — never `settled` or
    /// `failed`, so a group can never re-split as its calls land.
    func testTheRuleReadsOnlyKindNameAndWorkflowId() {
        XCTAssertTrue(ExpToolGroup.isExpToolCall(call(1)))
        XCTAssertTrue(
            ExpToolGroup.isExpToolCall(call(1, "mcp__exponential__exponential_pr_open"))
        )
        XCTAssertTrue(ExpToolGroup.isExpToolCall(call(1, settled: false, failed: true)))
        // Another server's same-named tool is not ours.
        XCTAssertFalse(ExpToolGroup.isExpToolCall(call(1, "mcp__linear__issues_get")))
        XCTAssertFalse(ExpToolGroup.isExpToolCall(call(1, "Bash")))
        // A WORKFLOW call is its own row, never a group member.
        XCTAssertFalse(ExpToolGroup.isExpToolCall(call(1), workflowIds: ["tc-1"]))
        XCTAssertFalse(ExpToolGroup.isExpToolCall(.narration(id: 1, text: "issues_get")))
    }

    /// A run ends at the first item that is not the SAME tool in the SAME lane.
    func testTheRunEndsAtTheFirstOtherToolOrLane() {
        let feed: [AgentFeedItem] = [
            call(1), call(2),
            call(3, "exponential_issues_list"),
            call(4, subagentId: "s1"),
        ]
        XCTAssertEqual(ExpToolGroup.runEnd(feed, start: 0), 1)
        XCTAssertEqual(ExpToolGroup.runEnd(feed, start: 2), 2)
        // A run never opens on someone else's tool.
        XCTAssertEqual(
            ExpToolGroup.runEnd([call(1, "Bash"), call(2, "Bash")], start: 0), 0
        )
    }

    /// `the caption is the contract's` — plural copy, `{n}` filled in, and the
    /// ` · N failed` tail `ToolGroupSummary` writes.
    func testTheCaptionIsTheContracts() {
        XCTAssertEqual(ExpToolGroup.caption([call(1), call(2)]), "Read 2 issues")
        XCTAssertEqual(
            ExpToolGroup.caption([call(1), call(2, settled: false)]), "Reading 2 issues"
        )
        XCTAssertEqual(
            ExpToolGroup.caption([call(1), call(2, failed: true)]),
            "Read 2 issues\(ToolGroupSummary.separator)1 failed"
        )
        XCTAssertEqual(ExpToolGroup.caption([call(1, "Bash")]), "")
        XCTAssertEqual(ExpToolGroup.caption([]), "")
    }

    /// EXP-948: one of ours BREAKS a command run and never joins it, and a lone
    /// call stays the single row it always was.
    func testAnExponentialCallBreaksAToolRunAndNeverJoinsIt() {
        let bash = { (id: Int) in self.call(id, "Bash") }
        let feed: [AgentFeedItem] = [bash(1), bash(2), call(3), bash(4)]
        XCTAssertEqual(
            AgentFeed.rows(feed),
            [.toolRun([feed[0], feed[1]]), .single(feed[2]), .single(feed[3])]
        )
        XCTAssertEqual(AgentFeedRow.expRun([call(3), call(4)]).rowClass, .tool)
        XCTAssertEqual(AgentFeedRow.expRun([call(3), call(4)]).id, 3)
    }
}
