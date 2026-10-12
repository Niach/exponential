import Foundation
import XCTest
@testable import ExpCore

// EXP-1270 (EXP-1259 D6): the Getting started model, locked ×4 against
// `getting-started.json` — every case (pointer AND phone), the WHOLE expected
// object, the same cases web, the desktop and Android replay.
final class GettingStartedTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Step: Decodable {
            let key: String
            let title: String
            let icon: String
            let target: String
            let needsDevice: Bool?
            let visibleWhen: String?
            let phones: Bool
        }
        struct Signals: Decodable {
            let hasBoard: Bool
            let githubInstalled: Bool
            let hasDesktopDevice: Bool
            let hasServerDevice: Bool
            let hasCodingSession: Bool
            let hasInvitedTeam: Bool
            let mcpConnected: Bool
            let hasWidget: Bool
        }
        struct Permissions: Decodable {
            let canManageMembers: Bool
            let canManageWidgets: Bool
        }
        struct Entry: Decodable {
            let key: String
            let done: Bool
            let target: String
            let next: Bool
        }
        struct Expected: Decodable {
            let entries: [Entry]
            let done: Int
            let total: Int
            let progress: String
            let complete: Bool
        }
        struct Case: Decodable {
            let name: String
            let signals: Signals
            let permissions: Permissions
            let platform: String
            let expected: Expected
        }
        struct Docs: Decodable {
            let label: String
            let icon: String
            let url: String
        }
        struct Card: Decodable {
            let width: Double
            let height: Double
            let padding: Double
            let icon: Double
            let gap: Double
        }
        let title: String
        let progress: String
        let meter: Double
        let docs: Docs
        let card: Card
        let testIds: [String: String]
        let steps: [Step]
        let cases: [Case]
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/getting-started.json")
        return try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
    }

    func testReplaysEveryCaseAsAWholeObject() throws {
        let cases = try fixture().cases
        XCTAssertTrue(cases.contains { $0.platform == "phone" })
        XCTAssertTrue(cases.contains { $0.platform == "pointer" })
        for c in cases {
            let s = c.signals
            let state = GettingStarted.derive(
                GettingStarted.Signals(
                    hasBoard: s.hasBoard,
                    githubInstalled: s.githubInstalled,
                    hasDesktopDevice: s.hasDesktopDevice,
                    hasServerDevice: s.hasServerDevice,
                    hasCodingSession: s.hasCodingSession,
                    hasInvitedTeam: s.hasInvitedTeam,
                    mcpConnected: s.mcpConnected,
                    hasWidget: s.hasWidget
                ),
                permissions: GettingStarted.Permissions(
                    canManageMembers: c.permissions.canManageMembers,
                    canManageWidgets: c.permissions.canManageWidgets
                ),
                platform: try XCTUnwrap(GettingStarted.Platform(rawValue: c.platform), c.name)
            )
            let expected = GettingStarted.State(
                entries: try c.expected.entries.map { entry in
                    GettingStarted.Entry(
                        key: try XCTUnwrap(GettingStarted.Key(rawValue: entry.key), c.name),
                        done: entry.done,
                        target: try XCTUnwrap(GettingStarted.Target(rawValue: entry.target), c.name),
                        next: entry.next
                    )
                },
                done: c.expected.done,
                total: c.expected.total,
                progress: c.expected.progress,
                complete: c.expected.complete
            )
            XCTAssertEqual(state, expected, c.name)
        }
    }

    func testTheStepsAreTheFixtures() throws {
        let steps = try fixture().steps
        XCTAssertEqual(GettingStarted.steps.count, steps.count)
        for (mine, want) in zip(GettingStarted.steps, steps) {
            XCTAssertEqual(mine.key.rawValue, want.key)
            XCTAssertEqual(mine.title, want.title)
            XCTAssertEqual(GettingStarted.title(mine.key), want.title)
            XCTAssertEqual(mine.icon, want.icon)
            XCTAssertEqual(mine.target.rawValue, want.target)
            XCTAssertEqual(mine.needsDevice, want.needsDevice ?? false)
            XCTAssertEqual(mine.visibleWhen?.rawValue, want.visibleWhen)
            XCTAssertEqual(mine.phones, want.phones)
        }
    }

    func testTheHeaderCardAndTestIdsAreTheFixtures() throws {
        let f = try fixture()
        XCTAssertEqual(GettingStarted.title, f.title)
        XCTAssertEqual(
            GettingStarted.progress(done: 4, total: 8),
            f.progress.replacingOccurrences(of: "{done}", with: "4").replacingOccurrences(of: "{total}", with: "8")
        )
        XCTAssertEqual(GettingStarted.meterWidth, f.meter)
        XCTAssertEqual(GettingStarted.docsLabel, f.docs.label)
        XCTAssertEqual(GettingStarted.docsIcon, f.docs.icon)
        XCTAssertEqual(GettingStarted.docsUrl.absoluteString, f.docs.url)
        XCTAssertEqual(GettingStarted.Card.width, f.card.width)
        XCTAssertEqual(GettingStarted.Card.height, f.card.height)
        XCTAssertEqual(GettingStarted.Card.padding, f.card.padding)
        XCTAssertEqual(GettingStarted.Card.icon, f.card.icon)
        XCTAssertEqual(GettingStarted.Card.gap, f.card.gap)
        XCTAssertEqual(GettingStarted.TestId.block, f.testIds["block"])
        XCTAssertEqual(GettingStarted.TestId.docs, f.testIds["docs"])
        XCTAssertEqual(GettingStarted.TestId.entry, f.testIds["entry"])
        XCTAssertEqual(
            GettingStarted.TestId.card(.run),
            f.testIds["card"]?.replacingOccurrences(of: "{key}", with: "run")
        )
    }
}
