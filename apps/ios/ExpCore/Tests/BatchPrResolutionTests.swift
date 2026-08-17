import Foundation
import XCTest
@testable import ExpCore

// EXP-535: a batch session carries no issue/PR linkage, so its Merge button
// rides a client-side resolution — the team's sole open batch PR, collapsed by
// prUrl with the NEWEST linked issue as the representative. Anything ambiguous
// (two distinct open batch PRs) must resolve to nil: Reviews still lists every
// PR there.
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

    func testResolvesTheSoleOpenBatchPrToItsNewestLinkedIssue() {
        let batchUrl = "https://github.com/acme/web/pull/7"
        let resolved = BatchPrResolution.soleOpenBatchPr(
            issues: [
                issue(id: "i-1", prUrl: batchUrl, createdAt: "2026-07-25T00:00:00Z"),
                issue(id: "i-3", prUrl: batchUrl, createdAt: "2026-07-25T02:00:00Z"),
                issue(id: "i-2", prUrl: batchUrl, createdAt: "2026-07-25T01:00:00Z"),
            ],
            teamBoardIds: ["b-1"]
        )

        XCTAssertEqual(resolved?.id, "i-3")
    }

    func testTwoDistinctOpenBatchPrsAreAmbiguousAndResolveToNil() {
        let resolved = BatchPrResolution.soleOpenBatchPr(
            issues: [
                issue(id: "i-1", prUrl: "https://github.com/acme/web/pull/7"),
                issue(id: "i-2", prUrl: "https://github.com/acme/web/pull/9"),
            ],
            teamBoardIds: ["b-1"]
        )

        XCTAssertNil(resolved)
    }

    // Single-issue `exp/<IDENTIFIER>` branches, non-open PR states,
    // out-of-team boards, and rows without a prUrl never count as the batch
    // PR — nor may they trip the exactly-one guard on a real one.
    func testIgnoresNonBatchNonOpenOutOfTeamAndUrlLessRows() {
        let batchUrl = "https://github.com/acme/web/pull/7"
        let resolved = BatchPrResolution.soleOpenBatchPr(
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

        XCTAssertEqual(resolved?.id, "i-1")
    }

    func testNoOpenBatchPrResolvesToNil() {
        let resolved = BatchPrResolution.soleOpenBatchPr(
            issues: [
                issue(
                    id: "single",
                    prUrl: "https://github.com/acme/web/pull/2",
                    branch: "exp/EXP-12"
                ),
            ],
            teamBoardIds: ["b-1"]
        )

        XCTAssertNil(resolved)
    }
}
