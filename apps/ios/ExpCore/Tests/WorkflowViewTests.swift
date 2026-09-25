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
        // EXP-983 — speculative starts.
        let edgeStyles: [EdgeStyleCase]
        // EXP-984 — the agent review gate and the run's counters.
        let reviewLines: [ReviewLineCase]
        let metricRows: [MetricRowCase]
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
        /// EXP-983 — absent on a node the engine never serialized.
        let afterNodeIds: [String]?
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
        let serial: Bool
    }

    private struct EdgeStyleCase: Decodable {
        let edge: FixtureStyledEdge
        let fromState: String
        let toState: String
        let style: String
    }

    private struct FixtureStyledEdge: Decodable {
        let cycle: Bool
        let serial: Bool
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

    private struct ReviewLineCase: Decodable {
        let review: WorkflowNodeReview
        let approved: Bool
        let line: String
    }

    private struct MetricRowCase: Decodable {
        let metrics: WorkflowMetrics
        let rows: [FixtureMetricRow]
    }

    private struct FixtureMetricRow: Decodable, Equatable {
        let label: String
        let value: String
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
                        id: $0.id, issueId: $0.issueId, memberIssueIds: $0.memberIssueIds,
                        afterNodeIds: $0.afterNodeIds ?? []
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
                actual.map {
                    FixtureEdge(from: $0.from, to: $0.to, cycle: $0.cycle, serial: $0.serial)
                },
                testCase.expected,
                testCase.name
            )
        }
    }

    // MARK: - Speculative starts (EXP-983)

    // How an edge is drawn, over the same fixture the other three read: a
    // cycle beats everything, a landed blocker beats a stale dependent, and a
    // serialization edge is always speculative.
    func testEdgeStyleFixtureCases() throws {
        let fixture = try fixture()

        XCTAssertFalse(fixture.edgeStyles.isEmpty)
        for testCase in fixture.edgeStyles {
            let edge = WorkflowView.Edge(
                from: "a", to: "b", cycle: testCase.edge.cycle, serial: testCase.edge.serial
            )
            XCTAssertEqual(
                WorkflowView.edgeStyle(
                    edge, fromState: testCase.fromState, toState: testCase.toState
                ).rawValue,
                testCase.style,
                "\(testCase.fromState) → \(testCase.toState)"
            )
        }
    }

    func testTheSpeculativeCopyIsByteLocked() {
        XCTAssertEqual(WorkflowView.contractPublishedLabel, "Contract published")
        XCTAssertEqual(WorkflowView.mergesInFirstLabel, "Merges in first")
    }

    // MARK: - Review gate, dynamic graphs, budgets, metrics (EXP-984)

    // The node panel's review line and the detail's Metrics rows, over the same
    // fixture the other three read: a verdict, its round, and what the oracle
    // (or its absence) makes of it; then the counters, in order, each row only
    // when it has something to say.
    func testReviewAndMetricsFixtureCases() throws {
        let fixture = try fixture()

        XCTAssertFalse(fixture.reviewLines.isEmpty)
        for testCase in fixture.reviewLines {
            XCTAssertEqual(
                WorkflowView.reviewLine(testCase.review, nodeApproved: testCase.approved),
                testCase.line,
                "\(testCase.review.verdict) round \(testCase.review.round) approved \(testCase.approved)"
            )
        }

        XCTAssertFalse(fixture.metricRows.isEmpty)
        for testCase in fixture.metricRows {
            XCTAssertEqual(
                WorkflowView.metricRows(testCase.metrics).map {
                    FixtureMetricRow(label: $0.label, value: $0.value)
                },
                testCase.rows
            )
        }
    }

    // The review gate's copy, byte for byte — the note is what a `proposed`
    // node's panel says before a member decides.
    func testTheReviewGateCopyIsByteLocked() {
        XCTAssertEqual(WorkflowView.admitNodeLabel, "Admit")
        XCTAssertEqual(WorkflowView.dismissNodeLabel, "Dismiss")
        XCTAssertEqual(
            WorkflowView.proposedNodeNote,
            "Filed during the run. Admit it into the workflow or dismiss it."
        )
        XCTAssertEqual(WorkflowView.agentReviewTitle, "Agent review")
        XCTAssertEqual(WorkflowView.nodeModelLabel, "Model")
        XCTAssertEqual(WorkflowView.nodeUnsyncedTitle, "Not synced yet")
        XCTAssertEqual(WorkflowView.metricsTitle, "Metrics")
    }

    // MARK: - The strict launch (EXP-1029)

    // The phone CARRIES the stored launch and never edits it, so what it says
    // about a node has to come out of the SAME rule the engine runs: two
    // models, the deprecated pins folded into the strong one, the agent's own
    // contract defaults where the row names nothing.
    func testTheStoredLaunchNormalizesToTwoModels() {
        let empty = WorkflowLaunch().normalized
        XCTAssertEqual(empty.agent, "claude")
        XCTAssertEqual(empty.model, DomainContract.workflowLaunchClaudeModel)
        XCTAssertEqual(empty.strongModel, DomainContract.workflowLaunchClaudeStrongModel)
        XCTAssertNil(empty.account)

        // An agent outside the contract is claude, whatever it says.
        XCTAssertEqual(WorkflowLaunch(agent: "brand-new").normalized.agent, "claude")

        let codex = WorkflowLaunch(agent: "codex").normalized
        XCTAssertEqual(codex.model, DomainContract.workflowLaunchCodexModel)
        XCTAssertEqual(codex.strongModel, DomainContract.workflowLaunchCodexStrongModel)

        // What the row names wins, blanks do not count, and the account rides
        // through.
        let pinned = WorkflowLaunch(
            agent: "claude", model: "sonnet", strongModel: "opus", account: "  "
        ).normalized
        XCTAssertEqual(pinned.model, "sonnet")
        XCTAssertEqual(pinned.strongModel, "opus")
        XCTAssertNil(pinned.account)
        XCTAssertEqual(
            WorkflowLaunch(account: "profile-1").normalized.account, "profile-1"
        )
    }

    // An OLD row carries the EXP-1002 pins and `reviewModel` instead of a
    // strong model: they fold into it, first one set wins, in this order.
    func testTheDeprecatedPinsFoldIntoTheStrongModel() {
        func strong(_ launch: WorkflowLaunch) -> String { launch.normalized.strongModel }

        XCTAssertEqual(
            strong(WorkflowLaunch(
                contractModel: "a", integrationModel: "b", riskModel: "c", reviewModel: "d"
            )),
            "d"
        )
        XCTAssertEqual(
            strong(WorkflowLaunch(contractModel: "a", integrationModel: "b", riskModel: "c")), "c"
        )
        XCTAssertEqual(strong(WorkflowLaunch(contractModel: "a", integrationModel: "b")), "a")
        XCTAssertEqual(strong(WorkflowLaunch(integrationModel: "b")), "b")
        // A stored strong model beats every one of them.
        XCTAssertEqual(
            strong(WorkflowLaunch(strongModel: "fable", contractModel: "a", reviewModel: "d")),
            "fable"
        )
    }

    // Which of the two a NODE takes: the strong one for the two special kinds
    // and for anything high-risk, the cheap one for the rest.
    func testTheNodeModelFollowsItsKindAndRisk() {
        let launch = WorkflowLaunch(agent: "claude", model: "sonnet", strongModel: "opus")
        func model(_ kind: String, _ risk: String) -> String {
            WorkflowView.modelForNode(launch, kind: kind, risk: risk)
        }
        XCTAssertEqual(model("leaf", "low"), "sonnet")
        XCTAssertEqual(model("leaf", "medium"), "sonnet")
        XCTAssertEqual(model("leaf", "high"), "opus")
        XCTAssertEqual(model("contract", "low"), "opus")
        XCTAssertEqual(model("integration", "low"), "opus")
    }

    // A `proposed` node is NOT part of the run: it never holds the final PR up,
    // and a graph of nothing but proposals draws no final-PR node at all.
    func testProposedNodesAreNotPartOfTheRun() {
        XCTAssertEqual(
            WorkflowView.finalPrCaption(
                states: ["landed", "proposed"], finalPrState: nil, finalPrNumber: nil
            ),
            "Opening the pull request"
        )
        XCTAssertNil(
            WorkflowView.finalPrCaption(
                states: ["proposed"], finalPrState: nil, finalPrNumber: nil
            )
        )
        // They are not in the merge train either — nothing proposed has a PR.
        let proposed = makeWorkflowNode(
            id: "a", issueId: "i1", state: DomainContract.wfNodeStateProposed
        )
        XCTAssertTrue(proposed.isProposed)
        XCTAssertTrue(
            WorkflowView.mergeTrain([proposed]).isEmpty
        )
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
                }
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
        XCTAssertEqual(WorkflowView.mergeFinalPrLabel, "Merge")
        XCTAssertEqual(
            WorkflowView.mergeFinalPrConfirm,
            "The workflow's branch is squash-merged into the default branch and the run is done."
        )
        XCTAssertEqual(WorkflowView.runningNowLabel, "Running now")
        XCTAssertEqual(WorkflowView.retryNodeLabel, "Retry")
        XCTAssertEqual(WorkflowView.skipNodeLabel, "Skip")
        XCTAssertEqual(
            WorkflowView.skipNodeConfirm,
            "Its dependents go on without it. The node's work is not part of the final pull request."
        )
    }

    // The two entity overloads the run surfaces actually call.
    func testTheRunOverloadsReadTheSyncedRows() {
        let approved = makeWorkflowNode(
            id: "a", issueId: "i1", approvedAt: "2026-09-19T09:00:00Z"
        )
        let waiting = makeWorkflowNode(id: "b", issueId: "i2", lane: 1)
        let train = WorkflowView.mergeTrain([waiting, approved])
        XCTAssertEqual(
            train.map(\.step), [WorkflowView.TrainStep.next, .needsApproval]
        )

        // EXP-983: every `start_on` starts, so a bound draft has no blocker.
        let workflow = makeWorkflow(status: DomainContract.wfStatusDraft)
        XCTAssertNil(WorkflowView.startBlocker(workflow))
        XCTAssertEqual(
            WorkflowView.startBlocker(makeWorkflow(status: DomainContract.wfStatusRunning)),
            "The workflow has already started."
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

    // EXP-983: the row's `after_node_ids` join the blocks edges as
    // serialization edges — the pair here has no relation, so it is one.
    func testTheEntityOverloadCarriesSerializationEdges() {
        let nodes = [
            makeWorkflowNode(id: "n1", issueId: "a"),
            makeWorkflowNode(id: "n2", issueId: "b", afterNodeIds: ["n1"]),
        ]
        let edges = WorkflowView.edges(nodes: nodes, relations: [])
        XCTAssertEqual(
            edges, [WorkflowView.Edge(from: "n1", to: "n2", cycle: false, serial: true)]
        )
        XCTAssertEqual(
            WorkflowView.edgeStyle(edges[0], fromState: "blocked", toState: "blocked"),
            .speculative
        )
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
    }

    // EXP-1002: the three phase pins decode when present and stay nil when
    // absent or null — a launch an older server wrote is still a launch.
    func testThePhaseModelsDecodeWithAndWithoutTheirKeys() {
        let pinned = WorkflowLaunch.parse(
            #"{"agent":"claude","model":"opus","contractModel":"fable","integrationModel":"sonnet","riskModel":"haiku"}"#
        )
        XCTAssertEqual(pinned.contractModel, "fable")
        XCTAssertEqual(pinned.integrationModel, "sonnet")
        XCTAssertEqual(pinned.riskModel, "haiku")
        XCTAssertEqual(pinned.model, "opus")

        let absent = WorkflowLaunch.parse(#"{"agent":"claude","model":"opus"}"#)
        XCTAssertNil(absent.contractModel)
        XCTAssertNil(absent.integrationModel)
        XCTAssertNil(absent.riskModel)

        let nulled = WorkflowLaunch.parse(
            #"{"contractModel":null,"integrationModel":null,"riskModel":null,"effort":"high"}"#
        )
        XCTAssertNil(nulled.contractModel)
        XCTAssertNil(nulled.integrationModel)
        XCTAssertNil(nulled.riskModel)
        XCTAssertEqual(nulled.effort, "high")
    }

    // The phone never writes a launch back (EXP-1033), but it still READS the
    // one the row carries: a launch has to survive the Codable round trip with
    // the three phase pins an older server may have written intact, since
    // `modelForNode` folds them into the strong model.
    func testALaunchRoundTripsWithItsPhaseModels() throws {
        var launch = WorkflowLaunch.parse(
            #"{"agent":"claude","contractModel":"fable","integrationModel":"sonnet","riskModel":"haiku","maxParallel":2}"#
        )
        launch.effort = "high"
        launch.maxParallel = 4

        let data = try JSONEncoder().encode(launch)
        let decoded = try JSONDecoder().decode(WorkflowLaunch.self, from: data)
        XCTAssertEqual(decoded, launch)
        XCTAssertEqual(decoded.contractModel, "fable")
        XCTAssertEqual(decoded.integrationModel, "sonnet")
        XCTAssertEqual(decoded.riskModel, "haiku")
        XCTAssertEqual(decoded.effort, "high")
        XCTAssertEqual(decoded.maxParallel, 4)
    }

    // EXP-1014/EXP-1033: the wire contract of `workflows.update` as the phone
    // sends it — the NAME and the runner device, nothing else. The workflow
    // screen configures no run any more (binding a device re-seeds the launch
    // server-side), so neither a launch nor the dead `startOn` ever leaves this
    // client; an unset field is omitted, and an unbound runner is the one
    // explicit null.
    func testTheUpdatePayloadCarriesOnlyTheNameAndTheDevice() throws {
        func object(_ patch: WorkflowPatch) throws -> [String: Any] {
            try XCTUnwrap(
                JSONSerialization.jsonObject(
                    with: JSONEncoder().encode(WorkflowUpdateInput(id: "wf-1", patch: patch))
                ) as? [String: Any]
            )
        }

        let renamed = try object(WorkflowPatch(name: "Renamed"))
        XCTAssertEqual(renamed["id"] as? String, "wf-1")
        XCTAssertEqual(renamed["name"] as? String, "Renamed")
        XCTAssertEqual(Set(renamed.keys), Set(["id", "name"]))

        let bound = try object(WorkflowPatch(deviceId: .some("dev-1")))
        XCTAssertEqual(bound["deviceId"] as? String, "dev-1")
        XCTAssertNil(bound["name"])

        // Unbinding the runner is what an explicit null means.
        XCTAssertTrue(try object(WorkflowPatch(deviceId: .some(nil)))["deviceId"] is NSNull)

        // An empty patch names the row and nothing else.
        XCTAssertEqual(Set(try object(WorkflowPatch()).keys), Set(["id"]))
    }

    // EXP-984: the review payload and the open counter set go the same way —
    // an unknown key is ignored, a missing one defaults, and a payload that
    // names no verdict is no review at all rather than a blank block.
    func testTheReviewPayloadAndCountersParseTolerantly() {
        let review = WorkflowNodeReview.parse(
            #"""
            {"verdict":"request_changes","findings":"The oracle is missing.",
             "oracle":{"command":"bun test","passed":false},"model":"opus",
             "round":2,"at":"2026-09-19T09:00:00Z","future":"x"}
            """#
        )
        XCTAssertEqual(review?.verdict, "request_changes")
        XCTAssertEqual(review?.findings, "The oracle is missing.")
        XCTAssertEqual(review?.oracle, WorkflowReviewOracle(command: "bun test", passed: false))
        XCTAssertEqual(review?.model, "opus")
        XCTAssertEqual(review?.round, 2)
        XCTAssertNil(WorkflowNodeReview.parse(nil))
        XCTAssertNil(WorkflowNodeReview.parse("not json"))
        XCTAssertNil(WorkflowNodeReview.parse("{}"))
        XCTAssertEqual(WorkflowNodeReview.parse(#"{"verdict":"approve"}"#)?.round, 0)

        // The counters ride the metrics jsonb beside the shape keys; a value
        // that is not a whole number is left out rather than read as 0.
        let metrics = WorkflowMetrics.parse(
            #"{"nodes":2,"depth":2,"cycles":[],"landed":"garbage","reviewRounds":6}"#
        )
        XCTAssertEqual(metrics.counters["reviewRounds"], 6)
        XCTAssertEqual(metrics.counters["nodes"], 2)
        XCTAssertNil(metrics.counters["landed"])
        XCTAssertNil(metrics.counters["cycles"])

        // EXP-984: the launch's review model rides the same tolerant parse.
        XCTAssertEqual(
            WorkflowLaunch.parse(#"{"reviewModel":"opus"}"#).reviewModel, "opus"
        )
    }
}

// MARK: - Row builders

private func makeWorkflowNode(
    id: String,
    issueId: String,
    memberIssueIds: [String] = [],
    state: String = "in_review",
    lane: Int = 0,
    approvedAt: String? = nil,
    afterNodeIds: [String] = []
) -> WorkflowNodeEntity {
    WorkflowNodeEntity(
        id: id, workflowId: "wf-1", teamId: "t1", issueId: issueId,
        memberIssueIds: memberIssueIds, state: state, lane: lane,
        approvedAt: approvedAt, afterNodeIds: afterNodeIds,
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

// EXP-1082 — the workflow contract's display states, locked ×4 against the
// same fixture. The strip, header, primary action and chip menu cases decode
// here already and SKIP until EXP-1066 implements them.
final class WorkflowContractViewTests: XCTestCase {
    private struct Fixture: Decodable {
        let displayStates: [DisplayStateCase]
        let needsYouLabel: String
        let nodeStrips: [NodeStripCase]
        let headerCaptions: [HeaderCaptionCase]
        let primaryActions: [PrimaryActionCase]
        let chipMenus: [ChipMenuCase]
    }

    private struct DisplayStateCase: Decodable {
        let state: String
        let display: String
        let caption: String
    }

    private struct StripNode: Decodable {
        let id: String
        let identifier: String
        let state: String
        let wave: Int
        let lane: Int
        let members: Int
        let live: Bool
        let needsYou: Bool
        let note: String?
    }

    private struct StripChip: Decodable {
        let id: String
        let title: String
        let display: String
        let caption: String
        let stacked: Bool
        let members: Int
        let live: Bool
        let needsYou: Bool
    }

    private struct StripColumn: Decodable {
        let wave: Int
        let nodes: [StripChip]
    }

    private struct NodeStripCase: Decodable {
        let name: String
        let skip: Bool?
        let nodes: [StripNode]
        let edges: [[String]]
        let strip: [StripColumn]
    }

    private struct HeaderNodeCase: Decodable {
        let state: String
        let members: Int
    }

    private struct HeaderCaptionCase: Decodable {
        let name: String
        let skip: Bool?
        let status: String
        let device: String?
        let nodes: [HeaderNodeCase]
        let caption: String
    }

    private struct PrimaryActionCase: Decodable {
        let status: String
        let device: String?
        let action: String?
    }

    private struct ChipMenuCase: Decodable {
        let state: String
        let menu: [String]
    }

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

    func testDisplayStatesFixtureCases() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.displayStates.isEmpty)
        for testCase in fixture.displayStates {
            let display = WorkflowView.nodeDisplayState(testCase.state)
            XCTAssertEqual(display.rawValue, testCase.display, "display of \(testCase.state)")
            XCTAssertEqual(display.label, testCase.caption, "caption of \(testCase.state)")
        }
        // Every contract display value has a case.
        XCTAssertEqual(
            WorkflowNodeDisplayState.allCases.map(\.rawValue),
            DomainContract.wfNodeDisplayStateValues
        )
    }

    func testTheNeedsYouLabelIsByteLocked() throws {
        XCTAssertEqual(WorkflowView.needsYouLabel, try fixture().needsYouLabel)
    }

    func testNodeStripFixtureCases() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.nodeStrips.isEmpty)
        throw XCTSkip("EXP-1066")
    }

    func testHeaderCaptionFixtureCases() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.headerCaptions.isEmpty)
        throw XCTSkip("EXP-1066")
    }

    func testPrimaryActionFixtureCases() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.primaryActions.isEmpty)
        throw XCTSkip("EXP-1066")
    }

    func testChipMenuFixtureCases() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.chipMenus.isEmpty)
        throw XCTSkip("EXP-1066")
    }
}
