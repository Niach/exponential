import Foundation
import XCTest

@testable import ExpCore

// waiting-cards.json DEEP LINKS: an inbox row or a push tap lands on the card
// it is about (the push's `messageId`, else the session's waiting card), the
// chat's pending deep link, not on the session's newest rows.
final class InboxCardLandingTests: XCTestCase {

    private func message(_ index: Int, card: TreeMessageCard? = nil) -> TreeMessage {
        TreeMessage(
            id: "m\(index)", sessionId: "root", authorKind: "agent", authorId: nil,
            body: "row \(index)", card: card, answeredAt: nil, consumedAt: nil,
            createdAt: String(format: "2026-11-13T00:%02d:%02d.000Z", index / 60, index % 60)
        )
    }

    func testAnInboxRowLandsOnAWaitingCardOutsideTheNewest200Rows() {
        // 250 messages, the waiting question is the OLDEST: outside the
        // chat's newest-200 window, still the session's waiting card.
        let question = TreeMessageCard(object: ["kind": "question", "question": "Ship it?"])
        let messages = (0..<250).map { message($0, card: $0 == 10 ? question : nil) }
        let card = ChatRowAction.sessionWaitingCard(sessionId: "root", messages: messages)
        XCTAssertEqual(card, "m10")
        XCTAssertEqual(
            InboxRoute.landing(target: .chat(chatId: "root", sessionId: nil), rootId: "root", messageId: card, plan: false),
            .card(rootId: "root", messageId: "m10")
        )

        // The chat takes it as THE pending deep link: one older page, then
        // the card is loaded, laid out, centred and rung.
        var link = WaitingCards.DeepLink(target: "m10", takenAtMs: 0)
        var loadedFrom = 250 - 200
        var distance: Double?
        var actions: [WaitingCards.DeepLink.Action] = []
        var now = 0.0
        while !link.settled, now < 20_000 {
            now += 16
            let found = 10 >= loadedFrom
            let action = link.step(.init(found: found, distance: found ? distance : nil, more: loadedFrom > 0), nowMs: now)
            if action != .none, actions.last != action { actions.append(action) }
            switch action {
            case .page:
                loadedFrom = max(0, loadedFrom - 200)
                link.pageLanded()
            case .reveal: distance = 400
            case .scroll: distance = 0
            default: break
            }
        }
        XCTAssertEqual(actions, [.page, .reveal, .scroll, .ring])
    }

    func testAQuestionPushCarriesItsCard() {
        let target = NotificationRouting.pushTarget([
            "type": "agent_message", "sessionId": "root", "rootId": "root",
            "messageId": "m1", "cardKind": "question",
        ])
        let card = NotificationRouting.Card(messageId: "m1", rootId: "root", plan: false)
        XCTAssertEqual(target, .session(id: "root", card: card))
        // Not synced yet (a cold tap): the push's own rootId still lands it.
        XCTAssertEqual(
            InboxRoute.landing(target: nil, rootId: card.rootId, messageId: card.messageId, plan: card.plan),
            .card(rootId: "root", messageId: "m1")
        )
    }

    func testAPlanPushOpensThePlanPage() {
        let target = NotificationRouting.pushTarget([
            "type": "agent_message", "sessionId": "child", "rootId": "root",
            "messageId": "p1", "cardKind": "plan",
        ])
        XCTAssertEqual(target, .session(id: "child", card: .init(messageId: "p1", rootId: "root", plan: true)))
        XCTAssertEqual(
            InboxRoute.landing(target: .chat(chatId: "root", sessionId: "child"), rootId: "root", messageId: "p1", plan: true),
            .plan(rootId: "root", messageId: "p1")
        )
    }

    func testNoCardNoRootOrASessionOnItsOwnScreenOpensTheSession() {
        XCTAssertEqual(NotificationRouting.pushTarget(["type": "agent_message", "sessionId": "s1"]), .session(id: "s1"))
        XCTAssertEqual(InboxRoute.landing(target: .chat(chatId: "root", sessionId: nil), rootId: "root", messageId: nil, plan: false), .session)
        XCTAssertEqual(InboxRoute.landing(target: nil, rootId: nil, messageId: "m1", plan: false), .session)
        XCTAssertEqual(InboxRoute.landing(target: .session(id: "s1"), rootId: "root", messageId: "m1", plan: false), .session)
    }
}
