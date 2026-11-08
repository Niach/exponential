import Foundation
import XCTest
@testable import ExpCore

// EXP-1188: the agent-prose link rule, locked ×4 (web `app-link.test.ts`,
// desktop `domain::app_link`, Android `AppLinkTest`) against the ONE contract
// fixture.
final class AppLinkTests: XCTestCase {
    private var repoRoot: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
    }

    private func fixture() throws -> [String: Any] {
        let url = repoRoot.appendingPathComponent("packages/domain-contract/fixtures/app-link.json")
        let data = try Data(contentsOf: url)
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    private func expected(_ raw: [String: Any]) throws -> AppLink {
        switch try XCTUnwrap(raw["kind"] as? String) {
        case "issue":
            return .issue(
                teamSlug: try XCTUnwrap(raw["teamSlug"] as? String),
                boardSlug: try XCTUnwrap(raw["boardSlug"] as? String),
                identifier: try XCTUnwrap(raw["identifier"] as? String)
            )
        case "issueRef":
            return .issueRef(
                teamSlug: try XCTUnwrap(raw["teamSlug"] as? String),
                identifier: try XCTUnwrap(raw["identifier"] as? String)
            )
        case "board":
            return .board(
                teamSlug: try XCTUnwrap(raw["teamSlug"] as? String),
                boardSlug: try XCTUnwrap(raw["boardSlug"] as? String)
            )
        case "plan":
            return .plan(
                teamSlug: try XCTUnwrap(raw["teamSlug"] as? String),
                sessionId: try XCTUnwrap(raw["sessionId"] as? String),
                messageId: try XCTUnwrap(raw["messageId"] as? String)
            )
        case "chats":
            return .chats(teamSlug: try XCTUnwrap(raw["teamSlug"] as? String))
        case "agent":
            return .agent(
                teamSlug: try XCTUnwrap(raw["teamSlug"] as? String),
                agentId: try XCTUnwrap(raw["agentId"] as? String)
            )
        case "agentHome":
            return .agentHome(teamSlug: try XCTUnwrap(raw["teamSlug"] as? String))
        case "inbox":
            return .inbox(teamSlug: try XCTUnwrap(raw["teamSlug"] as? String))
        case "devices":
            return .devices(teamSlug: try XCTUnwrap(raw["teamSlug"] as? String))
        case "settings":
            return .settings(
                teamSlug: try XCTUnwrap(raw["teamSlug"] as? String),
                section: try XCTUnwrap(raw["section"] as? String)
            )
        case "appHome":
            return .appHome(
                teamSlug: try XCTUnwrap(raw["teamSlug"] as? String),
                appSlug: try XCTUnwrap(raw["appSlug"] as? String)
            )
        case "session":
            return .session(
                teamSlug: try XCTUnwrap(raw["teamSlug"] as? String),
                sessionId: try XCTUnwrap(raw["sessionId"] as? String)
            )
        case "appScreen":
            return .appScreen(
                teamSlug: try XCTUnwrap(raw["teamSlug"] as? String),
                appSlug: try XCTUnwrap(raw["appSlug"] as? String),
                screen: try XCTUnwrap(raw["screen"] as? String),
                params: try XCTUnwrap(raw["params"] as? [String: String])
            )
        case "app":
            return .app(path: try XCTUnwrap(raw["path"] as? String))
        case "external":
            return .external(url: try XCTUnwrap(raw["url"] as? String))
        case "ignore":
            return .ignore
        case let other:
            XCTFail("unknown kind \(other)")
            return .ignore
        }
    }

    func testEveryFixtureCaseClassifiesTheSame() throws {
        let root = try fixture()
        let origin = try XCTUnwrap(root["origin"] as? String)
        let cases = try XCTUnwrap(root["cases"] as? [[String: Any]])
        let defaultTeam = root["team"] as? String
        XCTAssertGreaterThanOrEqual(cases.count, 23)
        for object in cases {
            let name = try XCTUnwrap(object["name"] as? String)
            let href = try XCTUnwrap(object["href"] as? String)
            let link = try expected(try XCTUnwrap(object["link"] as? [String: Any]))
            // A case's own `team` wins (null = no team); absent = the fixture's.
            let team = object.keys.contains("team") ? object["team"] as? String : defaultTeam
            XCTAssertEqual(AppLink.classify(href, origin: origin, team: team), link, name)
        }
    }

    // #EXP-1674: the opening rule, `openCases`.
    func testEveryOpenCaseHasTheSameOutcome() throws {
        let root = try fixture()
        let cases = try XCTUnwrap(root["openCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for object in cases {
            let name = try XCTUnwrap(object["name"] as? String)
            let input = try XCTUnwrap(object["input"] as? [String: Bool])
            let outcome = AppLink.linkOpenOutcome(
                member: try XCTUnwrap(input["member"]),
                activeTeam: try XCTUnwrap(input["activeTeam"]),
                found: try XCTUnwrap(input["found"]),
                webOnly: try XCTUnwrap(input["webOnly"])
            )
            XCTAssertEqual(outcome.rawValue, try XCTUnwrap(object["outcome"] as? String), name)
        }
    }

    func testNoAccessCopyIsTheFixtures() throws {
        let copy = try XCTUnwrap(try fixture()["copy"] as? [String: String])
        XCTAssertEqual(AppLink.noAccessCopy, copy["noAccess"])
    }

    // `encodeURIComponent`'s set, and the slug decodes back.
    func testExpandTeamLinkEncodesTheTeamLikeEncodeURIComponent() {
        XCTAssertEqual(AppLink.expandTeamLink("~/inbox", team: "a b/c?d"), "/t/a%20b%2Fc%3Fd/inbox")
        XCTAssertEqual(AppLink.expandTeamLink("~/inbox", team: "a-_.!~*'()"), "/t/a-_.!~*'()/inbox")
        XCTAssertEqual(AppLink.expandTeamLink("~/inbox", team: "ä"), "/t/%C3%A4/inbox")
        XCTAssertEqual(
            AppLink.classify("~/inbox", origin: "https://app.exponential.at", team: "a b/c?d"),
            .inbox(teamSlug: "a b/c?d")
        )
        XCTAssertNil(AppLink.expandTeamLink("~/inbox", team: ""))
    }

    func testTeamSlugReadsAnAppPathsTeam() {
        XCTAssertEqual(AppLink.app(path: "/t/my%20team/reviews?x=1").teamSlug, "my team")
        XCTAssertNil(AppLink.app(path: "/admin").teamSlug)
        XCTAssertEqual(AppLink.inbox(teamSlug: "acme").teamSlug, "acme")
        XCTAssertNil(AppLink.ignore.teamSlug)
    }

    // VAPP-218: the screen params' query rule beyond the fixture's cases.
    func testAppScreenQueryKeepsBadEscapesAndPlus() {
        XCTAssertEqual(
            AppLink.classify("/t/acme/apps/crm/deals?a=%zz&b=x+y&=skip&c=1=2", origin: "https://app.exponential.at"),
            .appScreen(teamSlug: "acme", appSlug: "crm", screen: "deals", params: ["a": "%zz", "b": "x+y", "c": "1=2"])
        )
    }

    // #EXP-1674: plan-card.json `linkCases`, the same cases ×4.
    func testCardLinkCases() throws {
        let url = repoRoot.appendingPathComponent("packages/domain-contract/fixtures/plan-card.json")
        let card = try XCTUnwrap(try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any])
        let origin = try XCTUnwrap(card["linkOrigin"] as? String)
        let defaultTeam = try XCTUnwrap(card["linkTeam"] as? String)
        let cases = try XCTUnwrap(card["linkCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for item in cases {
            let name = try XCTUnwrap(item["name"] as? String)
            let href = try XCTUnwrap(item["href"] as? String)
            let opens = try XCTUnwrap(item["opens"] as? Bool)
            // Absent = the fixture's team; null = no team.
            let team = item.keys.contains("team") ? item["team"] as? String : defaultTeam
            XCTAssertEqual(AppLink.cardLinkOpens(href, origin: origin, team: team), opens, name)
        }
    }

    func testOriginTrailingSlashIsTolerated() {
        XCTAssertEqual(
            AppLink.classify("https://app.exponential.at/t/acme/sessions/abc", origin: "https://app.exponential.at/"),
            .session(teamSlug: "acme", sessionId: "abc")
        )
    }
}
