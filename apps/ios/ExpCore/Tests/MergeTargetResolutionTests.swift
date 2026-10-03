import Foundation
import XCTest
@testable import ExpCore

// EXP-734/SLOP-3: an issue-less run (batch, action, chat) owns the pull request
// it opened on its own row, so its Merge affordance targets the SESSION. Issue
// runs keep merging through their issue.
final class MergeTargetResolutionTests: XCTestCase {
    private func session(
        id: String = "cs-1",
        issueId: String? = nil,
        actionName: String? = nil,
        status: String = DomainContract.codingSessionStatusRunning,
        branch: String? = nil,
        prUrl: String? = nil,
        prNumber: Int? = nil,
        prState: String? = nil,
        batchIssueIds: String? = nil
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id,
            issueId: issueId,
            teamId: "ws-1",
            userId: "u-1",
            deviceLabel: "macbook",
            status: status,
            branch: branch,
            batchIssueIds: batchIssueIds,
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
            MergeTargetResolution.resolve(session: row, issue: nil),
            .session(sessionId: "cs-action")
        )
    }

    func testChatRunWithItsOwnOpenPrTargetsTheSession() {
        let row = session(
            id: "cs-chat",
            status: DomainContract.codingSessionStatusInReview,
            branch: "exp/chat-a1b2c3d4",
            prUrl: "https://github.com/acme/web/pull/13",
            prNumber: 13,
            prState: DomainContract.prStateOpen
        )
        XCTAssertEqual(
            MergeTargetResolution.resolve(session: row, issue: nil),
            .session(sessionId: "cs-chat")
        )
    }

    func testIssueRunTargetsItsIssue() {
        let row = session(id: "cs-issue", issueId: "i-1")
        XCTAssertEqual(
            MergeTargetResolution.resolve(session: row, issue: issue()),
            .issue(issueId: "i-1")
        )
        // A merged or closed issue PR leaves nothing to merge — and the run's
        // own PR columns are NULL on an issue run, so nothing falls through.
        XCTAssertNil(
            MergeTargetResolution.resolve(
                session: row,
                issue: issue(prState: DomainContract.prStateMerged)
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
                MergeTargetResolution.resolve(session: row, issue: nil),
                "\(state) must offer no merge"
            )
        }
        // A stamped state with no url is not a PR either.
        let urlless = session(
            actionName: "Refresh screenshots", prState: DomainContract.prStateOpen
        )
        XCTAssertNil(
            MergeTargetResolution.resolve(session: urlless, issue: nil)
        )
    }

    func testBatchRunTargetsItsOwnPr() {
        // SLOP-3: a batch run owns the combined PR it opened on its own row.
        let row = session(
            id: "cs-batch",
            status: DomainContract.codingSessionStatusInReview,
            branch: "exp/batch-a1b2c3d4",
            prUrl: "https://github.com/acme/web/pull/7",
            prNumber: 7,
            prState: DomainContract.prStateOpen
        )
        XCTAssertEqual(
            MergeTargetResolution.resolve(session: row, issue: nil),
            .session(sessionId: "cs-batch")
        )
        // No PR on its row: nothing to merge, whatever its branch says.
        XCTAssertNil(
            MergeTargetResolution.resolve(
                session: session(
                    id: "cs-batch-2",
                    status: DomainContract.codingSessionStatusInReview,
                    branch: "exp/batch-a1b2c3d4"
                ),
                issue: nil
            )
        )
    }

    // EXP-1165: a batch whose combined PR a covered issue carries (same url,
    // still open) merges through THAT issue — the stack choice and "Fix
    // conflicts" live on the issue path. Twin of web session-merge-target.
    func testBatchRunWithACarrierTargetsTheIssue() {
        let url = "https://github.com/acme/web/pull/7"
        let row = session(
            id: "cs-batch",
            status: DomainContract.codingSessionStatusInReview,
            branch: "exp/batch-a1b2c3d4",
            prUrl: url,
            prNumber: 7,
            prState: DomainContract.prStateOpen,
            batchIssueIds: "[\"i-1\",\"i-2\"]"
        )
        let other = issue(id: "i-1", prUrl: "https://github.com/acme/web/pull/99")
        let carrier = issue(id: "i-2", prUrl: url)
        XCTAssertEqual(
            MergeTargetResolution.resolve(session: row, issue: nil, batchIssues: [other, carrier]),
            .issue(issueId: "i-2")
        )
        // No carrier: the session row as before.
        XCTAssertEqual(
            MergeTargetResolution.resolve(session: row, issue: nil, batchIssues: [other]),
            .session(sessionId: "cs-batch")
        )
        // A merged carrier is no carrier.
        XCTAssertEqual(
            MergeTargetResolution.resolve(
                session: row,
                issue: nil,
                batchIssues: [issue(id: "i-2", prUrl: url, prState: DomainContract.prStateMerged)]
            ),
            .session(sessionId: "cs-batch")
        )
        // An issue the run does not cover never carries it.
        XCTAssertEqual(
            MergeTargetResolution.resolve(
                session: row, issue: nil, batchIssues: [issue(id: "i-9", prUrl: url)]
            ),
            .session(sessionId: "cs-batch")
        )
    }

    func testIssueRunIgnoresBatchIssues() {
        let row = session(id: "cs-issue", issueId: "i-1", prUrl: "https://github.com/acme/web/pull/3")
        XCTAssertEqual(
            MergeTargetResolution.resolve(
                session: row, issue: issue(), batchIssues: [issue(id: "i-2")]
            ),
            .issue(issueId: "i-1")
        )
    }
}
