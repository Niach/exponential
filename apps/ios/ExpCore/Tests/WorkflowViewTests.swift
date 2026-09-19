import Foundation
import XCTest
@testable import ExpCore

// EXP-981: what every client SAYS about a workflow, locked ×4 (web
// `workflow-view.test.ts`, Android `WorkflowViewTest`, desktop
// `domain::workflow_view`) against the ONE contract fixture — same cases, same
// strings, byte for byte.
final class WorkflowViewTests: XCTestCase {
    private struct Fixture: Decodable {
        let bands: [BandCase]
        let shapeLines: [ShapeCase]
        let captions: [CaptionCase]
        let titles: [TitleCase]
        let edges: [EdgeCase]
    }

    private struct BandCase: Decodable {
        let status: String
        let band: String
    }

    private struct ShapeCase: Decodable {
        let metrics: WorkflowMetrics
        let line: String
        let cycleNote: String?
    }

    private struct CaptionCase: Decodable {
        let node: FixtureNode
        let workflowStatus: String
        let caption: String
        let tone: String
    }

    private struct FixtureNode: Decodable {
        let kind: String
        let state: String
        let risk: String
    }

    private struct TitleCase: Decodable {
        let identifier: String
        let members: Int
        let title: String
    }

    private struct EdgeCase: Decodable {
        let name: String
        let nodes: [FixtureEdgeNode]
        let relations: [FixtureRelation]
        let cycleEdges: [String]
        let expected: [FixtureEdge]
    }

    private struct FixtureEdgeNode: Decodable {
        let id: String
        let issueId: String
        let memberIssueIds: [String]
    }

    private struct FixtureRelation: Decodable {
        let type: String
        let issueId: String
        let relatedIssueId: String
    }

    private struct FixtureEdge: Decodable, Equatable {
        let from: String
        let to: String
        let cycle: Bool
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
            .appendingPathComponent("packages/domain-contract/fixtures/workflow-view.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    func testContractFixtureCases() throws {
        let fixture = try fixture()

        XCTAssertFalse(fixture.bands.isEmpty)
        for testCase in fixture.bands {
            XCTAssertEqual(
                WorkflowView.band(testCase.status).rawValue, testCase.band, testCase.status
            )
        }

        for testCase in fixture.shapeLines {
            XCTAssertEqual(WorkflowView.shapeLine(testCase.metrics), testCase.line)
            XCTAssertEqual(WorkflowView.cycleNote(testCase.metrics), testCase.cycleNote)
        }

        for testCase in fixture.captions {
            let node = WorkflowView.CaptionNode(
                kind: testCase.node.kind, state: testCase.node.state, risk: testCase.node.risk
            )
            XCTAssertEqual(
                WorkflowView.nodeCaption(node, workflowStatus: testCase.workflowStatus),
                testCase.caption
            )
            XCTAssertEqual(
                WorkflowView.nodeTone(testCase.node.state).rawValue, testCase.tone
            )
        }

        for testCase in fixture.titles {
            XCTAssertEqual(
                WorkflowView.nodeTitle(
                    identifier: testCase.identifier, memberCount: testCase.members
                ),
                testCase.title
            )
        }

        for testCase in fixture.edges {
            let actual = WorkflowView.edges(
                nodes: testCase.nodes.map {
                    WorkflowView.EdgeNode(
                        id: $0.id, issueId: $0.issueId, memberIssueIds: $0.memberIssueIds
                    )
                },
                relations: testCase.relations.map {
                    WorkflowView.EdgeRelation(
                        type: $0.type, issueId: $0.issueId, relatedIssueId: $0.relatedIssueId
                    )
                },
                cycleEdges: testCase.cycleEdges
            )
            XCTAssertEqual(
                actual.map { FixtureEdge(from: $0.from, to: $0.to, cycle: $0.cycle) },
                testCase.expected,
                testCase.name
            )
        }
    }

    // MARK: - Byte-locked copy

    func testTheListsCopyIsByteLocked() {
        XCTAssertEqual(WorkflowView.title, "Workflows")
        XCTAssertEqual(WorkflowView.emptyTitle, "No workflows yet")
        XCTAssertEqual(
            WorkflowView.emptyBody,
            "Select backlog issues on a board and choose Create workflow to plan them as one parallel run."
        )
        XCTAssertEqual(WorkflowView.planLabel, "Plan")
        XCTAssertEqual(WorkflowView.deleteLabel, "Delete workflow")
        XCTAssertEqual(WorkflowView.bands.map(\.title), ["Running", "Draft", "Done"])
        XCTAssertEqual(WorkflowView.bands.map(\.key), [.running, .draft, .done])
    }

