import Foundation
import XCTest
@testable import ExpCore

// EXP-850 §7: the workflow caption, locked ×4 (TS
// `workflow-caption.test.ts`, desktop `steer::workflow_caption`, Android
// `WorkflowCaptionTest`) against the ONE contract fixture — same cases, read
// through `#filePath` because the unit-test bundle carries no repo resources
// (the ToolGroupSummaryTests playbook).
final class WorkflowCaptionTests: XCTestCase {
    private struct FixtureCase {
        /// The fixture case's own name (the assertion message).
        let caseName: String
        /// The workflow's name — what the caption prints.
        let name: String
        let status: String
        let phases: [WorkflowCaptionPhase]
        let agents: [WorkflowCaptionAgent]
        let expected: String
    }

    private func cases() throws -> [FixtureCase] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/workflow-caption.json")
        let data = try Data(contentsOf: url)
        let raw = try XCTUnwrap(try JSONSerialization.jsonObject(with: data) as? [[String: Any]])
        return try raw.map { object in
            let workflow = try XCTUnwrap(object["workflow"] as? [String: Any])
            let phases = (workflow["phases"] as? [[String: Any]] ?? []).map { phase in
                WorkflowCaptionPhase(
                    index: (phase["index"] as? NSNumber)?.intValue ?? 0,
                    title: phase["title"] as? String ?? ""
                )
            }
            let agents = (workflow["agents"] as? [[String: Any]] ?? []).map { agent in
                WorkflowCaptionAgent(
                    index: (agent["index"] as? NSNumber)?.intValue ?? 0,
                    state: agent["state"] as? String ?? "",
                    phaseIndex: (agent["phaseIndex"] as? NSNumber)?.intValue
                )
            }
            return FixtureCase(
                caseName: try XCTUnwrap(object["name"] as? String),
                name: try XCTUnwrap(workflow["name"] as? String),
                status: try XCTUnwrap(workflow["status"] as? String),
                phases: phases,
                agents: agents,
                expected: try XCTUnwrap(object["expected"] as? String)
            )
        }
    }

    func testEveryFixtureCaseRendersByteExact() throws {
        let cases = try cases()
        XCTAssertGreaterThanOrEqual(cases.count, 12)
        for fixture in cases {
            XCTAssertEqual(
                WorkflowCaption.caption(
                    name: fixture.name,
                    status: fixture.status,
                    phases: fixture.phases,
                    agents: fixture.agents
                ),
                fixture.expected,
                fixture.caseName
            )
        }
    }

    func testTheFixtureCoversEveryStatus() throws {
        let expectations = try cases().map(\.expected)
        XCTAssertTrue(expectations.contains { $0.hasSuffix("· starting") })
        XCTAssertTrue(expectations.contains { $0.contains("agents done ·") })
        XCTAssertTrue(expectations.contains { $0.hasSuffix("· done · 1 agent") })
        XCTAssertTrue(expectations.contains { $0.hasSuffix("· failed") })
        XCTAssertTrue(expectations.contains { $0.hasSuffix("· stopped") })
    }

    // An unknown status reads as stopped — a run that is over and did not say
    // it succeeded must never render as still running.
    func testAnUnknownStatusReadsAsStopped() {
        XCTAssertEqual(
            WorkflowCaption.caption(name: "probe", status: "exploded"),
            "Workflow probe · stopped"
        )
    }

    // The card's own convenience wrapper goes through the same derivation.
    func testTheCardCaptionMatchesTheSharedRule() {
        let workflow = AgentWorkflow(
            id: "toolu_1",
            name: "wire-probe",
            status: .running,
            phases: [AgentWorkflowPhase(index: 1, title: "Alpha")],
            agents: [
                AgentWorkflowAgent(index: 1, label: "alpha:one", phaseIndex: 1, state: .done),
                AgentWorkflowAgent(index: 2, label: "alpha:two", phaseIndex: 1, state: .running),
            ]
        )
        XCTAssertEqual(workflow.caption, "Workflow wire-probe · 1/2 agents done · Alpha")
    }
}
