import Foundation
import XCTest
@testable import ExpCore

// EXP-825: the preselection a play button hands the home composer. Hashable
// (it rides an `AppRoute` case), `action` wins over `issues` when both arrive
// (the web search-param rule), and the FAB's seed is empty.
final class AgentComposerSeedTests: XCTestCase {
    func testTheEmptySeedNamesNoSubject() {
        XCTAssertFalse(AgentComposerSeed.empty.hasSubject)
        XCTAssertEqual(AgentComposerSeed.empty.effectiveIssueIds, [])
        XCTAssertNil(AgentComposerSeed.empty.promptId)
        XCTAssertNil(AgentComposerSeed.empty.deviceId)
        XCTAssertNil(AgentComposerSeed.empty.text)
        XCTAssertNil(AgentComposerSeed.empty.teamId)
        XCTAssertEqual(AgentComposerSeed.empty, AgentComposerSeed())
    }

    // An issue opened from the Inbox/Search may sit on a non-active team: the
    // seed carries that team so the composer's pools match it.
    func testATeamRidesWithTheSubject() {
        let seed = AgentComposerSeed(issueIds: ["a"], teamId: "team-2")
        XCTAssertEqual(seed.teamId, "team-2")
        XCTAssertTrue(seed.hasSubject)
        // The team alone names no subject.
        XCTAssertFalse(AgentComposerSeed(teamId: "team-2").hasSubject)
    }

    // #EXP-2172: Fix with an agent seeds the run's repository; it wins over
    // the default once the team's list holds it, else the default stays.
    func testASeededRepositoryWinsWhenTheTeamHasIt() {
        XCTAssertNil(AgentComposerSeed.empty.repositoryId)
        XCTAssertFalse(AgentComposerSeed(text: "fix", teamId: "t", repositoryId: "r2").hasSubject)
        XCTAssertEqual(AgentComposerSeed.chatRepo(seeded: "r2", current: "", repoIds: ["r1", "r2"]), "r2")
        XCTAssertEqual(AgentComposerSeed.chatRepo(seeded: "r2", current: "r1", repoIds: ["r1", "r2"]), "r2")
        XCTAssertEqual(AgentComposerSeed.chatRepo(seeded: "gone", current: "", repoIds: ["r1", "r2"]), "r1")
        XCTAssertEqual(AgentComposerSeed.chatRepo(seeded: nil, current: "r2", repoIds: ["r1", "r2"]), "r2")
        XCTAssertEqual(AgentComposerSeed.chatRepo(seeded: nil, current: "", repoIds: []), "")
    }

    func testIssuesAreASubjectInPickOrder() {
        let seed = AgentComposerSeed(issueIds: ["b", "a"])
        XCTAssertTrue(seed.hasSubject)
        XCTAssertEqual(seed.effectiveIssueIds, ["b", "a"])
    }

    func testAnActionWinsOverIssues() {
        let seed = AgentComposerSeed(issueIds: ["a"], promptId: "act-1")
        XCTAssertTrue(seed.hasSubject)
        XCTAssertEqual(seed.promptId, "act-1")
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
        let base = AgentComposerSeed(issueIds: ["a"], deviceId: "d", text: "t")
        XCTAssertEqual(base, base)
        XCTAssertEqual(base.hashValue, base.hashValue)
        XCTAssertNotEqual(base, AgentComposerSeed(issueIds: ["a", "b"], deviceId: "d", text: "t"))
        XCTAssertNotEqual(base, AgentComposerSeed(issueIds: ["a"], deviceId: "e", text: "t"))
        XCTAssertNotEqual(base, AgentComposerSeed(issueIds: ["a"], deviceId: "d", text: "u"))
        XCTAssertNotEqual(base, AgentComposerSeed(issueIds: ["a"], promptId: "x", deviceId: "d", text: "t"))
        XCTAssertNotEqual(base, AgentComposerSeed(issueIds: ["a"], deviceId: "d", text: "t", teamId: "team-2"))
        var set: Set<AgentComposerSeed> = [base]
        set.insert(AgentComposerSeed(issueIds: ["a"], deviceId: "d", text: "t"))
        XCTAssertEqual(set.count, 1)
    }
}
