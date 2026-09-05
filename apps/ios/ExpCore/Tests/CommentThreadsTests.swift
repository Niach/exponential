import XCTest
@testable import ExpCore

// EXP-741: the ONE grouping rule every client's activity feed applies.
final class CommentThreadsTests: XCTestCase {
    private func row(_ id: String, parent: String? = nil) -> CommentEntity {
        CommentEntity(
            id: id, issueId: "i-1", teamId: "t-1", authorId: "u-1", body: "hi",
            kind: "regular", editedAt: nil, createdAt: "2026-07-03T10:00:00Z",
            updatedAt: "2026-07-03T10:00:00Z", parentId: parent
        )
    }

    func testKeepsOrderAndGroupsRepliesUnderTheirParent() {
        let threads = threadComments([row("a"), row("a1", parent: "a"), row("b"), row("a2", parent: "a"), row("b1", parent: "b")])
        XCTAssertEqual(threads.topLevel.map(\.id), ["a", "b"])
        XCTAssertEqual(threads.repliesByParent["a"]?.map(\.id), ["a1", "a2"])
        XCTAssertEqual(threads.repliesByParent["b"]?.map(\.id), ["b1"])
        XCTAssertEqual(threads.count, 5)
    }

    func testOrphanReplySurfacesAsTopLevel() {
        let threads = threadComments([row("orphan", parent: "gone"), row("c")])
        XCTAssertEqual(threads.topLevel.map(\.id), ["orphan", "c"])
        XCTAssertTrue(threads.repliesByParent.isEmpty)
    }

    func testNeverNestsARowUnderItself() {
        XCTAssertEqual(threadComments([row("self", parent: "self")]).topLevel.map(\.id), ["self"])
    }

    func testViaMcpReadsOffTheSource() {
        XCTAssertFalse(row("a").isViaMcp)
        let mcp = CommentEntity(
            id: "m", issueId: "i-1", teamId: "t-1", authorId: "u-1", body: "bot",
            kind: "regular", editedAt: nil, createdAt: "2026-07-03T10:00:00Z",
            updatedAt: "2026-07-03T10:00:00Z", source: DomainContract.commentSourceMcp
        )
        XCTAssertTrue(mcp.isViaMcp)
    }
}
