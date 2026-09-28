import Foundation
import XCTest
@testable import ExpCore

// EXP-1097: the relations view model + its copy, fixture-locked ×4 (web
// `issue-relations-view.test.ts`, Android `IssueRelationsViewTest`, desktop
// `domain::relations_view`) against the ONE contract fixture — same copy
// table, same named cases. Change one, change all four.
final class IssueRelationsViewTests: XCTestCase {
    private struct Fixture: Decodable {
        let copy: [String: String]
        let cases: [FixtureCase]
    }

    private struct FixtureCase: Decodable {
        let name: String
        let input: FixtureInput
        let expected: FixtureModel
    }

    private struct FixtureIssue: Decodable {
        let id: String
        let identifier: String
        let title: String
        let status: String
    }

    private struct FixtureRelation: Decodable {
        let type: String
        let issueId: String
        let relatedIssueId: String
    }

    private struct FixtureInput: Decodable {
        let subjectId: String
        let issues: [FixtureIssue]
        let relations: [FixtureRelation]
        let toggled: [String]
        let showAll: [String]

        var input: IssueRelationsView.Input {
            IssueRelationsView.Input(
                subjectId: subjectId,
                relations: relations.map {
                    IssueRelationsView.Relation(
                        type: $0.type, issueId: $0.issueId, relatedIssueId: $0.relatedIssueId
                    )
                },
                issues: issues.map {
                    IssueRelationsView.Issue(
                        id: $0.id, identifier: $0.identifier, title: $0.title, status: $0.status
                    )
                },
                toggled: Set(toggled.compactMap(IssueRelationsView.BandKey.init(rawValue:))),
                showAll: Set(showAll.compactMap(IssueRelationsView.BandKey.init(rawValue:)))
            )
        }
    }

    private struct FixtureRow: Codable, Equatable {
        let id: String
        let identifier: String
        let title: String
        let status: String
        let open: Bool

        init(_ row: IssueRelationsView.Row) {
            id = row.id
            identifier = row.identifier
            title = row.title
            status = row.status
            open = row.open
        }
    }

    private struct FixtureBand: Codable, Equatable {
        let key: String
        let title: String
        let count: Int
        let openCount: Int
        let expanded: Bool
        let rows: [FixtureRow]
        let more: String?
        let less: String?

        init(_ band: IssueRelationsView.Band) {
            key = band.key.rawValue
            title = band.title
            count = band.count
            openCount = band.openCount
            expanded = band.expanded
            rows = band.rows.map(FixtureRow.init)
            more = band.more
            less = band.less
        }
    }

    private struct FixtureSubIssues: Codable, Equatable {
        let rows: [FixtureRow]
        let done: Int
        let total: Int
        let progress: String?
    }

    private struct FixtureModel: Codable, Equatable {
        let parent: FixtureRow?
        let subIssues: FixtureSubIssues
        let bands: [FixtureBand]
        let relationCount: Int

        init(_ model: IssueRelationsView.Model) {
            parent = model.parent.map(FixtureRow.init)
            subIssues = FixtureSubIssues(
                rows: model.subIssues.rows.map(FixtureRow.init),
                done: model.subIssues.done,
                total: model.subIssues.total,
                progress: model.subIssues.progress
            )
            bands = model.bands.map(FixtureBand.init)
            relationCount = model.relationCount
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
            .appendingPathComponent("packages/domain-contract/fixtures/issue-relations-view.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testLocksTheCopyTable() throws {
        var derived = IssueRelationsView.Copy.table
        derived["showMore(4)"] = IssueRelationsView.showMore(4)
        derived["progress(2,5)"] = IssueRelationsView.progress(2, 5)
        derived["cap"] = String(IssueRelationsView.bandCap)
        XCTAssertEqual(derived, try fixture().copy)
    }

    func testEveryBandKeyIsKnown() throws {
        for testCase in try fixture().cases {
            for key in testCase.input.toggled + testCase.input.showAll {
                XCTAssertNotNil(IssueRelationsView.BandKey(rawValue: key), "\(testCase.name): \(key)")
            }
        }
    }

    func testFixtureCases() throws {
        let cases = try fixture().cases
        XCTAssertFalse(cases.isEmpty)
        for testCase in cases {
            let actual = FixtureModel(IssueRelationsView.build(testCase.input.input))
            XCTAssertEqual(actual, testCase.expected, testCase.name)
        }
    }
}
