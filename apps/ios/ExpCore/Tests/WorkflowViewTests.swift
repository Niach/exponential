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
        // EXP-982 — running a workflow.
        let startBlockers: [StartBlockerCase]
        let trains: [TrainCase]
        let trainStepLabels: [String: String]
        let finalPr: [FinalPrCase]
        let rowSubtitles: [RowSubtitleCase]
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

    private struct StartBlockerCase: Decodable {
        let workflow: FixtureStartable
        let metrics: WorkflowMetrics
        let blocker: String?
    }

    private struct FixtureStartable: Decodable {
        let status: String
        let deviceId: String?
        let repositoryId: String?
        let startOn: String
    }

    private struct TrainCase: Decodable {
        let name: String
        let gate: String
        let nodes: [FixtureTrainNode]
        let expected: [FixtureTrainEntry]
    }

    private struct FixtureTrainNode: Decodable {
        let id: String
        let kind: String
        let state: String
        let wave: Int
        let lane: Int
        let approvedAt: String?
    }

    private struct FixtureTrainEntry: Decodable, Equatable {
        let id: String
        let step: String
    }

    private struct FinalPrCase: Decodable {
        let states: [String]
        let finalPrState: String?
        let finalPrNumber: Int?
        let caption: String?
    }

    private struct RowSubtitleCase: Decodable {
        let status: String
        let metrics: WorkflowMetrics
        let subtitle: String
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

    // MARK: - Running a workflow (EXP-982)

    func testRunningTheWorkflowFixtureCases() throws {
        let fixture = try fixture()

        XCTAssertFalse(fixture.startBlockers.isEmpty)
        for testCase in fixture.startBlockers {
            let workflow = WorkflowView.StartableWorkflow(
                status: testCase.workflow.status,
                deviceId: testCase.workflow.deviceId,
                repositoryId: testCase.workflow.repositoryId,
                startOn: testCase.workflow.startOn
            )
            XCTAssertEqual(
                WorkflowView.startBlocker(workflow, metrics: testCase.metrics),
                testCase.blocker,
                testCase.workflow.status
            )
        }

        for testCase in fixture.trains {
            let actual = WorkflowView.mergeTrain(
                testCase.nodes.map {
                    WorkflowView.TrainNode(
                        id: $0.id, kind: $0.kind, state: $0.state,
                        wave: $0.wave, lane: $0.lane, approvedAt: $0.approvedAt
                    )
                },
                gate: testCase.gate
            )
            XCTAssertEqual(
                actual.map { FixtureTrainEntry(id: $0.id, step: $0.step.rawValue) },
                testCase.expected,
                testCase.name
            )
        }

        for (step, label) in fixture.trainStepLabels {
            let parsed = WorkflowView.TrainStep(rawValue: step)
            XCTAssertNotNil(parsed, step)
            XCTAssertEqual(parsed.map(WorkflowView.trainStepLabel), label, step)
        }

        for testCase in fixture.finalPr {
            XCTAssertEqual(
                WorkflowView.finalPrCaption(
                    states: testCase.states,
                    finalPrState: testCase.finalPrState,
                    finalPrNumber: testCase.finalPrNumber
                ),
                testCase.caption,
                testCase.states.joined(separator: ",")
            )
        }

        for testCase in fixture.rowSubtitles {
            XCTAssertEqual(
                WorkflowView.rowSubtitle(status: testCase.status, metrics: testCase.metrics),
                testCase.subtitle,
                testCase.status
            )
        }
    }

    // The run controls' copy, byte for byte — the confirms are the sentences
    // the destructive dialogs show.
    func testTheRunControlsCopyIsByteLocked() {
        XCTAssertEqual(WorkflowView.startLabel, "Start")
        XCTAssertEqual(WorkflowView.pauseLabel, "Pause")
        XCTAssertEqual(WorkflowView.resumeLabel, "Resume")
        XCTAssertEqual(WorkflowView.cancelLabel, "Cancel workflow")
        XCTAssertEqual(
            WorkflowView.cancelConfirm,
            "Its live runs end and its branch is deleted. Nothing reached the default branch."
        )
        XCTAssertEqual(WorkflowView.approveNodeLabel, "Approve and land")
        XCTAssertEqual(WorkflowView.withdrawApprovalLabel, "Withdraw approval")
        XCTAssertEqual(WorkflowView.mergeTrainTitle, "Merge train")
        XCTAssertEqual(WorkflowView.mergeTrainEmpty, "Nothing is waiting to land.")
        XCTAssertEqual(WorkflowView.finalPrTitle, "Final pull request")
        XCTAssertEqual(WorkflowView.retryNodeLabel, "Retry")
        XCTAssertEqual(WorkflowView.skipNodeLabel, "Skip")
        XCTAssertEqual(
            WorkflowView.skipNodeConfirm,
            "Its dependents go on without it. The node's work is not part of the final pull request."
        )
    }

    // The gate rule the merge train and the node panel share: the contract is
    // always human-gated, every other node follows the workflow's gate.
    func testTheContractNodeAlwaysNeedsAPerson() {
        for gate in DomainContract.wfGateValues {
            XCTAssertTrue(
                WorkflowView.nodeNeedsApproval(
                    gate: gate, kind: DomainContract.wfNodeKindContract
                ),
                gate
            )
        }
        XCTAssertFalse(
            WorkflowView.nodeNeedsApproval(
                gate: DomainContract.wfGateNone, kind: DomainContract.wfNodeKindLeaf
            )
        )
        XCTAssertTrue(
            WorkflowView.nodeNeedsApproval(
                gate: DomainContract.wfGateAgent, kind: DomainContract.wfNodeKindLeaf
            )
        )
    }

    // The two entity overloads the run surfaces actually call.
    func testTheRunOverloadsReadTheSyncedRows() {
        let approved = makeWorkflowNode(
            id: "a", issueId: "i1", approvedAt: "2026-09-19T09:00:00Z"
        )
        let waiting = makeWorkflowNode(id: "b", issueId: "i2", lane: 1)
        let train = WorkflowView.mergeTrain(
            [waiting, approved], gate: DomainContract.wfGateHuman
        )
        XCTAssertEqual(
            train.map(\.step), [WorkflowView.TrainStep.next, .needsApproval]
        )

        let workflow = makeWorkflow(status: DomainContract.wfStatusDraft)
        XCTAssertEqual(
            WorkflowView.startBlocker(workflow),
            "Only \"When landed\" starts are available yet."
        )
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
    id: String,
    issueId: String,
    memberIssueIds: [String] = [],
    state: String = "in_review",
    lane: Int = 0,
    approvedAt: String? = nil
) -> WorkflowNodeEntity {
    WorkflowNodeEntity(
        id: id, workflowId: "wf-1", teamId: "t1", issueId: issueId,
        memberIssueIds: memberIssueIds, state: state, lane: lane,
        approvedAt: approvedAt,
        createdAt: "2026-09-19T09:00:00Z", updatedAt: "2026-09-19T09:00:00Z"
    )
}

private func makeWorkflow(status: String) -> WorkflowEntity {
    WorkflowEntity(
        id: "wf-1", teamId: "t1", repositoryId: "repo-1", name: "Ship it",
        status: status, deviceId: "dev-1",
        metrics: #"{"nodes":3,"depth":2,"width":2}"#,
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
