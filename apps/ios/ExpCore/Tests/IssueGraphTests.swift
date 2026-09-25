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

// EXP-1057: THE mini-graph geometry, locked ×4 (web `issue-graph.test.ts`,
// desktop `domain::issue_graph::geometry`, Android `IssueGraphGeometryTest`)
// against `issue-graph-geometry.json` — one test per named fixture case.
final class IssueGraphGeometryTests: XCTestCase {
    private typealias G = IssueGraph.Geometry

    private struct Fixture: Decodable {
        let constants: [String: Double]
        let sizes: [SizeCase]
        let origins: [OriginCase]
        let edges: [EdgeCase]
    }

    private struct SizeCase: Decodable {
        let name: String
        let waves: Int
        let lanes: Int
        let width: Double
        let height: Double
        let viewWidth: Double
        let viewHeight: Double
    }

    private struct OriginCase: Decodable {
        let wave: Int
        let lane: Int
        let x: Double
        let y: Double
    }

    private struct Cell: Decodable {
        let wave: Int
        let lane: Int
    }

    private struct Point: Decodable {
        let x: Double
        let y: Double

        var point: IssueGraph.Geometry.Point { .init(x: x, y: y) }
    }

    private struct EdgeCase: Decodable {
        let name: String
        let from: Cell
        let to: Cell
        let start: Point
        let control1: Point
        let control2: Point
        let end: Point
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent(
                "packages/domain-contract/fixtures/issue-graph-geometry.json"
            )
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testConstants() throws {
        let constants = try fixture().constants
        let mirrored: [String: Double] = [
            "nodeWidth": G.nodeWidth,
            "nodeHeight": G.nodeHeight,
            "waveGap": G.waveGap,
            "laneGap": G.laneGap,
            "inset": G.inset,
            "maxViewWidth": G.maxViewWidth,
            "maxViewHeight": G.maxViewHeight,
            "edgeStroke": G.edgeStroke,
            "ringWidth": G.ringWidth,
            "nodeRadius": G.nodeRadius,
            "railGutter": G.railGutter,
            "railNodeWidth": G.railNodeWidth,
            "railDot": G.railDot,
            "railDotRing": G.railDotRing,
        ]
        XCTAssertEqual(mirrored, constants)
    }

    private func assertSize(_ name: String) throws {
        let fixture = try fixture()
        let testCase = try XCTUnwrap(fixture.sizes.first { $0.name == name }, name)
        XCTAssertEqual(
            G.size(waves: testCase.waves, lanes: testCase.lanes),
            G.Size(
                width: testCase.width,
                height: testCase.height,
                viewWidth: testCase.viewWidth,
                viewHeight: testCase.viewHeight
            ),
            name
        )
    }

    func testNoNodeDrawsNothing() throws { try assertSize("no node draws nothing") }
    func testOneNode() throws { try assertSize("one node") }
    func testTwoWavesFit() throws { try assertSize("two waves fit") }
    func testThreeWavesScrollSideways() throws { try assertSize("three waves scroll sideways") }
    func testTenLanesScrollDown() throws { try assertSize("ten lanes scroll down") }

    func testOrigins() throws {
        let origins = try fixture().origins
        XCTAssertFalse(origins.isEmpty)
        for origin in origins {
            XCTAssertEqual(
                G.origin(wave: origin.wave, lane: origin.lane),
                G.Point(x: origin.x, y: origin.y),
                "wave \(origin.wave) lane \(origin.lane)"
            )
        }
    }

    private func assertEdge(_ name: String) throws {
        let fixture = try fixture()
        let testCase = try XCTUnwrap(fixture.edges.first { $0.name == name }, name)
        XCTAssertEqual(
            G.edge(
                from: (testCase.from.wave, testCase.from.lane),
                to: (testCase.to.wave, testCase.to.lane)
            ),
            G.EdgeCurve(
                start: testCase.start.point,
                control1: testCase.control1.point,
                control2: testCase.control2.point,
                end: testCase.end.point
            ),
            name
        )
    }

    func testAForwardEdgeBendsOnTheGapsMiddle() throws {
        try assertEdge("a forward edge bends on the gap's middle")
    }

    func testASkippingEdgeStaysLevel() throws {
        try assertEdge("a skipping edge stays level")
    }

    func testABackwardCycleEdgeBowsByHalfItsRun() throws {
        try assertEdge("a backward cycle edge bows by half its run")
    }

    /// Every named case above exists in the fixture, and nothing new slipped in
    /// untested.
    func testEveryFixtureCaseIsCovered() throws {
        let fixture = try fixture()
        XCTAssertEqual(
            fixture.sizes.map(\.name),
            [
                "no node draws nothing", "one node", "two waves fit",
                "three waves scroll sideways", "ten lanes scroll down",
            ]
        )
        XCTAssertEqual(
            fixture.edges.map(\.name),
            [
                "a forward edge bends on the gap's middle",
                "a skipping edge stays level",
                "a backward cycle edge bows by half its run",
            ]
        )
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
