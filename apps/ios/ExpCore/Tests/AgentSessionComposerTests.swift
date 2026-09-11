import Foundation
import XCTest

@testable import ExpCore

// EXP-820/796/784: the composer-facing rules the session model reads — which
// plan row expands into the inline feedback field and the card copy shared
// ×4, when "Load earlier" is offered, and the rate-limit banner's one line.
// Pure, so the model's socket never has to be driven to test them.
final class AgentSessionComposerTests: XCTestCase {

    // MARK: - Inline card answers (EXP-820)

    func testThePlanRejectRowIsTheRejectIdElseTheLastOption() {
        XCTAssertEqual(AgentFeed.planRejectKey(for: plan(1)), "reject")
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
        XCTAssertEqual(AgentFeed.planRejectKey(for: card), "3")
        // Only a plan has a reject row; an option-less plan has none.
        XCTAssertNil(AgentFeed.planRejectKey(for: ask(5, askId: nil, index: nil)))
        XCTAssertNil(AgentFeed.planRejectKey(for: AgentQuestion(
            id: 8, wireId: "q8", text: "?", options: [], planMode: true
        )))
    }

    func testTheComposerCopyIsByteLockedToTheWeb() {
        XCTAssertEqual(AgentFeed.submitLabel, "Submit")
        XCTAssertEqual(AgentFeed.composerPlaceholder, "Message the agent…")
        XCTAssertEqual(AgentFeed.freeTextPlaceholder, "Type your answer…")
        XCTAssertEqual(AgentFeed.planFeedbackPlaceholder, "Tell the agent what to change…")
        XCTAssertEqual(AgentFeed.backToCurrentStepLabel, "Back to current step")
    }

    // MARK: - Load earlier (EXP-796)

    func testRowsAlreadyHeldAreAlwaysPageable() {
        XCTAssertTrue(AgentFeed.canLoadEarlier(
            windowStart: 1, historyTruncated: false, historyExhausted: false, connected: false
        ))
    }

    func testADevicePageNeedsAnOpenSocket() {
        XCTAssertTrue(AgentFeed.canLoadEarlier(
            windowStart: 0, historyTruncated: true, historyExhausted: false, connected: true
        ))
        // The socket closed under the history room: nothing left to ask on.
        XCTAssertFalse(AgentFeed.canLoadEarlier(
            windowStart: 0, historyTruncated: true, historyExhausted: false, connected: false
        ))
        XCTAssertFalse(AgentFeed.canLoadEarlier(
            windowStart: 0, historyTruncated: true, historyExhausted: true, connected: true
        ))
        XCTAssertFalse(AgentFeed.canLoadEarlier(
            windowStart: 0, historyTruncated: false, historyExhausted: false, connected: true
        ))
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

    /// EXP-818/831: the banner's gate — a wall (a rejection or a notice,
    /// never a bare warning) whose reset is not more than a minute behind.
    func testRateLimitBannerGateIsAWallInsideItsWindow() {
        let now = Date(timeIntervalSince1970: 1_700_000_000)
        let nowMs = 1_700_000_000_000
        XCTAssertTrue(AgentFeed.rateLimitIsWall(AgentSessionRateLimit(status: "rejected")))
        XCTAssertTrue(
            AgentFeed.rateLimitIsWall(
                AgentSessionRateLimit(status: "allowed_warning", message: "You've hit your limit")
            )
        )
        XCTAssertFalse(AgentFeed.rateLimitIsWall(AgentSessionRateLimit(status: "allowed_warning")))
        XCTAssertFalse(
            AgentFeed.rateLimitIsWall(AgentSessionRateLimit(status: "allowed_warning", message: "  "))
        )

        let past = AgentSessionRateLimit(status: "rejected", resetsAt: nowMs - 61_000)
        let recent = AgentSessionRateLimit(status: "rejected", resetsAt: nowMs - 30_000)
        let ahead = AgentSessionRateLimit(status: "rejected", resetsAt: nowMs + 3_600_000)
        XCTAssertTrue(AgentFeed.rateLimitExpired(past, now: now))
        XCTAssertFalse(AgentFeed.rateLimitExpired(recent, now: now))
        XCTAssertFalse(AgentFeed.rateLimitExpired(ahead, now: now))
        XCTAssertFalse(AgentFeed.rateLimitExpired(AgentSessionRateLimit(status: "rejected"), now: now))

        XCTAssertFalse(AgentFeed.rateLimitBannerShows(past, now: now))
        XCTAssertTrue(AgentFeed.rateLimitBannerShows(recent, now: now))
        XCTAssertTrue(AgentFeed.rateLimitBannerShows(AgentSessionRateLimit(status: "rejected"), now: now))
        XCTAssertFalse(
            AgentFeed.rateLimitBannerShows(AgentSessionRateLimit(status: "allowed_warning"), now: now)
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
