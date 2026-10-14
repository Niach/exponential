import XCTest

@testable import ExpCore

// EXP-1303: the pull_requests stack merge, the same cases as Android's
// `PullRequestStackTest`.
final class PullRequestStackTests: XCTestCase {
    private func pr(
        _ id: String, _ number: Int, base: String? = nil, state: String = DomainContract.prStateOpen
    ) -> PullRequestEntity {
        PullRequestEntity(
            id: id, teamId: "team", repositoryId: "repo", number: number, url: "https://github.com/o/r/pull/\(number)",
            head: "exp/\(id)", base: "main", basePullRequestId: base, state: state,
            createdAt: "2026-10-14T00:00:00Z", updatedAt: "2026-10-14T00:00:00Z"
        )
    }

    func testALoneOpenPrHasNoStack() {
        XCTAssertNil(PullRequestStack.stackConfirm(pr("a", 1), in: [pr("a", 1)]))
    }

    func testMergeStackFromTheMiddleLandsTheWholeChainThroughItsTop() throws {
        let all = [pr("t", 3, base: "m"), pr("b", 1), pr("m", 2, base: "b")]
        let confirm = try XCTUnwrap(PullRequestStack.stackConfirm(all[2], in: all))
        XCTAssertEqual(confirm.title, DomainContract.diffUiMergeStack)
        XCTAssertEqual(confirm.body, "Lands 3 pull requests, bottom-up: #1, #2, #3.")
        XCTAssertEqual(confirm.throughId, "t")
    }

    func testTheTopMemberMergesThroughItself() throws {
        let all = [pr("b", 1), pr("t", 2, base: "b")]
        XCTAssertEqual(try XCTUnwrap(PullRequestStack.stackConfirm(all[1], in: all)).throughId, "t")
    }

    func testMergedMembersDropOutOfTheOpenChain() throws {
        let all = [pr("b", 1, state: DomainContract.prStateMerged), pr("m", 2, base: "b"), pr("t", 3, base: "m")]
        let confirm = try XCTUnwrap(PullRequestStack.stackConfirm(all[1], in: all))
        XCTAssertEqual(confirm.body, "Lands 2 pull requests, bottom-up: #2, #3.")
    }

    func testAForkHasNoStack() {
        let all = [pr("b", 1), pr("x", 2, base: "b"), pr("y", 3, base: "b")]
        XCTAssertNil(PullRequestStack.stackConfirm(all[0], in: all))
        XCTAssertNil(PullRequestStack.stackConfirm(all[1], in: all))
    }
}
