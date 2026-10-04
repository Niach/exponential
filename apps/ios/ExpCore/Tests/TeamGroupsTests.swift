import Foundation
import XCTest
@testable import ExpCore

// EXP-1186: the cross-team lists' grouping — teams by name, empty teams out,
// unknown teams dropped, item order kept.
final class TeamGroupsTests: XCTestCase {
    private func team(_ id: String, _ name: String) -> TeamEntity {
        TeamEntity(
            id: id, name: name, slug: id, iconUrl: nil,
            createdAt: "2026-01-01T00:00:00Z", updatedAt: "2026-01-01T00:00:00Z"
        )
    }

    func testGroupsByTeamNameAndKeepsItemOrder() {
        let teams = [team("t-z", "zeta"), team("t-a", "Alpha"), team("t-e", "empty")]
        let items = [("1", "t-z"), ("2", "t-a"), ("3", "t-z"), ("4", "t-gone")]
        let groups = TeamGroups.group(items, teams: teams) { $0.1 }
        XCTAssertEqual(groups.map(\.team.id), ["t-a", "t-z"])
        XCTAssertEqual(groups[1].items.map(\.0), ["1", "3"])
    }

    func testOrderBreaksNameTiesById() {
        let ordered = TeamGroups.ordered([team("b", "Same"), team("a", "same")])
        XCTAssertEqual(ordered.map(\.id), ["a", "b"])
    }

    func testMultiTeamNeedsMoreThanOne() {
        XCTAssertFalse(TeamGroups.isMultiTeam([]))
        XCTAssertFalse(TeamGroups.isMultiTeam([team("a", "A")]))
        XCTAssertTrue(TeamGroups.isMultiTeam([team("a", "A"), team("b", "B")]))
    }
}
