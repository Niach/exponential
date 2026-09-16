import Foundation
import XCTest
@testable import ExpCore

// EXP-897 Part 4 — the ONE graph behind the Work header's badge and overlay.
// The same four tests web (`pr-graph.test.ts`), Android (`PrGraphTest`) and
// the desktop (`pr_graph::*`) run.
final class PrGraphTests: XCTestCase {
    private func issue(
        _ id: String,
        identifier: String,
        prUrl: String? = nil,
        branch: String? = nil,
        base: String? = nil,
        createdAt: String = "2026-09-16T09:00:00Z"
    ) -> IssueEntity {
        IssueEntity(
            id: id, boardId: "b1", number: 1, identifier: identifier, title: "T \(identifier)",
            description: nil, status: "in_review", priority: "none", assigneeId: nil,
            creatorId: nil, source: nil, dueDate: nil, sortOrder: 1, completedAt: nil,
            duplicateOfId: nil, prUrl: prUrl, prNumber: 1, prState: "open", branch: branch,
            prBaseBranch: base, prMergedAt: nil, createdAt: createdAt, updatedAt: createdAt
        )
    }

    private func graph(_ subject: IssueEntity, _ issues: [IssueEntity]) -> PrGraph.Graph {
        PrGraph.build(
            issue: subject, session: nil, issues: issues, sessions: [], relations: []
        )
    }

