import Foundation
import XCTest
@testable import ExpCore

// EXP-900: the read-time activity fold, locked ×4 (web `fold.test.ts`, Android
// `ActivityFoldTest`, desktop `domain::activity_fold`) against the ONE contract
// fixture — same cases, same names.
final class ActivityFoldTests: XCTestCase {
    /// The committed contract fixture, read through `#filePath` because the
    /// unit-test bundle carries no repo resources.
    private func fixtureCases() throws -> [[String: Any]] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()          // ExpCore/Tests/
            .deletingLastPathComponent()          // ExpCore/
            .deletingLastPathComponent()          // apps/ios/
            .deletingLastPathComponent()          // apps/
            .deletingLastPathComponent()          // the repo root
            .appendingPathComponent("packages/domain-contract/fixtures/activity-fold.json")
        let json = try JSONSerialization.jsonObject(with: try Data(contentsOf: url))
        return try XCTUnwrap(json as? [[String: Any]])
    }

    /// The fixture's payload (an object, or JSON null) as the entity's stored
    /// JSON string.
    private func payloadString(_ value: Any?) throws -> String? {
        guard let value, !(value is NSNull) else { return nil }
        let data = try JSONSerialization.data(withJSONObject: value)
        return String(data: data, encoding: .utf8)
    }

    private func event(_ row: [String: Any]) throws -> IssueEventEntity {
        let createdAt = try XCTUnwrap(row["createdAt"] as? String)
        return IssueEventEntity(
            id: try XCTUnwrap(row["id"] as? String),
            issueId: try XCTUnwrap(row["issueId"] as? String),
            teamId: "t1",
            actorUserId: row["actorUserId"] as? String,
            type: try XCTUnwrap(row["type"] as? String),
            payload: try payloadString(row["payload"]),
            createdAt: createdAt,
            updatedAt: createdAt
        )
    }

    private func barrier(_ row: [String: Any]) throws -> ActivityBarrier {
        ActivityBarrier(
            issueId: try XCTUnwrap(row["issueId"] as? String),
            actorUserId: row["actorUserId"] as? String,
            createdAt: try XCTUnwrap(row["createdAt"] as? String)
        )
    }

    /// (id, type, payload as a decoded OBJECT, createdAt) — never the payload
    /// TEXT: key order differs between the fixture and a re-encoded fold.
    private func comparable(_ event: IssueEventEntity) -> [String: Any] {
        [
            "id": event.id,
            "type": event.type,
            "payload": decodedPayload(event.payload),
            "createdAt": event.createdAt,
        ]
    }

    private func comparableRow(_ row: [String: Any]) -> [String: Any] {
        var payload: Any = NSNull()
        if let raw = row["payload"], !(raw is NSNull) { payload = raw }
        return [
            "id": row["id"] as? String ?? "",
            "type": row["type"] as? String ?? "",
            "payload": payload,
            "createdAt": row["createdAt"] as? String ?? "",
        ]
    }

    private func decodedPayload(_ stored: String?) -> Any {
        guard let stored, let data = stored.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data)
        else { return NSNull() }
        return object
    }

    func testContractFixtureCases() throws {
        let cases = try fixtureCases()
        XCTAssertFalse(cases.isEmpty, "the activity-fold fixture must not be empty")
        for testCase in cases {
            let name = try XCTUnwrap(testCase["name"] as? String)
            let events = try (testCase["events"] as? [[String: Any]] ?? []).map { try event($0) }
            let barriers = try (testCase["barriers"] as? [[String: Any]] ?? []).map { try barrier($0) }
            let expected = (testCase["expected"] as? [[String: Any]] ?? [])
                .map { comparableRow($0) }
            let actual = foldActivity(events, barriers: barriers).map { comparable($0) }
            XCTAssertEqual(actual as NSArray, expected as NSArray, name)
        }
    }

    // MARK: - Direct tests

    private func statusEvent(
        id: String,
        from: String,
        to: String,
        at: String,
        actor: String? = "A"
    ) -> IssueEventEntity {
        IssueEventEntity(
            id: id,
            issueId: "i1",
            teamId: "t1",
            actorUserId: actor,
            type: DomainContract.issueEventTypeStatusChanged,
            payload: "{\"fromStatusId\":\"\(from)\",\"toStatusId\":\"\(to)\"}",
            createdAt: at,
            updatedAt: at
        )
    }

    /// Exactly the window folds; one millisecond more does not (`span > window`).
    func testWindowBoundary() {
        let atTheEdge = [
            statusEvent(id: "e1", from: "a", to: "b", at: "2026-09-19T10:00:00.000Z"),
            statusEvent(id: "e2", from: "b", to: "c", at: "2026-09-19T10:10:00.000Z"),
        ]
        XCTAssertEqual(foldActivity(atTheEdge).map(\.id), ["e2"], "a 10 min span still folds")

        let justOver = [
            statusEvent(id: "e1", from: "a", to: "b", at: "2026-09-19T10:00:00.000Z"),
            statusEvent(id: "e2", from: "b", to: "c", at: "2026-09-19T10:10:00.001Z"),
        ]
        XCTAssertEqual(
            foldActivity(justOver).map(\.id),
            ["e1", "e2"],
            "one millisecond past the window leaves the run alone"
        )
    }

    /// Electric's Postgres text form parses too, so a synced row folds like a
    /// tRPC-era one.
    func testPostgresWireTimestampsFold() {
        let events = [
            statusEvent(id: "e1", from: "a", to: "b", at: "2026-09-19 10:00:00+00"),
            statusEvent(id: "e2", from: "b", to: "a", at: "2026-09-19 10:01:00+00"),
        ]
        XCTAssertTrue(foldActivity(events).isEmpty, "A → B → A over the wire form cancels")
    }

    func testInputsAreNotMutated() {
        let events = [
            statusEvent(id: "e1", from: "a", to: "b", at: "2026-09-19T10:00:00Z"),
            statusEvent(id: "e2", from: "b", to: "c", at: "2026-09-19T10:01:00Z"),
        ]
        let payloadsBefore = events.map(\.payload)
        let folded = foldActivity(events)
        XCTAssertEqual(folded.count, 1)
        XCTAssertEqual(events.map(\.payload), payloadsBefore, "the input rows stay untouched")
        XCTAssertEqual(events.map(\.id), ["e1", "e2"])
        XCTAssertEqual(
            decodedPayload(folded[0].payload) as? NSDictionary,
            ["fromStatusId": "a", "toStatusId": "c"] as NSDictionary
        )
    }

    func testFoldFieldKey() {
        XCTAssertEqual(
            foldFieldKey(statusEvent(id: "e1", from: "a", to: "b", at: "2026-09-19T10:00:00Z")),
            "status"
        )
        let created = IssueEventEntity(
            id: "e2",
            issueId: "i1",
            teamId: "t1",
            actorUserId: "A",
            type: DomainContract.issueEventTypeCreated,
            payload: nil,
            createdAt: "2026-09-19T10:00:00Z",
            updatedAt: "2026-09-19T10:00:00Z"
        )
        XCTAssertNil(foldFieldKey(created), "a created row never folds")
    }
}
