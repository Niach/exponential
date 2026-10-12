import XCTest
@testable import ExpCore

// EXP-1284: the `threads` router's wire forms. `threads.update` CLEARS a
// title with an explicit `null` (never an omitted key), link/unlink carry
// exactly ONE target, and the response rows decode tolerantly — `results`
// (a native jsonb array over tRPC) lands as the same TEXT the shape stores.
final class ThreadsApiWireTests: XCTestCase {
    private func json(_ value: some Encodable) throws -> [String: Any] {
        let data = try JSONEncoder().encode(value)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    func testUpdateSendsTheTitleOrAnExplicitNull() throws {
        let set = try json(UpdateThreadInput(id: "t1", title: "Onboarding"))
        XCTAssertEqual(set["title"] as? String, "Onboarding")

        let cleared = try json(UpdateThreadInput(id: "t1", title: nil))
        XCTAssertEqual(cleared.keys.sorted(), ["id", "title"])
        XCTAssertTrue(cleared["title"] is NSNull)
    }

    func testCreateOmitsUnsetFields() throws {
        let bare = try json(CreateThreadInput(teamId: "ws1"))
        XCTAssertEqual(bare.keys.sorted(), ["teamId"])

        let seeded = try json(CreateThreadInput(teamId: "ws1", title: "T", issueIds: ["i1", "i2"]))
        XCTAssertEqual(seeded["issueIds"] as? [String], ["i1", "i2"])
    }

    func testLinkCarriesExactlyOneTarget() throws {
        let issue = try json(ThreadLinkInput.issue(threadId: "t1", issueId: "i1"))
        XCTAssertEqual(issue.keys.sorted(), ["id", "issueId"])
        XCTAssertEqual(issue["id"] as? String, "t1")

        let run = try json(ThreadLinkInput.run(threadId: "t1", sessionId: "cs1"))
        XCTAssertEqual(run.keys.sorted(), ["id", "sessionId"])
        XCTAssertEqual(run["sessionId"] as? String, "cs1")
    }

    func testGetDecodesTheThreadAndItsItems() throws {
        let body = """
            {"thread":{"id":"t1","teamId":"ws1","title":null,"createdBy":"u1",
                       "results":[{"topic":"Summary","label":"Board","attachmentId":"a1"}],
                       "createdAt":"2026-10-12T09:00:00.000Z","updatedAt":"2026-10-12T10:00:00.000Z"},
             "items":[{"id":"ti1","threadId":"t1","kind":"issue","issueId":"i1","sessionId":null,
                       "teamId":"ws1","boardId":"b1","boardDeletedAt":null,"boardArchivedAt":null,
                       "addedBy":"u1","addedBySessionId":null,"source":"user","sortOrder":1,
                       "createdAt":"2026-10-12T09:00:00.000Z","updatedAt":"2026-10-12T09:00:00.000Z"}]}
            """
        let detail = try JSONDecoder().decode(ThreadDetail.self, from: Data(body.utf8))
        XCTAssertNil(detail.thread.title)
        XCTAssertEqual(parseSessionResults(detail.thread.results).map(\.attachmentId), ["a1"])
        XCTAssertEqual(detail.thread.entity().updatedAt, "2026-10-12T10:00:00.000Z")
        XCTAssertEqual(detail.items.count, 1)
        let item = detail.items[0].entity()
        XCTAssertEqual(item.targetId, "i1")
        XCTAssertEqual(item.boardId, "b1")
        XCTAssertEqual(item.sortOrder, 1)
    }

    func testUnlinkDecodesTheRunsNewThread() throws {
        let moved = try JSONDecoder().decode(
            ThreadUnlinkResult.self,
            from: Data(#"{"removed":true,"txid":42,"newThreadId":"t9"}"#.utf8)
        )
        XCTAssertTrue(moved.removed)
        XCTAssertEqual(moved.newThreadId, "t9")

        let issue = try JSONDecoder().decode(
            ThreadUnlinkResult.self, from: Data(#"{"removed":false,"txid":42}"#.utf8)
        )
        XCTAssertFalse(issue.removed)
        XCTAssertNil(issue.newThreadId)
    }
}
