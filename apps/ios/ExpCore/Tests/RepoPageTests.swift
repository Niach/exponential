import Foundation
import XCTest
@testable import ExpCore

// #EXP-1616: the repo page ×4, replayed by path against packages/domain-
// contract/fixtures/repo-page.json (TS `repo-page.ts`): EVERY `treeCases`,
// `branchCases`, `rollupCases` and `tabCases` entry, comparing the WHOLE
// result, plus the copy and tabs.
final class RepoPageTests: XCTestCase {
    private struct Fixture: Decodable {
        let tabs: [TabEntry]
        let copy: CopyJSON
        let treeCases: [TreeCase]
        let branchCases: [BranchCase]
        let rollupCases: [RollupCase]
        let tabCases: [TabCase]
    }

    private struct TabEntry: Decodable { let id: String; let label: String }

    private struct CopyJSON: Decodable {
        let provider: [String: String]
        let add: [String: String]
        let pulls: [String: String]
        let runs: [String: String]
        let branches: [String: String]
        let settings: [String: String]
        let checks: [String: CheckJSON]
    }

    private struct CheckJSON: Decodable { let label: String; let tone: String }

    private struct PrJSON: Decodable {
        let id: String
        let number: Int
        let title: String?
        let head: String
        let base: String
        let state: String
        let checksState: String

        var pr: RepoPage.Pr {
            RepoPage.Pr(id: id, number: number, title: title, head: head, base: base, state: state, checksState: checksState)
        }
    }

    private struct RowJSON: Decodable {
        let id: String
        let number: Int
        let identifier: String
        let title: String
        let depth: Int
        let subtitle: String
        let draft: Bool
        let checks: String?

        var row: RepoPage.PrRow {
            RepoPage.PrRow(
                id: id, number: number, identifier: identifier, title: title, depth: depth,
                subtitle: subtitle, draft: draft, checks: checks
            )
        }
    }

    private struct TreeCase: Decodable { let name: String; let prs: [PrJSON]; let expected: [RowJSON] }

    private struct BranchJSON: Decodable { let name: String; let sha: String }

    private struct BranchRowJSON: Decodable {
        let name: String
        let sha: String
        let isDefault: Bool
        let prNumber: Int?
    }

    private struct BranchCase: Decodable {
        let name: String
        let branches: [BranchJSON]
        let defaultBranch: String
        let prs: [PrJSON]
        let expected: [BranchRowJSON]
    }

    private struct RollupCase: Decodable { let name: String; let prs: [PrJSON]; let expected: String? }

    private struct TabCase: Decodable { let param: String?; let expected: String }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/repo-page.json")
        return try JSONDecoder().decode(Fixture.self, from: Data(contentsOf: url))
    }

    func testTreeCases() throws {
        let cases = try fixture().treeCases
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            XCTAssertEqual(RepoPage.prTree(entry.prs.map(\.pr)), entry.expected.map(\.row), entry.name)
        }
    }

    func testBranchCases() throws {
        let cases = try fixture().branchCases
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            let rows = RepoPage.branches(
                entry.branches.map { (name: $0.name, sha: $0.sha) },
                defaultBranch: entry.defaultBranch,
                prs: entry.prs.map(\.pr)
            )
            let expected = entry.expected.map {
                RepoPage.BranchRow(name: $0.name, sha: $0.sha, isDefault: $0.isDefault, prNumber: $0.prNumber)
            }
            XCTAssertEqual(rows, expected, entry.name)
        }
    }

    func testRollupCases() throws {
        let cases = try fixture().rollupCases
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            XCTAssertEqual(RepoPage.checksRollup(entry.prs.map(\.pr)), entry.expected, entry.name)
        }
    }

    func testTabCases() throws {
        let cases = try fixture().tabCases
        XCTAssertFalse(cases.isEmpty)
        for entry in cases {
            XCTAssertEqual(RepoPage.tab(entry.param).rawValue, entry.expected, String(describing: entry.param))
        }
    }

    func testTabsAndCopy() throws {
        let f = try fixture()
        XCTAssertEqual(f.tabs.map(\.id), RepoPage.tabs.map(\.rawValue))
        XCTAssertEqual(f.tabs.map(\.label), RepoPage.tabs.map(\.label))
        XCTAssertEqual(f.copy.provider, ["github": RepoPage.Copy.Provider.github, "ecode": RepoPage.Copy.Provider.ecode])
        XCTAssertEqual(f.copy.add, [
            "title": RepoPage.Copy.Add.title,
            "github": RepoPage.Copy.Add.github,
            "ecode": RepoPage.Copy.Add.ecode,
            "name": RepoPage.Copy.Add.name,
            "defaultBranch": RepoPage.Copy.Add.defaultBranch,
            "mirror": RepoPage.Copy.Add.mirror,
            "mirrorUrl": RepoPage.Copy.Add.mirrorUrl,
            "cancel": RepoPage.Copy.Add.cancel,
            "create": RepoPage.Copy.Add.create,
        ])
        XCTAssertEqual(f.copy.pulls, ["empty": RepoPage.Copy.Pulls.empty])
        XCTAssertEqual(f.copy.runs, ["empty": RepoPage.Copy.Runs.empty])
        XCTAssertEqual(f.copy.branches, ["empty": RepoPage.Copy.Branches.empty, "default": RepoPage.Copy.Branches.default])
        XCTAssertEqual(f.copy.settings, [
            "provider": RepoPage.Copy.Settings.provider,
            "defaultBranch": RepoPage.Copy.Settings.defaultBranch,
            "cloneUrl": RepoPage.Copy.Settings.cloneUrl,
            "backups": RepoPage.Copy.Settings.backups,
            "noBackups": RepoPage.Copy.Settings.noBackups,
            "restore": RepoPage.Copy.Settings.restore,
            "restoreTitle": RepoPage.Copy.Settings.restoreTitle,
            "restoreConfirm": RepoPage.Copy.Settings.restoreConfirm,
            "mirror": RepoPage.Copy.Settings.mirror,
            "mirrorUrl": RepoPage.Copy.Settings.mirrorUrl,
        ])
        for (state, check) in f.copy.checks {
            let mine = try XCTUnwrap(RepoPage.Copy.checks(state), state)
            XCTAssertEqual(mine.label, check.label, state)
            XCTAssertEqual(mine.tone.rawValue, check.tone, state)
        }
        XCTAssertEqual(f.copy.checks.count, 3)
        XCTAssertNil(RepoPage.Copy.checks("none"))
    }
}
