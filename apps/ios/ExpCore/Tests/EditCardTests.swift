import Foundation
import XCTest
@testable import ExpCore

/// EXP-916: the fixture is the contract. `fixtures/feed/edit-cards.json` is
/// replayed through the REAL row projection in `AgentFeedTests`
/// (`everyFixtureCaseProjectsByteExact`); this file holds the fixture reader
/// both suites share, plus the card rules the TS suite spells out by hand,
/// under THOSE test names.
///
/// A case: `feed` (items with `id`, `kind`, and for tools `toolKind`, `detail`,
/// `settled`, `failed`, `diff`, `workflowId`, `subagentId`), an optional
/// `start` (the render window's first index, EXP-783), an optional `lane`
/// (project THAT subagent's items instead of the main lane), an optional `live`
/// (the id `AgentFeed.liveToolRowId` would name) and `expected` = one string
/// per row.
enum EditCardFixture {
    struct Item: Decodable {
        let id: Int
        let kind: String
        let toolKind: String?
        let workflowId: String?
        let subagentId: String?
        let detail: String?
        let diff: String?
        let settled: Bool?
        let failed: Bool?
        let text: String?
    }

    struct Case: Decodable {
        let name: String
        let feed: [Item]
        let start: Int?
        let lane: String?
        let live: Int?
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
            .appendingPathComponent("packages/domain-contract/fixtures/feed/edit-cards.json")
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
                name: "Edit",
                detail: entry.detail,
                subagentId: entry.subagentId,
                callId: "tc-\(entry.id)",
                toolKind: entry.toolKind,
                settled: entry.settled ?? false,
                failed: entry.failed ?? false,
                diff: entry.diff
            )
        case "user_message":
            return .userMessage(
                id: entry.id, text: entry.text ?? "", subagentId: entry.subagentId
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

final class EditCardTests: XCTestCase {
    private func edit(_ id: Int, _ detail: String, settled: Bool = false) -> AgentFeedItem {
        .tool(
            id: id, name: "Edit", detail: detail, subagentId: nil,
            callId: "tc-\(id)", toolKind: "edit", settled: settled
        )
    }

    /// `the fixture covers a split, a live row, a stub, a lane and a window`
    func testTheFixtureCoversASplitALiveRowAStubALaneAndAWindow() throws {
        let names = try EditCardFixture.cases().map(\.name).joined(separator: "\n")
        XCTAssertTrue(names.contains("splits"), names)
        XCTAssertTrue(names.contains("live"), names)
        XCTAssertTrue(names.contains("failed"), names)
        XCTAssertTrue(names.contains("subagent"), names)
        XCTAssertTrue(names.contains("window"), names)
    }

    /// `the rule reads only kind, toolKind and workflowId`
    func testTheRuleReadsOnlyKindToolKindAndWorkflowId() {
        func tool(_ kind: String?, callId: String? = "tc-1") -> AgentFeedItem {
            .tool(
                id: 1, name: "Edit", detail: nil, subagentId: nil,
                callId: callId, toolKind: kind
            )
        }
        XCTAssertTrue(EditCard.isEditCall(tool("edit")))
        XCTAssertTrue(EditCard.isEditCall(tool("delete")))
        XCTAssertTrue(EditCard.isEditCall(tool("move")))
        XCTAssertFalse(EditCard.isEditCall(tool("read")))
        XCTAssertFalse(EditCard.isEditCall(tool(nil)))
        // A WORKFLOW call is its own row, never a card member.
        XCTAssertFalse(EditCard.isEditCall(tool("edit"), workflowIds: ["tc-1"]))
        XCTAssertFalse(EditCard.isEditCall(.narration(id: 1, text: "edit")))
    }

    /// `the copy is the contract's`
    func testTheCopyIsTheContracts() {
        XCTAssertEqual(EditCard.title(1), "1 file edited")
        XCTAssertEqual(EditCard.title(4), "4 files edited")
        XCTAssertEqual(EditCard.preview, 5)
        XCTAssertNil(EditCard.moreLabel(5))
        XCTAssertEqual(EditCard.moreLabel(8), "3 more")
    }

    /// `a live row is only the card's LAST member`
    func testALiveRowIsOnlyTheCardsLastMember() {
        let items = [edit(1, "a.ts"), edit(2, "b.ts")]
        XCTAssertNil(EditCard.card(items, liveItemId: 1).liveIndex)
        XCTAssertEqual(EditCard.card(items, liveItemId: 2).liveIndex, 1)
        XCTAssertNil(EditCard.card(items, liveItemId: nil).liveIndex)
    }

    /// A run ends at anything else — the same scan the projection runs.
    func testTheRunEndsAtTheFirstNonEditOfTheLane() {
        let feed: [AgentFeedItem] = [
            edit(1, "a.ts"),
            edit(2, "b.ts"),
            .tool(
                id: 3, name: "Bash", detail: "bun test", subagentId: nil,
                callId: "tc-3", toolKind: "execute"
            ),
            edit(4, "c.ts"),
        ]
        XCTAssertEqual(EditCard.runEnd(feed, start: 0), 1)
        XCTAssertEqual(EditCard.runEnd(feed, start: 3), 3)
        // Another lane's edit never joins this one.
        let lanes: [AgentFeedItem] = [
            edit(1, "a.ts"),
            .tool(
                id: 2, name: "Edit", detail: "b.ts", subagentId: "sub-1",
                callId: "tc-2", toolKind: "edit"
            ),
        ]
        XCTAssertEqual(EditCard.runEnd(lanes, start: 0), 0)
    }
}
