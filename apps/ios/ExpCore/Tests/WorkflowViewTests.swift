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
        let titles: [TitleCase]
        let edges: [EdgeCase]
        // EXP-982 — running a workflow.
        let startBlockers: [StartBlockerCase]
        let finalPr: [FinalPrCase]
        let rowSubtitles: [RowSubtitleCase]
        // EXP-983 — speculative starts.
        let edgeStyles: [EdgeStyleCase]
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

    // MARK: - Review gate, dynamic graphs, budgets, metrics (EXP-984)

    // The review gate's copy, byte for byte — the note is what a `proposed`
    // node's panel says before a member decides.
    func testTheReviewGateCopyIsByteLocked() {
        XCTAssertEqual(WorkflowView.admitNodeLabel, "Admit")
        XCTAssertEqual(WorkflowView.dismissNodeLabel, "Dismiss")
        XCTAssertEqual(
            WorkflowView.proposedNodeNote,
            "Filed during the run. Admit it into the workflow or dismiss it."
        )
        XCTAssertEqual(WorkflowView.nodeUnsyncedTitle, "Not synced yet")
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
        // Its chip offers Admit / Dismiss.
        let proposed = makeWorkflowNode(
            id: "a", issueId: "i1", state: DomainContract.wfNodeStateProposed
        )
        XCTAssertTrue(proposed.isProposed)
        XCTAssertEqual(WorkflowView.nodeChipMenu(state: proposed.state), [.admit, .dismiss])
    }

    // MARK: - Running a workflow (EXP-982)

    func testRunningTheWorkflowFixtureCases() throws {
        let fixture = try fixture()

        XCTAssertFalse(fixture.startBlockers.isEmpty)
        for testCase in fixture.startBlockers {
            let workflow = WorkflowView.StartableWorkflow(
                status: testCase.workflow.status,
                deviceId: testCase.workflow.deviceId,
                repositoryId: testCase.workflow.repositoryId
            )
            XCTAssertEqual(
                WorkflowView.startBlocker(workflow, metrics: testCase.metrics),
                testCase.blocker,
                testCase.workflow.status
            )
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
        XCTAssertEqual(
            WorkflowView.cancelConfirm,
            "Its live runs end and its branch is deleted. Nothing reached the default branch."
        )
        XCTAssertEqual(WorkflowView.finalPrTitle, "Final pull request")
        XCTAssertEqual(WorkflowView.mergeFinalPrLabel, "Merge")
        XCTAssertEqual(
            WorkflowView.mergeFinalPrConfirm,
            "The workflow's branch is squash-merged into the default branch and the run is done."
        )
        XCTAssertEqual(WorkflowView.retryNodeLabel, "Retry")
        XCTAssertEqual(WorkflowView.skipNodeLabel, "Skip")
        XCTAssertEqual(
            WorkflowView.skipNodeConfirm,
            "Its dependents go on without it. The node's work is not part of the final pull request."
        )
    }

    // The entity overload the run surfaces actually call: a bound draft has
    // no blocker.
    func testTheStartBlockerReadsTheSyncedRow() {
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
    // server-side), so neither a launch nor a start rule ever leaves this
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

    // EXP-984: the review payload goes the same way — an unknown key is
    // ignored, a missing one defaults, and a payload that names no verdict is
    // no review at all rather than a blank block.
    func testTheReviewPayloadParsesTolerantly() {
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

        // EXP-1090: an old row's counters beside the shape keys are ignored;
        // the shape still reads.
        let metrics = WorkflowMetrics.parse(
            #"{"nodes":2,"depth":2,"cycles":[],"landed":"garbage","reviewRounds":6}"#
        )
        XCTAssertEqual(metrics, WorkflowMetrics(nodes: 2, depth: 2))

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

// EXP-1082/EXP-1066 — the workflow page's view model, locked ×4 against the
// same fixture: display states, the strip, the header caption, the primary
// action and the chip menu.
final class WorkflowContractViewTests: XCTestCase {
    private struct Fixture: Decodable {
        let displayStates: [DisplayStateCase]
        let needsYouLabel: String
        let nodeStrips: [NodeStripCase]
        let headerCaptions: [HeaderCaptionCase]
        let primaryActions: [PrimaryActionCase]
        let chipMenus: [ChipMenuCase]
        let overflowMenus: [OverflowMenuCase]
        let pageLabels: [String: String]
        let statusGlyphs: [StatusGlyphCase]
        let proposedNodeNote: String
    }

    private struct StatusGlyphCase: Decodable {
        let status: String
        let display: String
    }

    private struct OverflowMenuCase: Decodable {
        let status: String
        let menu: [String]
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
        let nodes: [StripNode]
        let strip: [StripColumn]
    }

    private struct HeaderNodeCase: Decodable {
        let state: String
        let members: Int
    }

    private struct HeaderCaptionCase: Decodable {
        let name: String
        let status: String
        let device: String?
        let nodes: [HeaderNodeCase]
        let caption: String
    }

    private struct PrimaryActionCase: Decodable {
        let status: String
        let device: String?
        let action: String?
        let finalPrState: String?
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
        for testCase in fixture.nodeStrips {
            let actual = WorkflowView.nodeStrip(
                nodes: testCase.nodes.map {
                    StripNodeInput(
                        id: $0.id, identifier: $0.identifier, state: $0.state,
                        wave: $0.wave, lane: $0.lane, members: $0.members,
                        live: $0.live, needsYou: $0.needsYou, note: $0.note
                    )
                }
            )
            let expected = testCase.strip.map { column in
                StripWave(
                    wave: column.wave,
                    nodes: column.nodes.map {
                        NodeChip(
                            id: $0.id, title: $0.title,
                            display: WorkflowNodeDisplayState(rawValue: $0.display) ?? .queued,
                            caption: $0.caption, stacked: $0.stacked, members: $0.members,
                            live: $0.live, needsYou: $0.needsYou
                        )
                    }
                )
            }
            XCTAssertEqual(actual, expected, testCase.name)
        }
    }

    func testHeaderCaptionFixtureCases() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.headerCaptions.isEmpty)
        for testCase in fixture.headerCaptions {
            XCTAssertEqual(
                WorkflowView.headerCaption(
                    status: testCase.status,
                    nodes: testCase.nodes.map { HeaderNode(state: $0.state, members: $0.members) },
                    deviceLabel: testCase.device
                ),
                testCase.caption,
                testCase.name
            )
        }
    }

    func testPrimaryActionFixtureCases() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.primaryActions.isEmpty)
        for testCase in fixture.primaryActions {
            XCTAssertEqual(
                WorkflowView.primaryAction(
                    status: testCase.status, deviceLabel: testCase.device,
                    finalPrState: testCase.finalPrState
                )?.rawValue,
                testCase.action,
                "\(testCase.status) \(testCase.device ?? "nil") \(testCase.finalPrState ?? "nil")"
            )
        }
    }

    func testStatusGlyphFixtureCases() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.statusGlyphs.isEmpty)
        for testCase in fixture.statusGlyphs {
            XCTAssertEqual(
                WorkflowView.statusGlyph(status: testCase.status).rawValue,
                testCase.display, testCase.status
            )
        }
    }

    func testTheProposedNodeNoteIsByteLocked() throws {
        XCTAssertEqual(WorkflowView.proposedNodeNote, try fixture().proposedNodeNote)
        let strip = WorkflowView.nodeStrip(
            nodes: [
                StripNodeInput(id: "p", identifier: "EXP-9", state: "proposed", wave: 0, lane: 0),
            ]
        )
        XCTAssertEqual(strip.first?.nodes.first?.caption, WorkflowView.proposedNodeNote)
    }

    func testChipMenuFixtureCases() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.chipMenus.isEmpty)
        for testCase in fixture.chipMenus {
            XCTAssertEqual(
                WorkflowView.nodeChipMenu(state: testCase.state).map(\.rawValue),
                testCase.menu,
                testCase.state
            )
        }
    }

    func testOverflowMenuFixtureCases() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.overflowMenus.isEmpty)
        for testCase in fixture.overflowMenus {
            XCTAssertEqual(
                WorkflowView.overflowMenu(status: testCase.status).map(\.rawValue),
                testCase.menu,
                testCase.status
            )
        }
    }

    func testPageLabelsAreByteLocked() throws {
        let labels = try fixture().pageLabels
        XCTAssertEqual(labels.count, 10)
        XCTAssertEqual(labels["allNodes"], WorkflowView.allNodesLabel)
        XCTAssertEqual(labels["decisions"], WorkflowView.decisionsLabel)
        XCTAssertEqual(labels["stop"], WorkflowView.stopWorkflowLabel)
        XCTAssertEqual(labels["pickDevice"], WorkflowView.pickDeviceLabel)
        XCTAssertEqual(labels["runsOn"], WorkflowView.runsOnLabel)
        XCTAssertEqual(labels["reviewFinalPr"], WorkflowView.reviewFinalPrLabel)
        XCTAssertEqual(labels["noChanges"], WorkflowView.noChangesLabel)
        XCTAssertEqual(labels["noRuns"], WorkflowView.noRunsLabel)
        XCTAssertEqual(labels["noResults"], WorkflowView.noResultsLabel)
        XCTAssertEqual(labels["dismissNodeConfirm"], WorkflowView.dismissNodeConfirm)
    }
}

