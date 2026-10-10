import Foundation
import XCTest
@testable import ExpCore

// Polish round P14: phone My issues groups by the RESOLVED team status row
// (custom statuses get a group, started clocks follow the team's ramp), and
// rows sharing category + name across teams merge into one group.
final class CrossTeamStatusGroupsTests: XCTestCase {

    private func row(
        id: String,
        team: String,
        category: IssueStatusCategory,
        name: String,
        sortOrder: Double,
        builtinKey: IssueStatus? = nil
    ) -> IssueStatusEntity {
        IssueStatusEntity(
            id: id,
            teamId: team,
            category: category.rawValue,
            name: name,
            color: nil,
            sortOrder: sortOrder,
            builtinKey: builtinKey?.rawValue,
            createdAt: "2026-01-01 00:00:00+00",
            updatedAt: "2026-01-01 00:00:00+00"
        )
    }

    private func issue(id: String, board: String, status: String, statusId: String? = nil) -> IssueEntity {
        IssueEntity(
            id: id, boardId: board, number: nil, identifier: id, title: id,
            description: nil, status: status, statusId: statusId,
            priority: IssuePriority.none.rawValue,
            assigneeId: nil, creatorId: nil, source: nil, dueDate: nil,
            sortOrder: nil, completedAt: nil, duplicateOfId: nil,
            prUrl: nil, prNumber: nil, prState: nil, branch: nil, prMergedAt: nil,
            createdAt: "2026-06-01 10:00:00+00", updatedAt: "2026-06-01 10:00:00+00"
        )
    }

    /// A team with three started rows: In Progress, In QA (custom), In Review.
    private func threeStartedTeam(_ team: String) -> [IssueStatusEntity] {
        [
            row(id: "\(team)-backlog", team: team, category: .backlog, name: "Backlog", sortOrder: 0, builtinKey: .backlog),
            row(id: "\(team)-progress", team: team, category: .started, name: "In Progress", sortOrder: 0, builtinKey: .inProgress),
            row(id: "\(team)-qa", team: team, category: .started, name: "In QA", sortOrder: 1),
            row(id: "\(team)-review", team: team, category: .started, name: "In Review", sortOrder: 2, builtinKey: .inReview),
            row(id: "\(team)-done", team: team, category: .completed, name: "Done", sortOrder: 0, builtinKey: .done),
        ]
    }

    func testCustomStatusGetsItsOwnGroupWithTheTeamClock() {
        let rows = threeStartedTeam("t1")
        let issues = [
            issue(id: "a", board: "b1", status: "in_progress", statusId: "t1-qa"),
            issue(id: "b", board: "b1", status: "in_progress", statusId: "t1-progress"),
            issue(id: "c", board: "b1", status: "in_review", statusId: "t1-review"),
        ]
        let groups = CrossTeamStatusGroups.groups(
            issues: issues, teamIdOf: { _ in "t1" }, statusRows: rows
        )
        XCTAssertEqual(groups.map(\.status.name), ["In Progress", "In QA", "In Review"])
        XCTAssertEqual(
            groups.map(\.status.iconName), ["progress-1-4", "progress-2-4", "progress-3-4"]
        )
        XCTAssertEqual(groups.map { $0.issues.map(\.id) }, [["b"], ["a"], ["c"]])
    }

    func testSameNamedRowsMergeAcrossTeams() {
        let rows = threeStartedTeam("t1") + threeStartedTeam("t2")
        let issues = [
            issue(id: "a", board: "b1", status: "in_progress", statusId: "t1-progress"),
            issue(id: "b", board: "b2", status: "in_progress", statusId: "t2-progress"),
            issue(id: "c", board: "b2", status: "backlog", statusId: "t2-backlog"),
        ]
        let teamOfBoard = ["b1": "t1", "b2": "t2"]
        let groups = CrossTeamStatusGroups.groups(
            issues: issues, teamIdOf: { teamOfBoard[$0.boardId] }, statusRows: rows
        )
        XCTAssertEqual(groups.map(\.status.name), ["Backlog", "In Progress"])
        XCTAssertEqual(Set(groups[1].issues.map(\.id)), ["a", "b"])
    }

    func testUnsyncedTeamFallsBackToTheBuiltinDefaults() {
        let groups = CrossTeamStatusGroups.groups(
            issues: [issue(id: "a", board: "b1", status: "in_review")],
            teamIdOf: { _ in "t-unsynced" },
            statusRows: []
        )
        XCTAssertEqual(groups.count, 1)
        XCTAssertEqual(groups[0].status.builtinKey, .inReview)
    }
}
