import Foundation
import XCTest
@testable import ExpCore

// EXP-825: the preselection a play button hands the Agent page composer.
// Hashable (it rides an `AppRoute` case), `action` wins over `issues` when
// both arrive (the web search-param rule), and the FAB's seed is empty.
final class AgentComposerSeedTests: XCTestCase {
    func testTheEmptySeedNamesNoSubject() {
        XCTAssertFalse(AgentComposerSeed.empty.hasSubject)
        XCTAssertEqual(AgentComposerSeed.empty.effectiveIssueIds, [])
        XCTAssertNil(AgentComposerSeed.empty.actionId)
        XCTAssertNil(AgentComposerSeed.empty.deviceId)
        XCTAssertNil(AgentComposerSeed.empty.prIssueId)
        XCTAssertNil(AgentComposerSeed.empty.text)
        XCTAssertNil(AgentComposerSeed.empty.icon)
        XCTAssertNil(AgentComposerSeed.empty.teamId)
        XCTAssertEqual(AgentComposerSeed.empty, AgentComposerSeed())
    }

    // An issue opened from the Inbox/Reviews/Search may sit on a non-active
    // team: the seed carries that team so the composer's pools match it.
    func testATeamRidesWithTheSubject() {
        let seed = AgentComposerSeed(issueIds: ["a"], teamId: "team-2")
        XCTAssertEqual(seed.teamId, "team-2")
        XCTAssertTrue(seed.hasSubject)
        // The team alone names no subject.
        XCTAssertFalse(AgentComposerSeed(teamId: "team-2").hasSubject)
    }

    func testIssuesAreASubjectInPickOrder() {
        let seed = AgentComposerSeed(issueIds: ["b", "a"])
        XCTAssertTrue(seed.hasSubject)
        XCTAssertEqual(seed.effectiveIssueIds, ["b", "a"])
    }

    func testAnActionWinsOverIssues() {
        let seed = AgentComposerSeed(issueIds: ["a"], actionId: "builtin:fix-conflicts", prIssueId: "a")
        XCTAssertTrue(seed.hasSubject)
        XCTAssertEqual(seed.actionId, "builtin:fix-conflicts")
        // The issue ids are still carried (the route hashes them) but the
        // composer checks NONE of them.
        XCTAssertEqual(seed.issueIds, ["a"])
        XCTAssertEqual(seed.effectiveIssueIds, [])
    }

    func testADeviceAloneIsNotASubject() {
        let seed = AgentComposerSeed(deviceId: "d-1")
        XCTAssertFalse(seed.hasSubject)
        XCTAssertEqual(seed.deviceId, "d-1")
    }

    // Two pushes with different seeds must be two different destinations.
    func testSeedsHashByEveryField() {
        let base = AgentComposerSeed(
            issueIds: ["a"], actionId: nil, deviceId: "d", prIssueId: nil, text: "t", icon: "rocket"
        )
        XCTAssertEqual(base, base)
        XCTAssertEqual(base.hashValue, base.hashValue)
        XCTAssertNotEqual(base, AgentComposerSeed(issueIds: ["a"], deviceId: "d", text: "t", icon: "bug"))
        XCTAssertNotEqual(base, AgentComposerSeed(issueIds: ["a", "b"], deviceId: "d", text: "t", icon: "rocket"))
        XCTAssertNotEqual(base, AgentComposerSeed(issueIds: ["a"], deviceId: "e", text: "t", icon: "rocket"))
        XCTAssertNotEqual(base, AgentComposerSeed(issueIds: ["a"], deviceId: "d", text: "u", icon: "rocket"))
        XCTAssertNotEqual(base, AgentComposerSeed(issueIds: ["a"], deviceId: "d", prIssueId: "p", text: "t", icon: "rocket"))
        XCTAssertNotEqual(base, AgentComposerSeed(issueIds: ["a"], deviceId: "d", text: "t", icon: "rocket", teamId: "team-2"))
        var set: Set<AgentComposerSeed> = [base]
        set.insert(AgentComposerSeed(issueIds: ["a"], deviceId: "d", text: "t", icon: "rocket"))
        XCTAssertEqual(set.count, 1)
    }
}
