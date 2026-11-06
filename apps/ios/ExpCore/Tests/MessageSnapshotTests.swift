import Foundation
import XCTest
@testable import ExpCore

// #EXP-1641: the on-demand message read a notification action falls back to
// (Android `MessageSnapshotTest`): a subset snapshot's inserts decode as the
// shape loop decodes them; anything else is skipped.
final class MessageSnapshotTests: XCTestCase {
    private let row = """
        {"id":"m1","conversation_id":"c-s2","root_id":"s1","team_id":"ws1",
         "author_kind":"agent","author_id":null,"body":"",
         "card":{"kind":"question","question":"Which board?","options":["A","B"]},
         "answered_at":null,"consumed_at":null,
         "created_at":"2026-10-13T09:00:00Z","updated_at":"2026-10-13T09:00:00Z"}
        """

    func testRowsReadsTheInsertsOfASubsetSnapshot() throws {
        let body = #"{"metadata":{},"data":[{"key":"k","headers":{"operation":"insert"},"value":\#(row)},{"headers":{"control":"snapshot-end"}}]}"#
        let rows = try MessageSnapshot.rows(Data(body.utf8))
        XCTAssertEqual(rows.map(\.id), ["m1"])
        XCTAssertTrue(rows.first?.card?.contains("Which board?") ?? false)
    }

    func testRowsReadsABareArrayAndSkipsDeletes() throws {
        let body = #"[{"key":"k","headers":{"operation":"delete"},"value":\#(row)},{"key":"k","value":\#(row)}]"#
        XCTAssertEqual(try MessageSnapshot.rows(Data(body.utf8)).map(\.id), ["m1"])
    }

    func testRowsRefusesABodyThatIsNoSnapshot() {
        XCTAssertThrowsError(try MessageSnapshot.rows(Data(#"{"error":"x"}"#.utf8)))
    }
}
