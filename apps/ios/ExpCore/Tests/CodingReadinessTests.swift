import Foundation
import XCTest
@testable import ExpCore

// EXP-1121: the readiness model + its copy, fixture-locked ×4 (web
// `coding-readiness.test.ts`, Android `CodingReadinessTest`, desktop
// `domain::coding_readiness`) against the ONE contract fixture — same copy
// table, same ago cases, same named cases. Change one, change all four.
final class CodingReadinessTests: XCTestCase {
    private struct Fixture: Decodable {
        let copy: [String: String]
        let ago: [AgoCase]
        let cases: [FixtureCase]
    }

    private struct AgoCase: Decodable {
        let nowMs: Int64
        let thenMs: Int64
        let expected: String
    }

    private struct FixtureCase: Decodable {
        let name: String
        let input: FixtureInput
        let expected: FixtureReadiness
    }

    private struct FixtureDevice: Decodable {
        let label: String
        let own: Bool
        let online: Bool
        let lastSeenAtMs: Int64?
    }

    private struct FixtureGithub: Decodable {
        let connected: Bool
        let label: String?
    }

    private struct FixtureInput: Decodable {
        let isMember: Bool
        let remoteStartEnabled: Bool?
        let teamName: String
        let boardName: String
        let nowMs: Int64
        let boardRepository: String?
        let github: FixtureGithub?
        let devices: [FixtureDevice]?

        var input: CodingReadiness.Input {
            CodingReadiness.Input(
                isMember: isMember,
                remoteStartEnabled: remoteStartEnabled,
                teamName: teamName,
                boardName: boardName,
                boardRepository: boardRepository,
                github: github.map { CodingReadiness.Github(connected: $0.connected, label: $0.label) },
                devices: devices?.map {
                    CodingReadiness.Device(
                        label: $0.label, own: $0.own, online: $0.online, lastSeenAtMs: $0.lastSeenAtMs
                    )
                },
                nowMs: nowMs
            )
        }
    }

    private struct FixtureStep: Decodable {
        let key: String
        let state: String
        let title: String
        let body: String?
        let detail: String?
        let fixes: [String]

        var step: CodingReadiness.Step? {
            guard let key = CodingReadiness.StepKey(rawValue: key),
                  let state = CodingReadiness.StepState(rawValue: state)
            else { return nil }
            let fixes = fixes.compactMap(CodingReadiness.Fix.init(rawValue:))
            guard fixes.count == self.fixes.count else { return nil }
            return CodingReadiness.Step(
                key: key, state: state, title: title, body: body, detail: detail, fixes: fixes
            )
        }
    }

    private struct FixtureReadiness: Decodable {
        let visible: Bool
        let loading: Bool
        let ready: Bool
        let metCount: Int
        let total: Int
        let steps: [FixtureStep]
        let summary: String
        let caption: String?

        var readiness: CodingReadiness.Readiness {
            CodingReadiness.Readiness(
                visible: visible, loading: loading, ready: ready, metCount: metCount,
                total: total, steps: steps.compactMap(\.step), summary: summary, caption: caption
            )
        }
    }

    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/coding-readiness.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testLocksTheCopyTable() throws {
        var derived = CodingReadiness.Copy.table
        derived["repositoryTitle(App)"] = CodingReadiness.repositoryTitle("App")
        derived["repositoryBody(App)"] = CodingReadiness.repositoryBody("App")
        derived["deviceBody(Acme)"] = CodingReadiness.deviceBody("Acme")
        derived["lastSeen(MacBook Pro, 2 h ago)"] = CodingReadiness.lastSeen("MacBook Pro", "2 h ago")
        derived["pickerUsedBy(Website)"] = CodingReadiness.pickerUsedBy("Website")
        derived["summary(0,3)"] = CodingReadiness.summary(met: 0, total: 3)
        derived["summary(2,3)"] = CodingReadiness.summary(met: 2, total: 3)
        derived["summary(3,3)"] = CodingReadiness.summary(met: 3, total: 3)
        XCTAssertEqual(derived, try fixture().copy)
    }

    func testAgo() throws {
        let cases = try fixture().ago
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            XCTAssertEqual(
                CodingReadiness.ago(nowMs: c.nowMs, thenMs: c.thenMs), c.expected,
                "ago \(c.expected)"
            )
        }
    }

    func testFixtureCases() throws {
        let cases = try fixture().cases
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            // Every fixture step/fix decodes into the Swift enums.
            XCTAssertEqual(c.expected.steps.compactMap(\.step).count, c.expected.steps.count, c.name)
            XCTAssertEqual(CodingReadiness.derive(c.input.input), c.expected.readiness, c.name)
        }
    }

    func testFixLabels() {
        XCTAssertEqual(CodingReadiness.Fix.chooseRepository.label, "Choose repository")
        XCTAssertEqual(CodingReadiness.Fix.setUpServer.label, "Set up a server")
    }
}
