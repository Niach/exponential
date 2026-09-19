import Foundation
import XCTest
@testable import ExpCore

// EXP-980: the blocks-graph rule, locked ×4 (web `issue-graph.test.ts`,
// Android `IssueGraphTest`, desktop `domain::issue_graph`) against the ONE
// contract fixture — same cases, same test names.
final class IssueGraphTests: XCTestCase {
    private struct FixtureCase: Decodable {
        let name: String
        let issues: [FixtureIssue]
        let relations: [FixtureRelation]
        let expectedCounts: [String: FixtureCounts]?
        let picked: [String]?
        let expectedSetBlockers: [String]?
        let subjects: [String]?
        let expectedGraph: FixtureGraph?
    }

    private struct FixtureIssue: Decodable {
        let id: String
        let identifier: String
        let status: String

        var issue: IssueEntity { makeIssue(id: id, identifier: identifier, status: status) }
    }

    private struct FixtureRelation: Decodable {
        let type: String
        let issueId: String
        let relatedIssueId: String

        var relation: IssueRelationEntity {
            makeRelation(from: issueId, to: relatedIssueId, type: type)
        }
    }

    private struct FixtureCounts: Decodable, Equatable {
        let blockedBy: Int
        let blocking: Int
    }

    private struct FixtureNode: Decodable, Equatable {
        let id: String
        let wave: Int
        let lane: Int
        let subject: Bool
    }

    private struct FixtureEdge: Decodable, Equatable {
        let from: String
        let to: String
        let cycle: Bool
    }

    private struct FixtureGraph: Decodable, Equatable {
        let nodes: [FixtureNode]
        let edges: [FixtureEdge]
        let hasCycle: Bool
        let truncated: Bool
    }

    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func cases() throws -> [FixtureCase] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/issue-graph.json")
        return try JSONDecoder().decode([FixtureCase].self, from: try Data(contentsOf: url))
    }

    private func flatten(_ graph: IssueGraph.Graph) -> FixtureGraph {
        FixtureGraph(
            nodes: graph.nodes.map {
                FixtureNode(id: $0.id, wave: $0.wave, lane: $0.lane, subject: $0.subject)
            },
            edges: graph.edges.map { FixtureEdge(from: $0.from, to: $0.to, cycle: $0.cycle) },
            hasCycle: graph.hasCycle,
            truncated: graph.truncated
        )
    }

    func testContractFixtureCases() throws {
        let cases = try cases()
        XCTAssertFalse(cases.isEmpty, "the issue-graph fixture must not be empty")
        for testCase in cases {
            let issues = testCase.issues.map(\.issue)
            let relations = testCase.relations.map(\.relation)
            if let expected = testCase.expectedCounts {
                let actual = IssueGraph.blockCounts(relations: relations, issues: issues)
                    .mapValues { FixtureCounts(blockedBy: $0.blockedBy, blocking: $0.blocking) }
                XCTAssertEqual(actual, expected, testCase.name)
            }
            if let expected = testCase.expectedSetBlockers {
                let actual = IssueGraph.openBlockersOfSet(
                    ids: testCase.picked ?? [], relations: relations, issues: issues
                ).map(\.id)
                XCTAssertEqual(actual, expected, testCase.name)
            }
            if let expected = testCase.expectedGraph {
                let actual = IssueGraph.blockGraph(
                    subjectIds: testCase.subjects ?? [], relations: relations, issues: issues
                )
                XCTAssertEqual(flatten(actual), expected, testCase.name)
            }
        }
    }

    // MARK: - blocks badge label

    func testNamesTheSideThatHasACount() {
        XCTAssertEqual(
            IssueGraph.blocksBadgeLabel(IssueGraph.BlockCounts(blockedBy: 2, blocking: 0)),
            "Blocked by 2"
        )
        XCTAssertEqual(
            IssueGraph.blocksBadgeLabel(IssueGraph.BlockCounts(blockedBy: 0, blocking: 1)),
            "Blocking 1"
        )
        XCTAssertEqual(
            IssueGraph.blocksBadgeLabel(IssueGraph.BlockCounts(blockedBy: 2, blocking: 1)),
            "Blocked by 2, blocking 1"
        )
    }

    // MARK: - node cap

    func testCutsTheClosureAtTheNodeCapNearestBlockersFirst() {
        let total = IssueGraph.maxNodes + 5
        let issues = (0..<total).map { index in
            makeIssue(id: "n\(index)", identifier: "EXP-\(1000 + index)", status: "backlog")
        }
        // A chain n(total-1) → … → n1 → n0; the subject is the most blocked end.
        let relations = (1..<total).map { index in
            makeRelation(from: "n\(index)", to: "n\(index - 1)", type: "blocks")
        }
        let graph = IssueGraph.blockGraph(
            subjectIds: ["n0"], relations: relations, issues: issues
        )
        XCTAssertTrue(graph.truncated)
        XCTAssertEqual(graph.nodes.count, IssueGraph.maxNodes)
        XCTAssertEqual(
            graph.nodes.last,
            IssueGraph.Node(
                id: "n0", wave: IssueGraph.maxNodes - 1, lane: 0, subject: true
            )
        )
    }

    // The notes under a graph are byte-locked copy, like the dialog's.
    func testNotesAreByteLocked() {
        XCTAssertEqual(IssueGraph.maxNodes, 60)
        XCTAssertEqual(IssueGraph.truncatedNote, "Showing the nearest 60 issues.")
        XCTAssertEqual(IssueGraph.cycleNote, "Red issues block each other in a cycle.")
    }
}

// MARK: - Row builders

private func makeIssue(id: String, identifier: String, status: String) -> IssueEntity {
    IssueEntity(
        id: id, boardId: "b1", number: 1, identifier: identifier, title: "T \(identifier)",
        description: nil, status: status, priority: "none", assigneeId: nil, creatorId: nil,
        source: nil, dueDate: nil, sortOrder: 1, completedAt: nil, duplicateOfId: nil,
        prUrl: nil, prNumber: nil, prState: nil, branch: nil, prMergedAt: nil,
        createdAt: "2026-09-19T09:00:00Z", updatedAt: "2026-09-19T09:00:00Z"
    )
}

private func makeRelation(from: String, to: String, type: String) -> IssueRelationEntity {
    IssueRelationEntity(
        id: "\(type)-\(from)-\(to)", issueId: from, relatedIssueId: to, type: type,
        source: "user", teamId: "t1", boardId: "b1",
        createdAt: "2026-09-19T09:00:00Z", updatedAt: "2026-09-19T09:00:00Z"
    )
}
