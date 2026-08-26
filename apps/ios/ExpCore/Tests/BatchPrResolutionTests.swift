import Foundation
import XCTest
@testable import ExpCore

// EXP-535: a batch session carries no issue linkage, so its Merge button
// rides a client-side resolution — the team's open batch PRs, collapsed by
// prUrl with the NEWEST linked issue as the representative. EXP-545: the
// session resolves ITS OWN PR by the branch the pr_open flip stamped on the
// row. EXP-546: that branch is the only key — a branchless row resolves nil
// rather than guessing. Anything ambiguous or unmatched must resolve to nil:
// Reviews still lists every PR there.
final class BatchPrResolutionTests: XCTestCase {
    private func issue(
        id: String,
        boardId: String = "b-1",
        prUrl: String?,
        branch: String? = "exp/batch-a1b2c3d4",
        prState: String? = DomainContract.prStateOpen,
        createdAt: String = "2026-07-25T00:00:00Z"
    ) -> IssueEntity {
        IssueEntity(
            id: id,
            boardId: boardId,
            number: nil,
            identifier: nil,
            title: "t",
            description: nil,
            status: "in_review",
            priority: "none",
            assigneeId: nil,
            creatorId: nil,
            source: nil,
            dueDate: nil,
            sortOrder: nil,
            completedAt: nil,
            duplicateOfId: nil,
            prUrl: prUrl,
            prNumber: nil,
            prState: prState,
            branch: branch,
            prMergedAt: nil,
            createdAt: createdAt,
            updatedAt: createdAt
        )
    }

    func testResolvesTheSessionsOwnBatchPrToItsNewestLinkedIssue() {
        let batchUrl = "https://github.com/acme/web/pull/7"
        let open = BatchPrResolution.openBatchPrs(
            issues: [
                issue(id: "i-1", prUrl: batchUrl, createdAt: "2026-07-25T00:00:00Z"),
                issue(id: "i-3", prUrl: batchUrl, createdAt: "2026-07-25T02:00:00Z"),
                issue(id: "i-2", prUrl: batchUrl, createdAt: "2026-07-25T01:00:00Z"),
            ],
            teamBoardIds: ["b-1"]
        )

        let resolved = BatchPrResolution.resolve(
            sessionBranch: "exp/batch-a1b2c3d4", openBatchPrs: open
        )
        XCTAssertEqual(resolved?.id, "i-3")
        // EXP-546: a branchless row resolves nothing, even when there is
        // exactly one open batch PR to guess at.
        XCTAssertNil(
            BatchPrResolution.resolve(sessionBranch: nil, openBatchPrs: open)
        )
    }

    func testSessionBranchPicksItsOwnPrAmongConcurrentBatchRuns() {
        // EXP-545: with the stamped branch a session resolves its own PR even
        // while a second batch PR is open; a branchless row resolves nil.
        let open = BatchPrResolution.openBatchPrs(
            issues: [
                issue(id: "i-1", prUrl: "https://github.com/acme/web/pull/7"),
                issue(
                    id: "i-2",
                    prUrl: "https://github.com/acme/web/pull/9",
                    branch: "exp/batch-ef567890"
                ),
            ],
            teamBoardIds: ["b-1"]
        )

        let resolved = BatchPrResolution.resolve(
            sessionBranch: "exp/batch-ef567890", openBatchPrs: open
        )
        XCTAssertEqual(resolved?.id, "i-2")
        XCTAssertNil(
            BatchPrResolution.resolve(sessionBranch: nil, openBatchPrs: open)
        )
    }

    func testSessionWhoseOwnPrClosedNeverOffersATeammatesPr() {
        // EXP-545 regression: my batch PR closed unmerged (my session stays
        // in_review — only merge ends it) while a teammate's batch PR is the
        // sole open one. My stamped branch matches nothing open → no Merge.
        let open = BatchPrResolution.openBatchPrs(
            issues: [
                issue(
                    id: "mine",
                    prUrl: "https://github.com/acme/web/pull/7",
                    branch: "exp/batch-a1b2c3d4",
                    prState: "closed"
                ),
                issue(
                    id: "theirs",
                    prUrl: "https://github.com/acme/web/pull/9",
                    branch: "exp/batch-ef567890"
                ),
            ],
            teamBoardIds: ["b-1"]
        )

        XCTAssertNil(
            BatchPrResolution.resolve(
                sessionBranch: "exp/batch-a1b2c3d4", openBatchPrs: open
            )
        )
    }

    // Single-issue `exp/<IDENTIFIER>` branches, non-open PR states,
    // out-of-team boards, and rows without a prUrl never count as the batch
    // PR — nor may they trip the exactly-one branch guard on a real one.
    func testIgnoresNonBatchNonOpenOutOfTeamAndUrlLessRows() {
        let batchUrl = "https://github.com/acme/web/pull/7"
        let open = BatchPrResolution.openBatchPrs(
            issues: [
                issue(id: "i-1", prUrl: batchUrl),
                issue(
                    id: "single",
                    prUrl: "https://github.com/acme/web/pull/2",
                    branch: "exp/EXP-12"
                ),
                issue(
                    id: "merged",
                    prUrl: "https://github.com/acme/web/pull/3",
                    branch: "exp/batch-deadbee1",
                    prState: DomainContract.prStateMerged
                ),
                issue(
                    id: "other-team",
                    boardId: "b-9",
                    prUrl: "https://github.com/acme/other/pull/4",
                    branch: "exp/batch-deadbee2"
                ),
                issue(id: "no-url", prUrl: nil, branch: "exp/batch-deadbee3"),
            ],
            teamBoardIds: ["b-1"]
        )

        let resolved = BatchPrResolution.resolve(
            sessionBranch: "exp/batch-a1b2c3d4", openBatchPrs: open
        )
        XCTAssertEqual(resolved?.id, "i-1")
        XCTAssertNil(
            BatchPrResolution.resolve(sessionBranch: nil, openBatchPrs: open)
        )
    }

    func testNoOpenBatchPrResolvesToNil() {
        let open = BatchPrResolution.openBatchPrs(
            issues: [
                issue(
                    id: "single",
                    prUrl: "https://github.com/acme/web/pull/2",
                    branch: "exp/EXP-12"
                ),
            ],
            teamBoardIds: ["b-1"]
        )

        XCTAssertNil(
            BatchPrResolution.resolve(
                sessionBranch: "exp/batch-a1b2c3d4", openBatchPrs: open
            )
        )
        XCTAssertNil(
            BatchPrResolution.resolve(sessionBranch: nil, openBatchPrs: open)
        )
    }
}
