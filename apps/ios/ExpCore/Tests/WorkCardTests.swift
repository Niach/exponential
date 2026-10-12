import Foundation
import XCTest
@testable import ExpCore

// EXP-1240 (EXP-1259 D12): THE Work card's model, locked ×4 (TS
// `work-card.test.ts`, desktop `steer::work_card`, Android `WorkCardTest`)
// against `packages/domain-contract/fixtures/work-card.json` — every case's
// WHOLE expected object, `fill` within 1e-9. Read through `#filePath`: the
// unit-test bundle carries no repo resources.
final class WorkCardTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/work-card.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [String: Any])
    }

    private func int(_ raw: Any?) -> Int? { (raw as? NSNumber)?.intValue }

    private func input(_ raw: [String: Any]) throws -> WorkCardInput {
        switch raw["kind"] as? String {
        case "workflow":
            let phases = (raw["phases"] as? [[String: Any]] ?? []).map {
                WorkflowCaptionPhase(index: int($0["index"]) ?? 0, title: $0["title"] as? String ?? "")
            }
            let agents = (raw["agents"] as? [[String: Any]] ?? []).map {
                WorkCardWorkflowAgent(
                    index: int($0["index"]) ?? 0,
                    label: $0["label"] as? String,
                    phaseIndex: int($0["phaseIndex"]),
                    model: $0["model"] as? String,
                    state: $0["state"] as? String ?? "",
                    durationMs: int($0["durationMs"])
                )
            }
            return .workflow(
                name: try XCTUnwrap(raw["name"] as? String),
                status: try XCTUnwrap(raw["status"] as? String),
                phases: phases,
                agents: agents,
                elapsedMs: int(raw["elapsedMs"])
            )
        case "tasks":
            return .tasks((raw["tasks"] as? [[String: Any]] ?? []).map {
                WorkCardTask(content: $0["content"] as? String ?? "", status: $0["status"] as? String ?? "")
            })
        default:
            return .subagent(
                title: raw["title"] as? String,
                state: try XCTUnwrap(raw["state"] as? String),
                toolCalls: int(raw["toolCalls"]),
                elapsedMs: int(raw["elapsedMs"])
            )
        }
    }

    private func lanes(_ raw: Any?) throws -> [WorkCardLane] {
        try (raw as? [[String: Any]] ?? []).map {
            WorkCardLane(
                label: try XCTUnwrap($0["label"] as? String),
                model: $0["model"] as? String,
                state: try XCTUnwrap($0["state"] as? String),
                duration: $0["duration"] as? String
            )
        }
    }

    private func row(_ raw: [String: Any]) throws -> WorkCardRow {
        switch raw["type"] as? String {
        case "folded":
            return .folded(
                label: try XCTUnwrap(raw["label"] as? String),
                lanes: try XCTUnwrap(raw["lanes"] as? String),
                duration: raw["duration"] as? String
            )
        case "phase":
            return .phase(
                title: try XCTUnwrap(raw["title"] as? String),
                state: try XCTUnwrap(WorkState(rawValue: raw["state"] as? String ?? "")),
                detail: raw["detail"] as? String,
                duration: raw["duration"] as? String,
                lanes: try lanes(raw["lanes"])
            )
        case "task":
            return .task(
                label: try XCTUnwrap(raw["label"] as? String),
                state: try XCTUnwrap(WorkState(rawValue: raw["state"] as? String ?? ""))
            )
        default:
            return .status(label: try XCTUnwrap(raw["label"] as? String))
        }
    }

    func testEveryFixtureCaseRendersTheWholeCard() throws {
        let cases = try XCTUnwrap(try fixture()["cases"] as? [[String: Any]])
        XCTAssertGreaterThanOrEqual(cases.count, 10)
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let want = try XCTUnwrap(c["expected"] as? [String: Any])
            let got = WorkCards.card(try input(try XCTUnwrap(c["input"] as? [String: Any])))

            XCTAssertEqual(got.kind.rawValue, want["kind"] as? String, name)
            XCTAssertEqual(got.icon, want["icon"] as? String, name)
            XCTAssertEqual(got.name, want["name"] as? String, name)
            XCTAssertEqual(got.kindWord, want["kindWord"] as? String, name)
            XCTAssertEqual(got.progress, want["progress"] as? String, name)
            XCTAssertEqual(got.elapsed, want["elapsed"] as? String, name)
            XCTAssertEqual(got.running, want["running"] as? Bool, name)

            let segments = try XCTUnwrap(want["segments"] as? [[String: Any]])
            XCTAssertEqual(got.segments.count, segments.count, name)
            for (mine, theirs) in zip(got.segments, segments) {
                XCTAssertEqual(mine.state.rawValue, theirs["state"] as? String, name)
                let fill = try XCTUnwrap((theirs["fill"] as? NSNumber)?.doubleValue)
                XCTAssertEqual(mine.fill, fill, accuracy: 1e-9, name)
            }

            let rows = try (want["rows"] as? [[String: Any]] ?? []).map(row)
            XCTAssertEqual(got.rows, rows, name)
            // Nothing the fixture does not name.
            XCTAssertEqual(
                Set(want.keys),
                ["kind", "icon", "name", "kindWord", "progress", "elapsed", "running", "segments", "rows"],
                name
            )
        }
    }

    func testTheCopyIconsAndGeometryAreTheFixtures() throws {
        let fixture = try fixture()
        let copy = try XCTUnwrap(fixture["copy"] as? [String: String])
        XCTAssertEqual(copy, [
            "workflow": WorkCards.Copy.workflow,
            "tasks": WorkCards.Copy.tasks,
            "subagent": WorkCards.Copy.subagent,
            "phase": WorkCards.Copy.phase,
            "loosePhase": WorkCards.Copy.loosePhase,
            "agent": WorkCards.Copy.agent,
            "lane": WorkCards.Copy.lane,
            "lanes": WorkCards.Copy.lanes,
            "call": WorkCards.Copy.call,
            "calls": WorkCards.Copy.calls,
            "running": WorkCards.Copy.running,
            "done": WorkCards.Copy.done,
            "failed": WorkCards.Copy.failed,
        ])
        XCTAssertEqual(try XCTUnwrap(fixture["icons"] as? [String: String]), [
            "workflow": WorkCards.Icons.workflow,
            "tasks": WorkCards.Icons.tasks,
            "subagent": WorkCards.Icons.subagent,
        ])
        XCTAssertEqual(try XCTUnwrap(fixture["testIds"] as? [String: String]), [
            "card": WorkCards.TestIds.card,
            "lane": WorkCards.TestIds.lane,
            "folded": WorkCards.TestIds.folded,
        ])
        let geometry = try XCTUnwrap(fixture["geometry"] as? [String: NSNumber])
        XCTAssertEqual(geometry["header"]?.doubleValue, WorkCards.Geometry.header)
        XCTAssertEqual(geometry["row"]?.doubleValue, WorkCards.Geometry.row)
        XCTAssertEqual(geometry["lane"]?.doubleValue, WorkCards.Geometry.lane)
        XCTAssertEqual(geometry["laneIndent"]?.doubleValue, WorkCards.Geometry.laneIndent)
        XCTAssertEqual(geometry["segmentHeight"]?.doubleValue, WorkCards.Geometry.segmentHeight)
        XCTAssertEqual(geometry["segmentGap"]?.doubleValue, WorkCards.Geometry.segmentGap)
    }

    /// The folded row expands into the phases it stands for, and a lane names
    /// the agent behind it (web `workflowFoldedPhases` / `workflowLaneAgent`).
    func testFoldedPhasesAndLaneTargetsFollowThePhaseOrder() {
        let phases = [
            WorkflowCaptionPhase(index: 1, title: "Alpha"),
            WorkflowCaptionPhase(index: 2, title: "Beta"),
            WorkflowCaptionPhase(index: 3, title: "Gamma"),
        ]
        let agents = [
            WorkCardWorkflowAgent(index: 1, label: "a", phaseIndex: 1, state: "done", durationMs: 60_000),
            WorkCardWorkflowAgent(index: 2, label: "b", phaseIndex: 2, state: "done", durationMs: 30_000),
            WorkCardWorkflowAgent(index: 3, label: "c", phaseIndex: 3, state: "running"),
        ]
        let card = WorkCards.card(.workflow(
            name: "probe", status: "running", phases: phases, agents: agents,
            elapsedMs: WorkCards.workflowElapsedMs(phases: phases, agents: agents)
        ))
        XCTAssertEqual(card.elapsed, "1m")
        XCTAssertEqual(card.foldedPhaseCount, 2)
        XCTAssertEqual(card.rowPhases, [0, 2])
        let folded = WorkCards.foldedPhases(phases: phases, agents: agents, card: card)
        XCTAssertEqual(folded.map(\.title), ["Alpha", "Beta"])
        XCTAssertEqual(folded.map(\.duration), ["1m", "30s"])
        XCTAssertEqual(folded[1].lanes.first?.duration, "30s")
        XCTAssertEqual(WorkCards.laneAgent(phases: phases, agents: agents, phase: 2, lane: 0)?.label, "c")
        XCTAssertNil(WorkCards.laneAgent(phases: phases, agents: agents, phase: 3, lane: 0))
    }
}