// A picked node's embedded Work screen: its subject and the faces it offers
// come from the SAME lookup the screen runs, so no face is offered that the
// screen would draw as an endless spinner.
final class WorkflowNodeWorkTests: XCTestCase {
    private let now = WireTimestamps.parse("2026-09-15T12:00:00Z")!
    private let shot = #"[{"topic":"t","label":"ios","attachmentId":"a1","width":10,"height":10}]"#

    private func run(
        _ id: String, issueId: String?, userId: String = "me", results: String? = nil
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: id, issueId: issueId, teamId: "t1", userId: userId,
            deviceLabel: "mac", status: "running", results: results,
            startedAt: "2026-09-15T11:00:00Z", endedAt: nil,
            createdAt: "2026-09-15T11:00:00Z", updatedAt: "2026-09-15T11:59:00Z"
        )
    }

    func testABatchNodesOwnIssueLessRunIsTheSubject() {
        let work = WorkflowView.nodeWork(
            issueId: "p", sessionId: "batch", issuePushed: true,
            sessions: [run("batch", issueId: nil, results: shot)], me: "me", now: now
        )
        XCTAssertEqual(work.subject, .session(id: "batch"))
        XCTAssertEqual(work.faces, [.run, .results])
    }

    func testATeammatesRunOffersNoRunFace() {
        let batch = WorkflowView.nodeWork(
            issueId: "p", sessionId: "theirs", issuePushed: false,
            sessions: [run("theirs", issueId: nil, userId: "them", results: shot)],
            me: "me", now: now
        )
        XCTAssertEqual(batch.subject, .issue(id: "p"))
        XCTAssertEqual(batch.faces, [.issue])
        let single = WorkflowView.nodeWork(
            issueId: "i", sessionId: "theirs", issuePushed: true,
            sessions: [run("theirs", issueId: "i", userId: "them", results: shot)],
            me: "me", now: now
        )
        XCTAssertEqual(single.subject, .issue(id: "i"))
        XCTAssertEqual(single.faces, [.issue, .changes])
    }

    func testASingleIssueNodeUsesTheIssuesCodingTarget() {
        let work = WorkflowView.nodeWork(
            issueId: "i", sessionId: "mine", issuePushed: false,
            sessions: [run("mine", issueId: "i", results: shot)], me: "me", now: now
        )
        XCTAssertEqual(work.subject, .issue(id: "i"))
        XCTAssertEqual(work.faces, [.issue, .run, .results])
        let noRun = WorkflowView.nodeWork(
            issueId: "i", sessionId: nil, issuePushed: false, sessions: [], me: "me", now: now
        )
        XCTAssertEqual(noRun.faces, [.issue])
    }

    // Web reads an EMPTY `deviceLabel` as absent (`deviceLabel ? ... : ...`).
    func testAnEmptyDeviceLabelReadsAsAbsent() {
        XCTAssertEqual(WorkflowView.primaryAction(status: DomainContract.wfStatusDraft, deviceLabel: ""), .pickDevice)
        XCTAssertEqual(WorkflowView.primaryAction(status: DomainContract.wfStatusDraft, deviceLabel: nil), .pickDevice)
        XCTAssertEqual(WorkflowView.primaryAction(status: DomainContract.wfStatusDraft, deviceLabel: "mac"), .start)
        let nodes = [HeaderNode(state: DomainContract.wfNodeStateRunning)]
        XCTAssertEqual(
            WorkflowView.headerCaption(status: DomainContract.wfStatusRunning, nodes: nodes, deviceLabel: ""),
            WorkflowView.headerCaption(status: DomainContract.wfStatusRunning, nodes: nodes, deviceLabel: nil)
        )
        XCTAssertTrue(
            WorkflowView.headerCaption(status: DomainContract.wfStatusRunning, nodes: nodes, deviceLabel: "mac")
                .hasPrefix("on mac · ")
        )
    }
}
