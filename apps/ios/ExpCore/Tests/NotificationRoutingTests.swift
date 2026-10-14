import Foundation
import XCTest

@testable import ExpCore

// EXP-933: push taps and inbox rows route through one rule.
final class NotificationRoutingTests: XCTestCase {

    func testATargetedAgentMessageOpensTheIssue() {
        XCTAssertEqual(
            NotificationRouting.pushTarget([
                "type": "agent_message",
                "teamId": "t1",
                "issueId": "i1",
                "identifier": "EXP-1",
                "notificationId": "n1",
            ]),
            .issue(id: "i1")
        )
    }

    func testAnIssueLessAgentMessageOpensTheInbox() {
        XCTAssertEqual(NotificationRouting.pushTarget(["type": "agent_message"]), .inbox)
        XCTAssertEqual(
            NotificationRouting.pushTarget(["type": "agent_message", "issueId": ""]), .inbox
        )
    }

    func testKeepsTheOtherPushKinds() {
        // SLOP-4: a reporter's reply is issue-scoped, like a comment.
        XCTAssertEqual(
            NotificationRouting.pushTarget(["type": "reporter_reply", "issueId": "i1"]),
            .issue(id: "i1")
        )
        XCTAssertEqual(
            NotificationRouting.pushTarget(["type": "session_blocked", "sessionId": "s1"]),
            .session(id: "s1")
        )
        XCTAssertEqual(NotificationRouting.pushTarget(["type": "session_blocked"]), .inbox)
        XCTAssertEqual(
            NotificationRouting.pushTarget(["type": "issue_assigned", "issueId": "i1"]),
            .issue(id: "i1")
        )
        XCTAssertEqual(NotificationRouting.pushTarget(["type": "issue_assigned"]), .none)
        XCTAssertEqual(NotificationRouting.pushTarget([:]), .none)
    }
}