    // The bulk bar's play menu, in order (EXP-981).
    func testThePlayMenusLabelsAreByteLocked() {
        XCTAssertEqual(WorkflowView.startAsBatchLabel, "Start as batch")
        XCTAssertEqual(WorkflowView.startAsStackLabel, "Start as stack")
        XCTAssertEqual(WorkflowView.createWorkflowLabel, "Create workflow…")
    }

    // Every contract value has a label; an unknown one falls through as itself
    // rather than rendering blank.
    func testEveryContractStateAndKindHasALabel() {
        for state in DomainContract.wfNodeStateValues {
            XCTAssertFalse(WorkflowView.nodeStateLabel(state).isEmpty, state)
            XCTAssertNotEqual(WorkflowView.nodeStateLabel(state), state, state)
        }
        for kind in DomainContract.wfNodeKindValues {
            XCTAssertNotEqual(WorkflowView.nodeKindLabel(kind), kind, kind)
        }
        XCTAssertEqual(WorkflowView.nodeStateLabel("brand-new"), "brand-new")
        XCTAssertEqual(WorkflowView.nodeKindLabel("brand-new"), "brand-new")
    }

    // MARK: - The synced rows go through the same rule

    // The entity overload is what every surface actually calls: a `parent`
    // relation is not an edge, and a relation between two members of the SAME
    // compound node collapses to nothing.
    func testTheEntityOverloadReadsTheSyncedRows() {
        let nodes = [
            makeWorkflowNode(id: "n1", issueId: "p", memberIssueIds: ["k1"]),
            makeWorkflowNode(id: "n2", issueId: "x"),
        ]
        let relations = [
            makeWorkflowRelation(from: "k1", to: "x", type: "blocks"),
            makeWorkflowRelation(from: "p", to: "k1", type: "blocks"),
            makeWorkflowRelation(from: "x", to: "p", type: "parent"),
        ]
        let edges = WorkflowView.edges(nodes: nodes, relations: relations)
        XCTAssertEqual(edges.count, 1)
        XCTAssertEqual(edges.first, WorkflowView.Edge(from: "n1", to: "n2", cycle: false))
    }

    // MARK: - Tolerant jsonb

    // Unknown keys are ignored and missing ones default: a `launch` or
    // `metrics` object a newer server wrote must degrade a field, never drop
    // the workflow.
    func testTheJsonbPayloadsParseTolerantly() {
        let launch = WorkflowLaunch.parse(
            #"{"agent":"claude","subagentModel":"sonnet","maxParallel":5,"future":"x"}"#
        )
        XCTAssertEqual(launch.agent, "claude")
        XCTAssertEqual(launch.subagentModel, "sonnet")
        XCTAssertEqual(launch.maxParallel, 5)
        XCTAssertNil(launch.model)
        XCTAssertEqual(WorkflowLaunch.parse(nil), WorkflowLaunch())
        XCTAssertEqual(WorkflowLaunch.parse("not json"), WorkflowLaunch())

        let metrics = WorkflowMetrics.parse(#"{"nodes":4,"depth":2,"width":2,"cycles":[["EXP-2"]]}"#)
        XCTAssertEqual(metrics.nodes, 4)
        XCTAssertEqual(metrics.edges, 0)
        XCTAssertEqual(metrics.cycles, [["EXP-2"]])
        XCTAssertEqual(metrics.cycleEdges, [])
        XCTAssertEqual(WorkflowMetrics.parse(nil), WorkflowMetrics())

        XCTAssertNil(WorkflowNodeBudget.parse(nil))
        XCTAssertNil(WorkflowNodeBudget.parse("{}"))
        XCTAssertEqual(WorkflowNodeBudget.parse(#"{"minutes":30}"#)?.minutes, 30)
    }
}

// MARK: - Row builders

private func makeWorkflowNode(
    id: String, issueId: String, memberIssueIds: [String] = []
) -> WorkflowNodeEntity {
    WorkflowNodeEntity(
        id: id, workflowId: "wf-1", teamId: "t1", issueId: issueId,
        memberIssueIds: memberIssueIds,
        createdAt: "2026-09-19T09:00:00Z", updatedAt: "2026-09-19T09:00:00Z"
    )
}

private func makeWorkflowRelation(
    from: String, to: String, type: String
) -> IssueRelationEntity {
    IssueRelationEntity(
        id: "\(type)-\(from)-\(to)", issueId: from, relatedIssueId: to, type: type,
        source: "user", teamId: "t1", boardId: "b1",
        createdAt: "2026-09-19T09:00:00Z", updatedAt: "2026-09-19T09:00:00Z"
    )
}
