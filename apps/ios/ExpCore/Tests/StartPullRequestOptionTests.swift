import Foundation
import XCTest
@testable import ExpCore

// EXP-259/EXP-270: the `pr` input's options are the team's OPEN issue-linked
// pull requests deduped by prUrl — a batch PR (several issues, one PR) must
// appear ONCE, carrying a representative issue id and listing every linked
// identifier.
final class StartPullRequestOptionTests: XCTestCase {
    private func issue(
        id: String,
        boardId: String,
        identifier: String?,
        prUrl: String?,
        prNumber: Int?,
        prState: String = DomainContract.prStateOpen
    ) -> IssueEntity {
        IssueEntity(
            id: id,
            boardId: boardId,
            number: nil,
            identifier: identifier,
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
            prNumber: prNumber,
            prState: prState,
            branch: nil,
            prMergedAt: nil,
            createdAt: "2026-07-25T00:00:00Z",
            updatedAt: "2026-07-25T00:00:00Z"
        )
    }

    func testDedupesABatchPullRequestIntoOneOption() {
        let batchUrl = "https://github.com/acme/web/pull/7"
        let options = StartPullRequestOption.build(
            from: [
                issue(id: "i-2", boardId: "b-1", identifier: "EXP-2", prUrl: batchUrl, prNumber: 7),
                issue(id: "i-1", boardId: "b-1", identifier: "EXP-1", prUrl: batchUrl, prNumber: 7),
                issue(
                    id: "i-3",
                    boardId: "b-1",
                    identifier: "EXP-3",
                    prUrl: "https://github.com/acme/web/pull/9",
                    prNumber: 9
                ),
            ],
            teamBoardIds: ["b-1"]
        )

        XCTAssertEqual(options.count, 2)
        guard let batch = options.first(where: { $0.identifiers.count == 2 }) else {
            return XCTFail("the batch PR should collapse into one option")
        }
        XCTAssertEqual(batch.identifiers, ["EXP-1", "EXP-2"])
        // Representative id is deterministic (lowest id), not fetch-order bound.
        XCTAssertEqual(batch.issueId, "i-1")
        XCTAssertEqual(batch.label, "#7 · EXP-1, EXP-2")
        XCTAssertEqual(Set(batch.linkedIssueIds), ["i-1", "i-2"])
    }

    // EXP-323: the Reviews row hands over its OWN representative (the newest
    // linked issue), which is not this builder's (lowest id) — both must land
    // on the same option so the seeded picker renders a label, not a blank.
    func testResolvesAnyLinkedIssueIdToTheRepresentativeOption() {
        let batchUrl = "https://github.com/acme/web/pull/7"
        let options = StartPullRequestOption.build(
            from: [
                issue(id: "i-2", boardId: "b-1", identifier: "EXP-2", prUrl: batchUrl, prNumber: 7),
                issue(id: "i-1", boardId: "b-1", identifier: "EXP-1", prUrl: batchUrl, prNumber: 7),
                issue(
                    id: "i-3",
                    boardId: "b-1",
                    identifier: "EXP-3",
                    prUrl: "https://github.com/acme/web/pull/9",
                    prNumber: 9
                ),
            ],
            teamBoardIds: ["b-1"]
        )

        XCTAssertEqual(StartPullRequestOption.option(in: options, forIssueId: "i-2")?.issueId, "i-1")
        XCTAssertEqual(StartPullRequestOption.option(in: options, forIssueId: "i-1")?.issueId, "i-1")
        XCTAssertEqual(StartPullRequestOption.option(in: options, forIssueId: "i-3")?.issueId, "i-3")
        XCTAssertNil(StartPullRequestOption.option(in: options, forIssueId: "unknown"))
    }

