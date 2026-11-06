import ExpPush
import Foundation
import UserNotifications
import XCTest

// #EXP-1641: notification-actions.json replayed ×3 (TS
// `notification-actions.test.ts`, Android `NotificationActionsTest`).
final class NotificationActionsTests: XCTestCase {
    private func fixture() throws -> [String: Any] {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("packages/domain-contract/fixtures/notification-actions.json")
        return try XCTUnwrap(try JSONSerialization.jsonObject(with: try Data(contentsOf: url)) as? [String: Any])
    }

    func testConstantsMatchTheFixture() throws {
        let f = try fixture()
        let copy = try XCTUnwrap(f["copy"] as? [String: String])
        XCTAssertEqual(copy["approve"], NotificationActions.Copy.approve)
        XCTAssertEqual(copy["requestChanges"], NotificationActions.Copy.requestChanges)
        XCTAssertEqual(copy["requestChangesPlaceholder"], NotificationActions.Copy.requestChangesPlaceholder)
        XCTAssertEqual(copy["answer"], NotificationActions.Copy.answer)
        XCTAssertEqual(copy["answerPlaceholder"], NotificationActions.Copy.answerPlaceholder)
        XCTAssertEqual(copy["reply"], NotificationActions.Copy.reply)
        XCTAssertEqual(copy["replyPlaceholder"], NotificationActions.Copy.replyPlaceholder)
        XCTAssertEqual(copy["send"], NotificationActions.Copy.send)
        XCTAssertEqual(copy["open"], NotificationActions.Copy.open)
        XCTAssertEqual(copy["failed"], NotificationActions.Copy.failed)
        let keys = try XCTUnwrap(f["dataKeys"] as? [String: String])
        XCTAssertEqual(keys["messageId"], NotificationActions.DataKeys.messageId)
        XCTAssertEqual(keys["cardKind"], NotificationActions.DataKeys.cardKind)
        XCTAssertEqual(keys["options"], NotificationActions.DataKeys.options)
        XCTAssertEqual(keys["custom"], NotificationActions.DataKeys.custom)
        XCTAssertEqual(keys["planQuestions"], NotificationActions.DataKeys.planQuestions)
        XCTAssertEqual(keys["sealed"], NotificationActions.DataKeys.sealed)
        XCTAssertEqual(f["maxOptionButtons"] as? Int, NotificationActions.maxOptionButtons)
    }

    func testReplaysEveryActionCase() throws {
        let cases = try XCTUnwrap(try fixture()["actionCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let input = try XCTUnwrap(c["input"] as? [String: Any])
            let actions = NotificationActions.notificationActions(
                data: try XCTUnwrap(input["data"] as? [String: String]),
                keyHeld: try XCTUnwrap(input["keyHeld"] as? Bool),
                openedAskText: input["openedAskText"] as? String
            )
            let expected = try XCTUnwrap(c["expected"] as? [[String: String]]).map {
                NotificationActions.Action(
                    id: $0["id"] ?? "",
                    kind: NotificationActions.Kind(rawValue: $0["kind"] ?? "") ?? .button,
                    title: $0["title"] ?? ""
                )
            }
            XCTAssertEqual(actions, expected, name)
        }
    }

    func testReplaysEveryOptionCase() throws {
        let cases = try XCTUnwrap(try fixture()["optionCases"] as? [[String: Any]])
        XCTAssertFalse(cases.isEmpty)
        for c in cases {
            let name = try XCTUnwrap(c["name"] as? String)
            let expected = try XCTUnwrap(c["expected"] as? [String: Any])
            let parsed = NotificationActions.askTextOptions(try XCTUnwrap(c["input"] as? String))
            XCTAssertEqual(parsed.options, expected["options"] as? [String], name)
            XCTAssertEqual(parsed.custom, expected["custom"] as? Bool, name)
        }
    }

    func testRouteNeverSendsASealedPushInClear() {
        typealias N = NotificationActions
        XCTAssertEqual(N.route(actionId: N.reply, sealed: false, rootId: nil, cardLocal: false), .send)
        XCTAssertEqual(N.route(actionId: N.reply, sealed: true, rootId: "r", cardLocal: false), .sendSealed(rootId: "r"))
        XCTAssertEqual(N.route(actionId: N.reply, sealed: true, rootId: nil, cardLocal: false), .refuse)
        XCTAssertEqual(N.route(actionId: N.approve, sealed: true, rootId: "r", cardLocal: false), .fetchThenAnswer)
        XCTAssertEqual(N.route(actionId: "option:1", sealed: true, rootId: "r", cardLocal: true), .answer)
        XCTAssertEqual(N.route(actionId: N.answer, sealed: false, rootId: nil, cardLocal: false), .answer)
        XCTAssertEqual(N.route(actionId: N.open, sealed: false, rootId: nil, cardLocal: false), .refuse)
    }

    func testCategoryOptionsAndIdentity() {
        typealias N = NotificationActions
        let plan = N.notificationActions(data: ["sessionId": "s", "messageId": "m", "cardKind": "plan"], keyHeld: false, openedAskText: nil)
        let category = N.category(plan)
        XCTAssertEqual(category.actions.map(\.identifier), [N.approve, N.requestChanges, N.open])
        XCTAssertEqual(category.actions[0].options, [.authenticationRequired])
        XCTAssertTrue(category.actions[1] is UNTextInputNotificationAction)
        XCTAssertEqual(category.actions[1].options, [.authenticationRequired])
        XCTAssertEqual((category.actions[1] as? UNTextInputNotificationAction)?.textInputButtonTitle, N.Copy.send)
        XCTAssertEqual((category.actions[1] as? UNTextInputNotificationAction)?.textInputPlaceholder, N.Copy.requestChangesPlaceholder)
        XCTAssertEqual(category.actions[2].options, [.foreground])
        XCTAssertEqual(N.categoryIdentifier(plan), N.categoryIdentifier(plan))
        let a = N.notificationActions(data: ["sessionId": "s", "messageId": "m", "cardKind": "question", "options": #"["A","B"]"#, "custom": "1"], keyHeld: false, openedAskText: nil)
        let b = N.notificationActions(data: ["sessionId": "s", "messageId": "m", "cardKind": "question", "options": #"["A","C"]"#, "custom": "1"], keyHeld: false, openedAskText: nil)
        XCTAssertNotEqual(N.categoryIdentifier(a), N.categoryIdentifier(b))
        XCTAssertTrue(N.categoryIdentifier(a).hasPrefix(N.categoryPrefix))
    }

    func testMergedCapsOurCategoriesAndKeepsOthers() {
        typealias N = NotificationActions
        let other = UNNotificationCategory(identifier: "other", actions: [], intentIdentifiers: [])
        var set: [UNNotificationCategory] = [other]
        for i in 0..<30 {
            let actions = [N.Action(id: "option:0", kind: .button, title: "L\(i)")]
            set = Array(N.merged(set, adding: N.category(actions), keep: 5))
        }
        XCTAssertTrue(set.contains { $0.identifier == "other" })
        XCTAssertEqual(set.filter { $0.identifier.hasPrefix(N.categoryPrefix) }.count, 5)
    }
}
