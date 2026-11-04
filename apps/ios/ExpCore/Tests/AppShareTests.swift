import Foundation
import XCTest
@testable import ExpCore

// VAPP-241: public links, locked ×4 (web app-share.ts, desktop
// `domain::app_share`, Android `AppShareTest`) against the ONE contract fixture.
final class AppShareTests: XCTestCase {
    private struct Fixture: Decodable {
        let testIds: TestIds
        let caps: Caps
        let copy: [String: String]
        let stateCases: [StateCase]
        let capCases: [CapCase]
        let bylineCases: [BylineCase]
        let rowCases: [RowCase]
        let historyCases: [HistoryCase]
        let switchCases: [SwitchCase]
    }

    private struct TestIds: Decodable {
        let shareLinks: String
        let shareRow: String
    }

    private struct Caps: Decodable {
        let perIpPerHour: Int
        let perLinkPerHour: Int
    }

    private struct Link: Decodable {
        let revokedAt: String?
        let expiresAt: String?
    }

    private struct StateCase: Decodable {
        let name: String
        let link: Link?
        let declared: Bool
        let now: String
        let state: String
    }

    private struct CapCase: Decodable {
        let name: String
        let ipCount: Int
        let linkCount: Int
        let allowed: Bool
    }

    private struct BylineCase: Decodable {
        let name: String
        let shareName: String?
        let byline: String
        let text: String
    }

    private struct SwitchCase: Decodable {
        let state: String
        let on: Bool
    }

    private struct RowCase: Decodable {
        let name: String
        let mode: String
        let state: String
        let expiresAt: String?
        let text: String
    }

    private struct HistoryCase: Decodable {
        let name: String
        let via: String?
        let actor: String?
        let names: [String: String]
        let who: String
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/app-share.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testConstants() throws {
        let f = try fixture()
        XCTAssertEqual(AppShare.TestIds.shareLinks, f.testIds.shareLinks)
        XCTAssertEqual(AppShare.TestIds.shareRow, f.testIds.shareRow)
        XCTAssertEqual(AppShare.Caps.perIpPerHour, f.caps.perIpPerHour)
        XCTAssertEqual(AppShare.Caps.perLinkPerHour, f.caps.perLinkPerHour)
        let pinned: [String: String] = [
            "title": AppShare.Copy.title, "submit": AppShare.Copy.submit, "view": AppShare.Copy.view,
            "off": AppShare.Copy.off, "on": AppShare.Copy.on, "expired": AppShare.Copy.expired,
            "orphaned": AppShare.Copy.orphaned, "expiresOn": AppShare.Copy.expiresOn,
            "copyLink": AppShare.Copy.copyLink, "newLink": AppShare.Copy.newLink, "guest": AppShare.Copy.guest,
        ]
        for (key, value) in pinned {
            XCTAssertEqual(f.copy[key], value, key)
        }
    }

    func testStateCases() throws {
        let cases = try fixture().stateCases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let link = c.link.map { AppShare.Link(revokedAt: $0.revokedAt, expiresAt: $0.expiresAt) }
            XCTAssertEqual(AppShare.linkState(link, declared: c.declared, now: c.now).rawValue, c.state, c.name)
        }
    }

    func testCapCases() throws {
        let cases = try fixture().capCases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            XCTAssertEqual(AppShare.underShareCaps(ipCount: c.ipCount, linkCount: c.linkCount), c.allowed, c.name)
        }
    }

    func testBylineCases() throws {
        let cases = try fixture().bylineCases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            XCTAssertEqual(AppShare.shareByline(c.shareName, byline: c.byline), c.text, c.name)
        }
    }

    func testSwitchCases() throws {
        let cases = try fixture().switchCases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let state = try XCTUnwrap(AppShare.LinkState(rawValue: c.state), c.state)
            XCTAssertEqual(AppShare.shareSwitchOn(state), c.on, c.state)
        }
    }

    func testRowCases() throws {
        let cases = try fixture().rowCases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let mode = try XCTUnwrap(AppShare.Mode(rawValue: c.mode), c.name)
            let state = try XCTUnwrap(AppShare.LinkState(rawValue: c.state), c.name)
            XCTAssertEqual(AppShare.shareRowSubtitle(mode, state, expiresAt: c.expiresAt), c.text, c.name)
        }
    }

    // The History list's `who` (`AppRecords.historyRows`).
    func testHistoryCases() throws {
        let cases = try fixture().historyCases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let event = AppRecords.RecordEvent(id: "e1", op: "update", actor: c.actor, via: c.via, before: nil, patch: nil)
            let rows = AppRecords.historyRows([event], names: c.names, fallback: "Agent")
            XCTAssertEqual(rows.first?.who, c.who, c.name)
        }
    }
}