    func testScopesToTheTeamsBoardsAndSkipsRowsWithoutAUrl() {
        let options = StartPullRequestOption.build(
            from: [
                issue(
                    id: "mine",
                    boardId: "b-1",
                    identifier: "EXP-1",
                    prUrl: "https://github.com/acme/web/pull/1",
                    prNumber: 1
                ),
                issue(
                    id: "other-team",
                    boardId: "b-9",
                    identifier: "OTH-1",
                    prUrl: "https://github.com/acme/other/pull/1",
                    prNumber: 1
                ),
                issue(id: "no-url", boardId: "b-1", identifier: "EXP-4", prUrl: nil, prNumber: nil),
            ],
            teamBoardIds: ["b-1"]
        )

        XCTAssertEqual(options.map(\.issueId), ["mine"])
    }

    // EXP-270 review: the open-PR guarantee lives in the helper itself, not in
    // the caller's SQL — merged/closed/nil-state rows must be excluded even
    // when the caller passes them through.
    func testExcludesIssuesWhosePullRequestIsNotOpen() {
        let options = StartPullRequestOption.build(
            from: [
                issue(
                    id: "open",
                    boardId: "b-1",
                    identifier: "EXP-1",
                    prUrl: "https://github.com/acme/web/pull/1",
                    prNumber: 1
                ),
                issue(
                    id: "merged",
                    boardId: "b-1",
                    identifier: "EXP-2",
                    prUrl: "https://github.com/acme/web/pull/2",
                    prNumber: 2,
                    prState: DomainContract.prStateMerged
                ),
                issue(
                    id: "closed",
                    boardId: "b-1",
                    identifier: "EXP-3",
                    prUrl: "https://github.com/acme/web/pull/3",
                    prNumber: 3,
                    prState: DomainContract.prStateClosed
                ),
            ],
            teamBoardIds: ["b-1"]
        )

        XCTAssertEqual(options.map(\.issueId), ["open"])
    }

    func testLabelFallsBackToIdentifiersWhenThePrNumberIsMissing() {
        let options = StartPullRequestOption.build(
            from: [
                issue(
                    id: "i-1",
                    boardId: "b-1",
                    identifier: "EXP-1",
                    prUrl: "https://github.com/acme/web/pull/1",
                    prNumber: nil
                ),
            ],
            teamBoardIds: ["b-1"]
        )

        XCTAssertEqual(options.first?.label, "EXP-1")
    }

    // EXP-1072: a workflow's OPEN final pull request is the workflow's own PR
    // — offered with the WORKFLOW id as its value, labelled like its title,
    // and a seed of that id resolves to it.
    func testOffersWorkflowsWithAnOpenFinalPullRequest() {
        func workflow(_ id: String, name: String, number: Int?, state: String?) -> WorkflowEntity {
            WorkflowEntity(
                id: id,
                teamId: "t-1",
                name: name,
                integrationBranch: "exp/wf-\(id)",
                finalPrUrl: state == nil ? nil : "https://github.com/acme/web/pull/\(number ?? 0)",
                finalPrNumber: number,
                finalPrState: state,
                createdAt: "2026-09-25T00:00:00Z",
                updatedAt: "2026-09-25T00:00:00Z"
            )
        }
        let options = StartPullRequestOption.build(
            from: [
                issue(
                    id: "i-1",
                    boardId: "b-1",
                    identifier: "EXP-1",
                    prUrl: "https://github.com/acme/web/pull/1",
                    prNumber: 1
                ),
            ],
            teamBoardIds: ["b-1"],
            workflows: [
                workflow("wf-open", name: "EXP-996 +5", number: 829, state: DomainContract.prStateOpen),
                workflow("wf-merged", name: "Done", number: 800, state: DomainContract.prStateMerged),
                workflow("wf-none", name: "Draft", number: nil, state: nil),
            ]
        )

        XCTAssertEqual(options.map(\.issueId), ["i-1", "wf-open"])
        let final = options.first { $0.issueId == "wf-open" }
        XCTAssertEqual(final?.label, "#829 · Workflow: EXP-996 +5")
        XCTAssertEqual(final?.label, WorkflowFinalPr.pickLabel(number: 829, name: "EXP-996 +5"))
        XCTAssertEqual(StartPullRequestOption.option(in: options, forIssueId: "wf-open")?.issueId, "wf-open")
    }
}
