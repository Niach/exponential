import XCTest
@testable import ExpCore

// Locks the HelpdeskApi wire types against the tRPC helpdesk router's response
// shapes (apps/web/src/lib/trpc/helpdesk.ts). tRPC runs without a transformer,
// so timestamps are ISO-8601 strings and booleans native JSON scalars — these
// fixtures mirror real `{result:{data}}`-unwrapped payloads (the envelope is
// TrpcClient's job; the Api structs decode the bare data).
final class HelpdeskApiTests: XCTestCase {
    private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
        try JSONDecoder().decode(T.self, from: Data(json.utf8))
    }

    // MARK: - listThreads

    func testListThreadsRowDecodesWithLastMessageAndUnread() throws {
        let rows = try decode([SupportThreadRow].self, #"""
        [
          {
            "id": "t1", "teamId": "w1", "title": "Login broken",
            "status": "open", "linkedIssueId": null,
            "reporterEmail": "jane@example.com", "reporterName": "Jane",
            "lastReporterSeenAt": "2026-07-18T09:30:00.000Z",
            "createdAt": "2026-07-18T09:00:00.000Z",
            "updatedAt": "2026-07-18T10:00:00.000Z",
            "lastMessage": {
              "body": "It still fails", "direction": "inbound",
              "createdAt": "2026-07-18T10:00:00.000Z"
            },
            "unread": true
          }
        ]
        """#)
        XCTAssertEqual(rows.count, 1)
        let row = try XCTUnwrap(rows.first)
        XCTAssertEqual(row.id, "t1")
        XCTAssertEqual(row.teamId, "w1")
        XCTAssertEqual(row.status, "open")
        XCTAssertNil(row.linkedIssueId)
        XCTAssertEqual(row.reporterName, "Jane")
        XCTAssertEqual(row.lastMessage?.body, "It still fails")
        XCTAssertEqual(row.lastMessage?.direction, "inbound")
        XCTAssertTrue(row.unread)
    }

    func testListThreadsRowDecodesNullLastMessageAndNulls() throws {
        // A ticket whose only message is internal has lastMessage null (the
        // helper filters to public visibility); reporterName is nullable.
        let rows = try decode([SupportThreadRow].self, #"""
        [
          {
            "id": "t2", "teamId": "w1", "title": "Question",
            "status": "resolved", "linkedIssueId": "i9",
            "reporterEmail": "anon@example.com", "reporterName": null,
            "lastReporterSeenAt": null,
            "createdAt": "2026-07-18T09:00:00.000Z",
            "updatedAt": "2026-07-18T09:00:00.000Z",
            "lastMessage": null,
            "unread": false
          }
        ]
        """#)
        let row = try XCTUnwrap(rows.first)
        XCTAssertNil(row.lastMessage)
        XCTAssertNil(row.reporterName)
        XCTAssertNil(row.lastReporterSeenAt)
        XCTAssertEqual(row.linkedIssueId, "i9")
        XCTAssertFalse(row.unread)
    }

    // MARK: - getThread

    func testGetThreadDecodesThreadMessagesAndLinkedIssue() throws {
        // The server returns the FULL support_threads row — unknown keys
        // (token bookkeeping etc.) must be ignored, not fatal.
        let detail = try decode(SupportThreadDetail.self, #"""
        {
          "thread": {
            "id": "t1", "teamId": "w1", "title": "Login broken",
            "status": "open", "linkedIssueId": "i5",
            "reporterEmail": "jane@example.com", "reporterName": "Jane",
            "tokenRevokedAt": null,
            "lastReporterSeenAt": "2026-07-18T09:30:00.000Z",
            "createdAt": "2026-07-18T09:00:00.000Z",
            "updatedAt": "2026-07-18T10:00:00.000Z"
          },
          "messages": [
            {
              "id": "m1", "threadId": "t1", "authorUserId": null,
              "direction": "inbound", "visibility": "public",
              "body": "I can't log in", "emailDeliveryId": null,
              "createdAt": "2026-07-18T09:00:00.000Z",
              "updatedAt": "2026-07-18T09:00:00.000Z"
            },
            {
              "id": "m2", "threadId": "t1", "authorUserId": "u1",
              "direction": "outbound", "visibility": "internal",
              "body": "Looks like the SSO regression",
              "emailDeliveryId": null,
              "createdAt": "2026-07-18T09:10:00.000Z",
              "updatedAt": "2026-07-18T09:10:00.000Z"
            }
          ],
          "linkedIssue": {
            "id": "i5", "identifier": "EXP-42", "title": "SSO regression",
            "status": "in_progress", "boardId": "b1"
          }
        }
        """#)
        XCTAssertEqual(detail.thread.id, "t1")
        XCTAssertEqual(detail.thread.linkedIssueId, "i5")
        XCTAssertEqual(detail.messages.count, 2)
        XCTAssertTrue(detail.messages[0].isInbound)
        XCTAssertNil(detail.messages[0].authorUserId)
        XCTAssertFalse(detail.messages[0].isInternal)
        XCTAssertTrue(detail.messages[1].isInternal)
        XCTAssertEqual(detail.messages[1].authorUserId, "u1")
        XCTAssertEqual(detail.linkedIssue?.identifier, "EXP-42")
        XCTAssertEqual(detail.linkedIssue?.boardId, "b1")
    }

    func testGetThreadDecodesNullLinkedIssue() throws {
        let detail = try decode(SupportThreadDetail.self, #"""
        {
          "thread": {
            "id": "t1", "teamId": "w1", "title": "Q", "status": "open",
            "linkedIssueId": null, "reporterEmail": "a@b.c",
            "reporterName": null,
            "createdAt": "2026-07-18T09:00:00.000Z",
            "updatedAt": "2026-07-18T09:00:00.000Z"
          },
          "messages": [],
          "linkedIssue": null
        }
        """#)
        XCTAssertNil(detail.linkedIssue)
        XCTAssertTrue(detail.messages.isEmpty)
    }

    // MARK: - escalate

    func testEscalateResultDecodesIssue() throws {
        // HelpdeskApi.escalate unwraps `{ issue }` — this fixture pins the
        // inner projection (txId rides along and is ignored).
        struct Envelope: Decodable { let issue: SupportEscalatedIssue }
        let result = try decode(Envelope.self, #"""
        { "issue": { "id": "i7", "identifier": "EXP-50", "title": "From ticket" }, "txId": 123 }
        """#)
        XCTAssertEqual(result.issue.id, "i7")
        XCTAssertEqual(result.issue.identifier, "EXP-50")
        XCTAssertEqual(result.issue.title, "From ticket")
    }
}