    /// EXP-876: a BATCH run as its own subject — no issue, no `pr_url`.
    private func batchRun(
        _ id: String = "run",
        batchIssueIds: String? = nil,
        branch: String? = nil,
        actionName: String? = nil
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: nil,
            teamId: "team-1",
            userId: "me",
            deviceLabel: "macbook",
            status: "running",
            branch: branch,
            batchIssueIds: batchIssueIds,
            actionName: actionName,
            startedAt: "2026-09-16T09:00:00Z",
            endedAt: nil,
            createdAt: "2026-09-16T09:00:00Z",
            updatedAt: "2026-09-16T09:00:00Z"
        )
    }

    private func runGraph(
        _ session: CodingSessionEntity,
        _ issues: [IssueEntity]
    ) -> PrGraph.Graph {
        PrGraph.build(
            issue: nil, session: session, issues: issues, sessions: [session], relations: []
        )
    }

    // One issue, one PR, stacked on another issue's branch: a STACK badge and
    // a position read from the bottom.
    func testReportsAStackBadgeForAStackedPr() {
        let low = issue(
            "low", identifier: "EXP-1", prUrl: "pr/1", branch: "exp/EXP-1", base: "main"
        )
        let high = issue(
            "high", identifier: "EXP-2", prUrl: "pr/2", branch: "exp/EXP-2", base: "exp/EXP-1"
        )
        let result = graph(high, [low, high])
        XCTAssertEqual(PrGraph.badgeKind(result), .stack)
        XCTAssertEqual(result.positionLabel, "2 of 2")
        XCTAssertEqual(result.below?.representative.id, "low")
        XCTAssertNil(result.above)
        XCTAssertEqual(result.bottom?.representative.id, "low")
        XCTAssertNil(result.batch)
    }

    // Two issues on ONE pull request (a batch run's combined PR): a BATCH
    // badge, no stack, and the entry lists both issues.
    func testReportsABatchBadgeForABatchPr() {
        let first = issue(
            "a", identifier: "EXP-1", prUrl: "pr/9", branch: "exp/batch-1",
            createdAt: "2026-09-16T09:00:00Z"
        )
        let second = issue(
            "b", identifier: "EXP-2", prUrl: "pr/9", branch: "exp/batch-1",
            createdAt: "2026-09-16T10:00:00Z"
        )
        let result = graph(first, [first, second])
        XCTAssertEqual(PrGraph.badgeKind(result), .batch)
        XCTAssertNil(result.positionLabel)
        XCTAssertEqual(result.batch?.issues.map(\.id), ["b", "a"])
        XCTAssertEqual(result.entry?.isBatch, true)
    }

    // A batch PR is an ENTRY like any other, so it can be a stack member —
    // then the badge carries both glyphs.
    func testReportsBothForABatchInsideAStack() {
        let low = issue(
            "low", identifier: "EXP-1", prUrl: "pr/1", branch: "exp/EXP-1", base: "main"
        )
        let batchA = issue(
            "ba", identifier: "EXP-2", prUrl: "pr/9", branch: "exp/batch-1", base: "exp/EXP-1",
            createdAt: "2026-09-16T09:00:00Z"
        )
        let batchB = issue(
            "bb", identifier: "EXP-3", prUrl: "pr/9", branch: "exp/batch-1", base: "exp/EXP-1",
            createdAt: "2026-09-16T10:00:00Z"
        )
        let result = graph(batchA, [low, batchA, batchB])
        XCTAssertEqual(PrGraph.badgeKind(result), .stackAndBatch)
        XCTAssertEqual(result.positionLabel, "2 of 2")
        XCTAssertEqual(result.batch?.issues.count, 2)
        XCTAssertEqual(result.stack.map(\.depth), [0, 1])
        XCTAssertEqual(result.stack.first?.entry.representative.id, "low")
    }

    // A pull request of its own, based on the default branch: no badge, and
    // nothing for the overlay to say.
    // EXP-876: the pill and its sheet are the surface built to name work that
    // spans several issues — and a batch RUN, which spans them, resolved
    // nothing at all before this (it links no issue and stamps no pr_url).
    // Mirrored ×4.
    func testReportsABatchBadgeForABatchRunBeforeItsPr() {
        let one = issue("one", identifier: "EXP-874")
        let two = issue("two", identifier: "EXP-876")
        let result = runGraph(batchRun(batchIssueIds: #"["one","two"]"#), [one, two])
        XCTAssertEqual(PrGraph.badgeKind(result), .batch)
        // The composer's order, so the sheet reads like the row that named it.
        XCTAssertEqual(result.batch?.issues.map(\.id), ["one", "two"])
        // No pull request yet: a batch of two is not a stack of two.
        XCTAssertTrue(result.stack.isEmpty)
    }

    func testReportsABatchRunsPullRequestOnceItOpens() {
        // The GROUPED entry wins the moment the PR exists: it carries the
        // branch and the base the stack chains on, so a stacked batch still
        // reads `stack+batch` from its run.
        let lower = issue(
            "lower", identifier: "EXP-11", prUrl: "pr/1", branch: "exp/EXP-11", base: "main"
        )
        let one = issue(
            "one", identifier: "EXP-874", prUrl: "pr/9",
            branch: "exp/batch-abcd1234", base: "exp/EXP-11"
        )
        let two = issue(
            "two", identifier: "EXP-876", prUrl: "pr/9",
            branch: "exp/batch-abcd1234", base: "exp/EXP-11"
        )
        let run = batchRun(batchIssueIds: #"["one","two"]"#, branch: "exp/batch-abcd1234")
        let result = runGraph(run, [lower, one, two])
        XCTAssertEqual(PrGraph.badgeKind(result), .stackAndBatch)
        XCTAssertEqual(result.batch?.issues.count, 2)
    }

    func testReportsNothingForABatchRunWhoseIssuesAreUnknown() {
        // No stored ids and no PR: the row reads "Batch run" and wears no
        // pill, rather than a pill that could say nothing.
        let bare = runGraph(batchRun(), [])
        XCTAssertNil(PrGraph.badgeKind(bare))
        XCTAssertNil(bare.batch)
        // An ACTION run is never a batch, whatever else it carries.
        let action = runGraph(
            batchRun("chat", batchIssueIds: #"["one"]"#, actionName: "Chat"),
            [issue("one", identifier: "EXP-874")]
        )
        XCTAssertNil(action.batch)
    }

    func testReportsNothingForALonePr() {
        let lone = issue(
            "lone", identifier: "EXP-1", prUrl: "pr/1", branch: "exp/EXP-1", base: "main"
        )
        let stranger = issue(
            "other", identifier: "EXP-2", prUrl: "pr/2", branch: "exp/EXP-2", base: "main"
        )
        let result = graph(lone, [lone, stranger])
        XCTAssertNil(PrGraph.badgeKind(result))
        XCTAssertTrue(result.stack.isEmpty)
        XCTAssertNil(result.batch)
        XCTAssertNil(result.positionLabel)
        XCTAssertTrue(result.isEmpty)
    }
}
