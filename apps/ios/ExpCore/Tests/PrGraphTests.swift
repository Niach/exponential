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
        status: String = "in_review",
        createdAt: String = "2026-09-16T09:00:00Z"
    ) -> IssueEntity {
        IssueEntity(
            id: id, boardId: "b1", number: 1, identifier: identifier, title: "T \(identifier)",
            description: nil, status: status, priority: "none", assigneeId: nil,
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

    // EXP-930 — mirrors web `lists the batch's issues on a batch run's run
    // face`. The pill on a batch run says `2 issues`; behind it the run face
    // lists those two. A batch row links NO issue (`issue_id` is null), so
    // keying the overlay off `issueId` was what left it with nothing to show:
    // the covered set comes off `batch_issue_ids`.
    func testListsTheBatchsIssuesOnABatchRunsRunFace() {
        let one = issue("one", identifier: "BATA")
        let two = issue("two", identifier: "BATB")
        let run = batchRun(batchIssueIds: #"["one","two"]"#, branch: "exp/batch-1")
        let stored = runGraph(run, [one, two])
        XCTAssertEqual(stored.batch?.issues.map(\.identifier), ["BATA", "BATB"])
        // ...and the run tree is still there, under the issues it covers.
        XCTAssertEqual(stored.tree.map(\.session.id), ["run"])

        // EXP-972: no stored ids names nothing — the branch-mates are NOT a
        // fallback any more (the server backfills `batch_issue_ids`).
        let older = issue("older", identifier: "BATA", branch: "exp/batch-1")
        let newer = issue(
            "newer", identifier: "BATB", branch: "exp/batch-1",
            createdAt: "2026-09-16T10:00:00Z"
        )
        let byBranch = runGraph(batchRun(branch: "exp/batch-1"), [newer, older])
        XCTAssertNil(byBranch.batch)
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

    // EXP-1058: the stacked chip that replaced the pill — the subject PR's
    // representative in front, every other issue of the stack/batch behind.
    func testNamesTheRepresentativeIssueAndTheCountOnTheStackedChip() {
        let url = "pr/9"
        let lower = issue(
            "lower", identifier: "EXP-1", prUrl: "pr/1", branch: "exp/LOWER", base: "main"
        )
        let one = issue(
            "one", identifier: "EXP-2", prUrl: url, branch: "exp/batch-abcd1234",
            base: "exp/LOWER", createdAt: "2026-09-16T10:00:00Z"
        )
        let two = issue(
            "two", identifier: "EXP-3", prUrl: url, branch: "exp/batch-abcd1234",
            base: "exp/LOWER"
        )
        // A batch inside a stack: the subject PR's representative, every other
        // issue on the stack behind it.
        let both = graph(two, [lower, one, two])
        XCTAssertEqual(PrGraph.badgeChip(both)?.issue?.id, "one")
        XCTAssertEqual(PrGraph.badgeChip(both)?.count, 2)
        // A plain batch: the others of the batch.
        let plainTwo = issue(
            "two", identifier: "EXP-3", prUrl: url, branch: "exp/batch-abcd1234", base: nil
        )
        let oneAlone = issue(
            "one", identifier: "EXP-2", prUrl: url, branch: "exp/batch-abcd1234",
            base: nil, createdAt: "2026-09-16T10:00:00Z"
        )
        let batch = graph(oneAlone, [oneAlone, plainTwo])
        XCTAssertEqual(PrGraph.badgeChip(batch)?.count, 1)
        // A run family with no issue earns no chip (SLOP-16 r5).
        let root = treeRun("root", parent: nil)
        let child = treeRun("child", parent: "root")
        let family = PrGraph.build(
            issue: nil, session: child, issues: [], sessions: [child, root], relations: []
        )
        XCTAssertNil(PrGraph.badgeChip(family))
        // No badge = no chip.
        let lone = issue("lone", identifier: "EXP-9")
        XCTAssertNil(PrGraph.badgeChip(graph(lone, [lone])))
    }

    // SLOP-16 r5: a run family alone earns NO badge on any face; a PR
    // relation still does, whatever runs ride with it.
    func testShapesNoBadgeForARunFamilyAlone() {
        let root = treeRun("root", parent: nil)
        let child = treeRun("child", parent: "root")
        let family = PrGraph.build(
            issue: nil, session: child, issues: [], sessions: [child, root], relations: []
        )
        XCTAssertNil(PrGraph.badgeShape(family))
        let alone = treeRun("alone", parent: nil)
        let lonely = PrGraph.build(
            issue: nil, session: alone, issues: [], sessions: [alone], relations: []
        )
        XCTAssertNil(PrGraph.badgeShape(lonely))
        let batchA = issue("bata", identifier: "EXP-1", prUrl: "pr/9")
        let batchB = issue("batb", identifier: "EXP-2", prUrl: "pr/9")
        let batched = PrGraph.build(
            issue: batchA, session: child, issues: [batchA, batchB],
            sessions: [child, root], relations: []
        )
        XCTAssertEqual(PrGraph.badgeShape(batched), .batch)
    }

    // EXP-1097: open blockers alone earn the chip, behind the PR relations and
    // the run family; the front chip is the first open blocker.
    func testShapesABlockedBadgeForAnIssueWithOpenBlockers() {
        let me = issue("me", identifier: "EXP-10")
        let b1 = issue("b1", identifier: "EXP-1")
        let b2 = issue("b2", identifier: "EXP-2")
        let closed = issue("closed", identifier: "EXP-3", status: "done")
        let relations = [
            relation("r1", from: "b2", to: "me"),
            relation("r2", from: "b1", to: "me"),
            relation("r3", from: "closed", to: "me"),
        ]
        let blocked = PrGraph.build(
            issue: me, session: nil, issues: [me, b1, b2, closed], sessions: [],
            relations: relations
        )
        XCTAssertEqual(PrGraph.badgeShape(blocked), .blocked)
        XCTAssertEqual(PrGraph.badgeChip(blocked)?.issue?.id, "b1")
        XCTAssertEqual(PrGraph.badgeChip(blocked)?.count, 1)
        // Only closed blockers: no chip.
        let done = PrGraph.build(
            issue: me, session: nil, issues: [me, closed], sessions: [], relations: relations
        )
        XCTAssertNil(PrGraph.badgeShape(done))
        // A run family does not change it: still blocked.
        let r1 = treeRun("r1", parent: nil, issueId: "me")
        let r2 = treeRun("r2", parent: "r1")
        XCTAssertEqual(
            PrGraph.badgeShape(PrGraph.build(
                issue: me, session: nil, issues: [me, b1], sessions: [r1, r2],
                relations: relations
            )),
            .blocked
        )
        // A batch wins over both.
        let batchA = issue("me", identifier: "EXP-10", prUrl: "pr/9")
        let batchB = issue("batb", identifier: "EXP-11", prUrl: "pr/9")
        XCTAssertEqual(
            PrGraph.badgeShape(PrGraph.build(
                issue: batchA, session: nil, issues: [batchA, batchB, b1],
                sessions: [r1, r2], relations: relations
            )),
            .batch
        )
    }

    // SLOP-16 r5: ONE band order on every face — Blocked by, Same pull
    // request, Pull request stack — each only when the subject has it; runs
    // are never a band.
    func testOrdersTheOverlaysSectionsTheSameEverywhere() {
        let me = issue("me", identifier: "EXP-10", prUrl: "pr/9")
        let partner = issue("partner", identifier: "EXP-11", prUrl: "pr/9")
        let blocker = issue("blocker", identifier: "EXP-1")
        let r1 = treeRun("r1", parent: nil, issueId: "me")
        let r2 = treeRun("r2", parent: "r1")
        let result = PrGraph.build(
            issue: me, session: nil, issues: [me, partner, blocker], sessions: [r1, r2],
            relations: [relation("rel", from: "blocker", to: "me")]
        )
        XCTAssertEqual(PrGraph.overlaySections(result), [.blocked, .batch])

        let lower = issue(
            "lower", identifier: "EXP-1", prUrl: "pr/1", branch: "exp/LOWER", base: "main"
        )
        let upper = issue(
            "upper", identifier: "EXP-2", prUrl: "pr/2", branch: "exp/UPPER", base: "exp/LOWER"
        )
        XCTAssertEqual(PrGraph.overlaySections(graph(upper, [lower, upper])), [.stack])

        let lone = issue("lone", identifier: "EXP-9")
        XCTAssertEqual(PrGraph.overlaySections(graph(lone, [lone])), [])
        // A lone PR is no band either.
        let lonePr = issue("pr", identifier: "EXP-8", prUrl: "pr/8", branch: "exp/EXP-8")
        XCTAssertEqual(PrGraph.overlaySections(graph(lonePr, [lonePr])), [])
    }

    /// SLOP-16 r5: the "Related work" copy, byte-identical ×4.
    func testPinsTheRelatedWorkCopy() {
        XCTAssertEqual(PrGraph.OverlayCopy.relatedWorkTitle, "Related work")
        XCTAssertEqual(PrGraph.OverlayCopy.blocked, "Blocked by")
        XCTAssertEqual(PrGraph.OverlayCopy.batch, "Same pull request")
        XCTAssertEqual(PrGraph.OverlayCopy.stack, "Pull request stack")
        XCTAssertEqual(PrGraph.OverlayCopy.empty, "Nothing else is linked to this issue.")
        XCTAssertEqual(PrGraph.overlayBandTitle(.blocked), "Blocked by")
        XCTAssertEqual(PrGraph.overlayBandTitle(.batch), "Same pull request")
        XCTAssertEqual(PrGraph.overlayBandTitle(.stack), "Pull request stack")
    }

    private func relation(_ id: String, from: String, to: String) -> IssueRelationEntity {
        IssueRelationEntity(
            id: id, issueId: from, relatedIssueId: to, type: "blocks", source: "user",
            teamId: "t1", boardId: "b1",
            createdAt: "2026-09-16T09:00:00Z", updatedAt: "2026-09-16T09:00:00Z"
        )
    }

    private func treeRun(
        _ id: String, parent: String?, issueId: String? = nil
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: issueId,
            teamId: "team-1",
            userId: "me",
            deviceLabel: "macbook",
            status: "running",
            parentSessionId: parent,
            startedAt: "2026-09-16T09:00:00Z",
            endedAt: nil,
            createdAt: "2026-09-16T09:00:00Z",
            updatedAt: "2026-09-16T09:00:00Z"
        )
    }
}
