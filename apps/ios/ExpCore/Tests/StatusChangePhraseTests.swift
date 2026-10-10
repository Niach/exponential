import XCTest
@testable import ExpCore

// Polish round (pinned ×4): `changed status from {from} to {to}` with the team
// status rows' display names — web `issue-event-labels.ts` twin.
final class StatusChangePhraseTests: XCTestCase {
    func testPayloadNamesWin() {
        XCTAssertEqual(
            StatusChangePhrase.phrase(fromName: "Backlog", fromAnchor: "backlog", toName: "In QA", toAnchor: "in_progress"),
            "changed status from Backlog to In QA"
        )
    }

    func testLegacyAnchorsReadTheBuiltinDisplayNames() {
        XCTAssertEqual(
            StatusChangePhrase.phrase(fromName: nil, fromAnchor: "backlog", toName: nil, toAnchor: "in_progress"),
            "changed status from Backlog to In Progress"
        )
    }

    func testRetiredTodoKeepsItsLabel() {
        XCTAssertEqual(
            StatusChangePhrase.phrase(fromName: nil, fromAnchor: "todo", toName: nil, toAnchor: "done"),
            "changed status from Todo to Done"
        )
    }

    func testMissingSides() {
        XCTAssertEqual(
            StatusChangePhrase.phrase(fromName: nil, fromAnchor: nil, toName: "Done", toAnchor: "done"),
            "changed status to Done"
        )
        XCTAssertEqual(
            StatusChangePhrase.phrase(fromName: nil, fromAnchor: nil, toName: nil, toAnchor: nil),
            "changed status"
        )
    }
}
