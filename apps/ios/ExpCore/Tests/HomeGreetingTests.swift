import Foundation
import XCTest
@testable import ExpCore

// EXP-1253 (EXP-1259 D1): the home greeting, locked ×4 against
// `composer-home.json` — the same `cases` web (`home-greeting.test.ts`), the
// desktop (`ui::home_greeting`) and Android (`HomeGreetingTest`) replay.
final class HomeGreetingTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Bucket: Decodable {
            let id: String
            let from: Int
            let to: Int
            let variants: [String]
        }
        struct Case: Decodable {
            let name: String
            let local: String
            let user: String?
            let expected: String
        }
        struct Geometry: Decodable {
            let mark: Double
            let markPhone: Double
            let gap: Double
            let headline: Double
            let headlinePhone: Double
            let headlineWeight: Int
            let headlineToCard: Double
            let optionsToRecent: Double
        }
        struct Recent: Decodable {
            let label: String
            let count: Int
        }
        let placeholder: String
        let geometry: Geometry
        let recent: Recent
        let dayStartsAtHour: Int
        let buckets: [Bucket]
        let cases: [Case]
        let testIds: [String: String]
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/composer-home.json")
        return try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
    }

    func testReplaysEveryFixtureCase() throws {
        let cases = try fixture().cases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            XCTAssertEqual(HomeGreeting.greeting(local: c.local, name: c.user), c.expected, c.name)
        }
    }

    func testTheBucketsAndCopyAreTheFixtures() throws {
        let f = try fixture()
        XCTAssertEqual(HomeGreeting.buckets.count, f.buckets.count)
        for (mine, want) in zip(HomeGreeting.buckets, f.buckets) {
            XCTAssertEqual(mine.id, want.id)
            XCTAssertEqual(mine.from, want.from)
            XCTAssertEqual(mine.to, want.to)
            XCTAssertEqual(mine.variants, want.variants)
        }
        XCTAssertEqual(HomeGreeting.placeholder, f.placeholder)
        XCTAssertEqual(HomeGreeting.dayStartsAtHour, f.dayStartsAtHour)
        XCTAssertEqual(HomeGreeting.recentLabel, f.recent.label)
        XCTAssertEqual(HomeGreeting.recentCount, f.recent.count)
        XCTAssertEqual(HomeGreeting.Geometry.mark, f.geometry.mark)
        XCTAssertEqual(HomeGreeting.Geometry.markPhone, f.geometry.markPhone)
        XCTAssertEqual(HomeGreeting.Geometry.gap, f.geometry.gap)
        XCTAssertEqual(HomeGreeting.Geometry.headline, f.geometry.headline)
        XCTAssertEqual(HomeGreeting.Geometry.headlinePhone, f.geometry.headlinePhone)
        XCTAssertEqual(HomeGreeting.Geometry.headlineWeight, f.geometry.headlineWeight)
        XCTAssertEqual(HomeGreeting.Geometry.headlineToCard, f.geometry.headlineToCard)
        XCTAssertEqual(HomeGreeting.Geometry.optionsToRecent, f.geometry.optionsToRecent)
        XCTAssertEqual(HomeGreeting.TestId.home, f.testIds["home"])
        XCTAssertEqual(HomeGreeting.TestId.greeting, f.testIds["greeting"])
        XCTAssertEqual(HomeGreeting.TestId.mark, f.testIds["mark"])
        XCTAssertEqual(HomeGreeting.TestId.recent, f.testIds["recent"])
        XCTAssertEqual(HomeGreeting.TestId.recentRow, f.testIds["recentRow"])
    }

    func testTheWallClockIsTheDevicesLocalTime() {
        let zone = TimeZone(identifier: "Europe/Vienna")!
        // 2026-10-12T17:30Z = 19:30 in Vienna (CEST, +2).
        let date = Date(timeIntervalSince1970: 1_791_826_200)
        XCTAssertEqual(HomeGreeting.localWallClock(date, timeZone: zone), "2026-10-12T19:30")
        XCTAssertEqual(HomeGreeting.greeting(now: date, name: "Danny S", timeZone: zone), "Evening, Danny")
    }

    func testAMalformedTimeGreetsNobody() {
        XCTAssertEqual(HomeGreeting.greeting(local: "yesterday", name: "Danny"), "")
    }

    // MARK: - Recent

    private func run(
        _ id: String,
        userId: String = "me",
        teamId: String = "t1",
        status: String = "running",
        parent: String? = nil,
        updatedAt: String,
        createdAt: String = "2026-10-01T00:00:00Z"
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id, issueId: nil, teamId: teamId, userId: userId, deviceLabel: "macbook",
            status: status, parentSessionId: parent, startedAt: createdAt, endedAt: nil,
            createdAt: createdAt, updatedAt: updatedAt
        )
    }

    func testRecentIsTheLastThreeOwnTopLevelRunsOfTheTeam() {
        let rows = [
            run("old", status: "ended", updatedAt: "2026-10-01T10:00:00Z"),
            run("a", updatedAt: "2026-10-05T10:00:00Z"),
            run("b", status: "ended", updatedAt: "2026-10-04T10:00:00Z"),
            run("child", parent: "a", updatedAt: "2026-10-06T10:00:00Z"),
            run("theirs", userId: "other", updatedAt: "2026-10-07T10:00:00Z"),
            run("elsewhere", teamId: "t2", updatedAt: "2026-10-07T10:00:00Z"),
            run("c", status: "in_review", updatedAt: "2026-10-03T10:00:00Z"),
        ]
        XCTAssertEqual(
            HomeGreeting.recentRuns(rows, userId: "me", teamId: "t1").map(\.id),
            ["a", "b", "c"]
        )
    }

    func testRecentBreaksUpdatedTiesOnCreatedAndDedupes() {
        let same = "2026-10-05 10:00:00+00"
        let rows = [
            run("x", updatedAt: same, createdAt: "2026-10-01T00:00:00Z"),
            run("y", updatedAt: same, createdAt: "2026-10-02T00:00:00Z"),
            run("y", updatedAt: same, createdAt: "2026-10-02T00:00:00Z"),
        ]
        XCTAssertEqual(HomeGreeting.recentRuns(rows, userId: "me", teamId: "t1").map(\.id), ["y", "x"])
        XCTAssertTrue(HomeGreeting.recentRuns(rows, userId: nil, teamId: "t1").isEmpty)
        XCTAssertTrue(HomeGreeting.recentRuns(rows, userId: "me", teamId: nil).isEmpty)
    }
}
