import Foundation
import XCTest
@testable import ExpCore

// EXP-1184: the live-run display rule, replayed from the ×4 fixture
// `packages/domain-contract/fixtures/session-display.json` (web, desktop and
// Android replay the same file) — never hand-mirrored cases.
final class CodingSessionDisplayTests: XCTestCase {
    private struct Fixture: Decodable {
        struct Case: Decodable {
            let name: String
            let status: String
            let needsInput: Bool
            let agentBusy: Bool
            let prState: String?
            let state: String
            let working: Bool
            let statusTone: String
        }
        let cases: [Case]
    }

    private func fixture() throws -> Fixture {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/session-display.json")
        return try JSONDecoder().decode(Fixture.self, from: try Data(contentsOf: url))
    }

    private func session(
        status: String, needsInput: Bool = false, agentBusy: Bool = false
    ) -> CodingSessionEntity {
        CodingSessionEntity(
            id: "sess-1",
            issueId: "issue-1",
            teamId: "ws-1",
            userId: "user-1",
            deviceLabel: nil,
            status: status,
            needsInput: needsInput,
            agentBusy: agentBusy,
            startedAt: "2026-07-17T09:00:00Z",
            endedAt: nil,
            createdAt: "2026-07-17T09:00:00Z",
            updatedAt: "2026-07-17T11:30:00Z"
        )
    }

    func testFixtureCases() throws {
        let fixture = try fixture()
        XCTAssertFalse(fixture.cases.isEmpty)
        for testCase in fixture.cases {
            let state = CodingSessionDisplayState.of(
                session: session(
                    status: testCase.status,
                    needsInput: testCase.needsInput,
                    agentBusy: testCase.agentBusy
                ),
                prState: testCase.prState
            )
            XCTAssertEqual(state.rawValue, testCase.state, testCase.name)
            XCTAssertEqual(
                CodingSessionDisplayState.working(status: testCase.status, state: state),
                testCase.working,
                testCase.name
            )
            XCTAssertEqual(state.statusTone.rawValue, testCase.statusTone, testCase.name)
        }
    }

    // The Work screen's viewer signal overrides the synced flag both ways.
    func testViewerBusyOverridesTheSyncedFlag() {
        XCTAssertEqual(
            CodingSessionDisplayState.of(
                session: session(status: "in_review", agentBusy: false), prState: "open", agentBusy: true
            ),
            .working
        )
        XCTAssertEqual(
            CodingSessionDisplayState.of(
                session: session(status: "running", agentBusy: true), prState: nil, agentBusy: false
            ),
            .done
        )
    }

    // EXP-848: a paused or disconnected row never animates; a row written
    // before the column exists decodes false, so it never animates either.
    func testWorkingHonoursTheCallersNarrowings() {
        XCTAssertFalse(CodingSessionDisplayState.working(status: "running", state: .working, paused: true))
        XCTAssertFalse(CodingSessionDisplayState.working(status: "running", state: .working, live: false))
        XCTAssertFalse(session(status: "running").agentBusy)
    }
}
