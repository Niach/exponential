import Foundation
import XCTest
@testable import ExpCore

// EXP-596: `issues.create` returns the whole inserted row, which the creating
// client mirrors into its local store so it can open the new issue without
// waiting for the Electric long-poll. The id is the only REQUIRED part of that
// response — a row this build can't fully decode (an older or newer server)
// must never turn a committed create into a client-side failure.
final class IssueCreateResultDecodingTests: XCTestCase {
    private func decode(_ json: String) throws -> IssueCreateResult {
        try JSONDecoder().decode(IssueCreateResult.self, from: Data(json.utf8))
    }

    func testDecodesTheFullInsertedRow() throws {
        // tRPC JSON: camelCase keys, Dates stringified, `dueDate` a date-only
        // string, and the procedure's own `txId`/`mentionedUserIds` siblings.
        let result = try decode("""
        {"issue":{"id":"issue-1","boardId":"board-1","teamId":"team-1","number":42,
        "identifier":"EXP-42","title":"Mobile create","description":"lands on it",
        "status":"backlog","statusId":"status-1","priority":"none","assigneeId":"user-1",
        "creatorId":"user-1","source":"user","dueDate":"2026-08-22","sortOrder":1024,
        "completedAt":null,"duplicateOfId":null,"prUrl":null,"prNumber":null,
        "prState":null,"branch":null,"prMergedAt":null,
        "createdAt":"2026-08-22T07:00:00.000Z","updatedAt":"2026-08-22T07:00:00.000Z"},
        "txId":"1234","mentionedUserIds":[]}
        """)
        XCTAssertEqual(result.id, "issue-1")
        let issue = try XCTUnwrap(result.issue)
        XCTAssertEqual(issue.identifier, "EXP-42")
        XCTAssertEqual(issue.title, "Mobile create")
        XCTAssertEqual(issue.statusId, "status-1")
        XCTAssertEqual(issue.dueDate, "2026-08-22")
        // The mirrored row is the synced row's shape, field for field.
        XCTAssertEqual(issue.entity().id, "issue-1")
        XCTAssertEqual(issue.entity().boardId, "board-1")
    }

    func testKeepsTheIdWhenTheRowCannotBeDecoded() throws {
        // A row missing a field this build requires: the create still succeeded,
        // so the id survives and only the local mirror is skipped.
        let result = try decode("""
        {"issue":{"id":"issue-2","boardId":"board-1","title":"No timestamps"},"txId":"7"}
        """)
        XCTAssertEqual(result.id, "issue-2")
        XCTAssertNil(result.issue)
    }

    func testRejectsAResponseWithoutAnId() {
        XCTAssertThrowsError(try decode(#"{"issue":{"title":"nameless"},"txId":"7"}"#))
    }

    func testReplacingDescriptionKeepsEveryOtherField() throws {
        // The create response predates the post-upload markdown patch, so the
        // mirror rewrites just the description.
        let result = try decode("""
        {"issue":{"id":"issue-3","boardId":"board-1","number":7,"identifier":"EXP-7",
        "title":"With images","description":"stripped","status":"todo","statusId":null,
        "priority":"high","assigneeId":null,"creatorId":"user-1","source":"user",
        "dueDate":null,"sortOrder":null,"completedAt":null,"duplicateOfId":null,
        "prUrl":null,"prNumber":null,"prState":null,"branch":null,"prMergedAt":null,
        "createdAt":"2026-08-22T07:00:00.000Z","updatedAt":"2026-08-22T07:00:00.000Z"}}
        """)
        let patched = try XCTUnwrap(result.issue).entity()
            .replacingDescription("![shot](/api/attachments/att-1)")
        XCTAssertEqual(patched.description, "![shot](/api/attachments/att-1)")
        XCTAssertEqual(patched.id, "issue-3")
        XCTAssertEqual(patched.identifier, "EXP-7")
        XCTAssertEqual(patched.title, "With images")
        XCTAssertEqual(patched.status, "todo")
        XCTAssertEqual(patched.priority, "high")
        XCTAssertEqual(patched.createdAt, "2026-08-22T07:00:00.000Z")
    }
}
