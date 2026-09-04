import Foundation
import XCTest
@testable import ExpCore

// EXP-734: an action or chat run's pull request links no issue at all, so its
// Merge affordance targets the SESSION row the server stamped it on. Issue and
// batch runs keep merging through an issue.
final class MergeTargetResolutionTests: XCTestCase {
    private func session(
        id: String = "cs-1",
        issueId: String? = nil,
        actionName: String? = nil,
        status: String = DomainContract.codingSessionStatusRunning,
        branch: String? = nil,
        prUrl: String? = nil,
        prNumber: Int? = nil,
        prState: String? = nil
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: issueId,
            teamId: "ws-1",
            userId: "u-1",
            deviceLabel: "macbook",
            status: status,
            branch: branch,
            actionName: actionName,
            startedAt: "2026-09-04T09:00:00Z",
            endedAt: nil,
            createdAt: "2026-09-04T09:00:00Z",
            updatedAt: "2026-09-04T09:00:00Z",
            prUrl: prUrl,
            prNumber: prNumber,
            prState: prState
        )
    }

    private func issue(
        id: String = "i-1",
        prUrl: String? = "https://github.com/acme/web/pull/3",
        prState: String? = DomainContract.prStateOpen,
        branch: String? = "exp/EXP-1"
    ) -> IssueEntity {
        IssueEntity(
            id: id,
            boardId: "b-1",
            number: nil,
            identifier: "EXP-1",
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
            prNumber: 3,
            prState: prState,
            branch: branch,
            prMergedAt: nil,
            createdAt: "2026-09-04T09:00:00Z",
            updatedAt: "2026-09-04T09:00:00Z"
        )
    }

    func testActionRunWithItsOwnOpenPrTargetsTheSession() {
        let row = session(
            id: "cs-action",
            actionName: "Refresh screenshots",
            branch: "exp/refresh-screenshots-a1b2c3d4",
            prUrl: "https://github.com/acme/web/pull/12",
            prNumber: 12,
            prState: DomainContract.prStateOpen
        )
        XCTAssertEqual(
            MergeTargetResolution.resolve(session: row, issue: nil, openBatchPrs: []),
            .session(sessionId: "cs-action")
        )
    }

    func testChatRunWithItsOwnOpenPrTargetsTheSession() {
        // A chat run is issue-less AND action-less, like a batch run — but its
        // `exp/chat-` branch matches no open batch PR, so it falls through to
        // its own stamped one.
        let row = session(
            id: "cs-chat",
            status: DomainContract.codingSessionStatusInReview,
            branch: "exp/chat-a1b2c3d4",
            prUrl: "https://github.com/acme/web/pull/13",
            prNumber: 13,
            prState: DomainContract.prStateOpen
        )
        XCTAssertEqual(
            MergeTargetResolution.resolve(session: row, issue: nil, openBatchPrs: []),
            .session(sessionId: "cs-chat")
        )
    }

    func testIssueRunTargetsItsIssue() {
        let row = session(id: "cs-issue", issueId: "i-1")
        XCTAssertEqual(
            MergeTargetResolution.resolve(session: row, issue: issue(), openBatchPrs: []),
            .issue(issueId: "i-1")
        )
        // A merged or closed issue PR leaves nothing to merge — and the run's
        // own PR columns are NULL on an issue run, so nothing falls through.
        XCTAssertNil(
            MergeTargetResolution.resolve(
                session: row,
                issue: issue(prState: DomainContract.prStateMerged),
                openBatchPrs: []
            )
        )
    }

    func testMergedOrClosedRunPrOffersNothing() {
        for state in [DomainContract.prStateMerged, DomainContract.prStateClosed] {
            let row = session(
                id: "cs-\(state)",
                actionName: "Refresh screenshots",
                prUrl: "https://github.com/acme/web/pull/12",
                prNumber: 12,
                prState: state
            )
            XCTAssertNil(
                MergeTargetResolution.resolve(session: row, issue: nil, openBatchPrs: []),
                "\(state) must offer no merge"
            )
        }
        // A stamped state with no url is not a PR either.
        let urlless = session(
            actionName: "Refresh screenshots", prState: DomainContract.prStateOpen
        )
        XCTAssertNil(
            MergeTargetResolution.resolve(session: urlless, issue: nil, openBatchPrs: [])
        )
    }

    func testBatchRunStillTargetsItsRepresentativeIssue() {
        let batchUrl = "https://github.com/acme/web/pull/7"
        let representative = issue(id: "i-batch", prUrl: batchUrl, branch: "exp/batch-a1b2c3d4")
        let open = BatchPrResolution.openBatchPrs(
            issues: [representative], teamBoardIds: ["b-1"]
        )
        let row = session(
            id: "cs-batch",
            status: DomainContract.codingSessionStatusInReview,
            branch: "exp/batch-a1b2c3d4"
        )
        XCTAssertEqual(
            MergeTargetResolution.resolve(session: row, issue: nil, openBatchPrs: open),
            .issue(issueId: "i-batch")
        )
    }
}
