import Foundation
import XCTest
@testable import ExpCore

// EXP-259/EXP-270 (EXP-1291: off `pull_requests`): the `pr` input's options
// are the team's OPEN pull requests, one row each, valued by the pull
// request's id.
final class StartPullRequestOptionTests: XCTestCase {
    private func pr(
        _ id: String, team: String = "t1", number: Int, state: String = "open", identifiers: [String] = []
    ) -> StartPullRequestOption.Candidate {
        .init(id: id, teamId: team, number: number, state: state, identifiers: identifiers)
    }

    func testOnlyTheTeamsOpenPullRequests() {
        let options = StartPullRequestOption.build(
            from: [
                pr("a", number: 1, identifiers: ["EXP-2", "EXP-1"]),
                pr("b", number: 2, state: "merged"),
                pr("c", number: 3, state: "closed"),
                pr("d", team: "t2", number: 4),
                pr("e", number: 5, state: "draft"),
            ],
            teamId: "t1"
        )
        XCTAssertEqual(options, [StartPullRequestOption(id: "a", number: 1, identifiers: ["EXP-1", "EXP-2"])])
    }

    func testLabelsNameTheNumberThenTheIssues() {
        XCTAssertEqual(StartPullRequestOption(id: "a", number: 42, identifiers: []).label, "#42")
        XCTAssertEqual(
            StartPullRequestOption(id: "a", number: 42, identifiers: ["EXP-1", "EXP-2"]).label,
            "#42 · EXP-1, EXP-2"
        )
    }

    func testOrderIsStableByLabel() {
        let options = StartPullRequestOption.build(
            from: [pr("z", number: 9), pr("y", number: 10), pr("x", number: 9)],
            teamId: "t1"
        )
        XCTAssertEqual(options.map(\.id), ["y", "x", "z"])
    }
}
