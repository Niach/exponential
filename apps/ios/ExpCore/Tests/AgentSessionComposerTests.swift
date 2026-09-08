import Foundation
import XCTest

@testable import ExpCore

// EXP-788/790/796/784: the composer-facing rules the session model reads —
// which pending card the typed text answers and how, when "Load earlier" is
// offered, and the rate-limit banner's one line. Pure, so the model's socket
// never has to be driven to test them.
final class AgentSessionComposerTests: XCTestCase {

    // MARK: - Composer answer routing (EXP-788)

    func testPendingCardIsTheFirstActiveUnlockedQuestionInFeedOrder() {
        let feed: [AgentFeedItem] = [
            .tool(id: 1, name: "Edit", detail: "a.ts", subagentId: nil),
            .question(ask(2, askId: "ask", index: 1)),
            .question(ask(3, askId: "ask", index: 2)),
        ]
        let active = AgentFeed.activeQuestionIds(feed)
        // The ask's current step: the earliest unanswered one.
        XCTAssertEqual(
            AgentFeed.pendingCard(feed, active: active, isLocked: { _ in false })?.id, 2
        )
        // Step one answered (locked) → the composer targets step two.
        XCTAssertEqual(
            AgentFeed.pendingCard(feed, active: active, isLocked: { $0 == "q2" })?.id, 3
        )
        // Every step locked → nothing pending.
        XCTAssertNil(AgentFeed.pendingCard(feed, active: active, isLocked: { _ in true }))
        // Not active (resolved) → nothing pending.
        XCTAssertNil(AgentFeed.pendingCard(feed, active: [], isLocked: { _ in false }))
    }

    func testPlanCardRoutesAsADenyWithTheRejectKey() {
        let route = AgentFeed.composerAnswerRoute(for: plan(1))
        XCTAssertEqual(route, .plan(question: plan(1), rejectKey: "reject"))
        XCTAssertEqual(route?.placeholder, AgentFeed.planPendingPlaceholder)
        XCTAssertEqual(route?.question.id, 1)
    }

    func testPlanCardWithoutARejectIdFallsBackToItsLastOption() {
        // An older engine whose options carry no `reject` id: "No, keep
        // planning" is last by contract.
        let card = AgentQuestion(
            id: 4, wireId: "plan4", text: "# Plan",
            options: [
                AgentQuestionOption(label: "Yes", key: "1"),
                AgentQuestionOption(label: "No, keep planning", key: "3"),
            ],
            planMode: true
        )
        XCTAssertEqual(
            AgentFeed.composerAnswerRoute(for: card), .plan(question: card, rejectKey: "3")
        )
    }

    func testQuestionWithAFreeTextRowRoutesTheTextAsThatRowsAnswer() {
        let card = ask(5, askId: nil, index: nil)
        let route = AgentFeed.composerAnswerRoute(for: card)
        XCTAssertEqual(route, .freeText(question: card, key: "text"))
        XCTAssertEqual(route?.placeholder, AgentFeed.questionPendingPlaceholder)
    }

    func testFixedOptionCardsTakeNoFreeAnswer() {
        // A plain permission card: its options are the only answers, so the
        // composer's text goes out as an ordinary message.
        let permission = AgentQuestion(
            id: 6, wireId: "perm6", text: "Run `rm -rf build`?",
            options: [
                AgentQuestionOption(label: "Allow", key: "allow-once"),
                AgentQuestionOption(label: "Deny", key: "reject"),
            ]
        )
        XCTAssertNil(AgentFeed.composerAnswerRoute(for: permission))
        // Resolved and option-less cards route nowhere either.
        var resolved = plan(7)
        resolved.resolved = true
        XCTAssertNil(AgentFeed.composerAnswerRoute(for: resolved))
        XCTAssertNil(AgentFeed.composerAnswerRoute(for: AgentQuestion(
            id: 8, wireId: "q8", text: "?", options: [], planMode: true
        )))
    }

    func testComposerCopyIsByteLockedToTheWeb() {
        XCTAssertEqual(AgentFeed.submitLabel, "Submit")
        XCTAssertEqual(
            AgentFeed.planPendingPlaceholder,
            "Tell the agent what to change, or pick an option above"
        )
        XCTAssertEqual(
            AgentFeed.questionPendingPlaceholder,
            "Answer directly, or pick an option above"
        )
    }

    // MARK: - Rate-limit banner (EXP-784)

    func testRateLimitCaptionCarriesTheMessageAndALocalResetClock() {
        let utc = TimeZone(identifier: "UTC")!
        let vienna = TimeZone(identifier: "Europe/Vienna")!
        // 2026-09-09T10:05:00Z
        let resetsAt = 1_788_948_300_000
        let limit = AgentSessionRateLimit(
            status: "rejected", resetsAt: resetsAt, message: "Weekly limit reached"
        )
        XCTAssertEqual(
            AgentFeed.rateLimitCaption(limit, timeZone: utc),
            "Weekly limit reached · resets 10:05"
        )
        XCTAssertEqual(
            AgentFeed.rateLimitCaption(limit, timeZone: vienna),
            "Weekly limit reached · resets 12:05"
        )
        XCTAssertEqual(
            AgentFeed.rateLimitCaption(AgentSessionRateLimit(status: "rejected"), timeZone: utc),
            "Rate limited"
        )
        XCTAssertEqual(
            AgentFeed.rateLimitCaption(
                AgentSessionRateLimit(status: "rejected", resetsAt: resetsAt, message: ""),
                timeZone: utc
            ),
            "Rate limited · resets 10:05"
        )
    }

    // MARK: - Fixtures

    private func plan(_ id: Int) -> AgentQuestion {
        AgentQuestion(
            id: id,
            wireId: "plan\(id)",
            text: "# Plan\n\n- step one",
            options: [
                AgentQuestionOption(label: "Yes", key: "exit-plan-bypass"),
                AgentQuestionOption(
                    label: "Yes, and start with a fresh context", key: "exit-plan-clear-bypass"
                ),
                AgentQuestionOption(
                    label: "No, keep planning", key: "reject",
                    description: "Sends your next message back to planning"
                ),
            ],
            planMode: true
        )
    }

    private func ask(_ id: Int, askId: String?, index: Int?) -> AgentQuestion {
        AgentQuestion(
            id: id,
            wireId: "q\(id)",
            askId: askId,
            index: index,
            total: askId == nil ? nil : 2,
            text: "Which one?",
            options: [
                AgentQuestionOption(label: "Left", key: "1"),
                AgentQuestionOption(label: "Right", key: "2"),
                AgentQuestionOption(label: "Type something.", key: "text", freeText: true),
            ]
        )
    }
}
