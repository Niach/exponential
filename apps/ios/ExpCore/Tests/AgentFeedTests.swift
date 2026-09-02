import XCTest

@testable import ExpCore

// EXP-78/EXP-197 + steer protocol v2 (EXP-249, the only wire since EXP-613).
// Mirrors the Android AgentFeedTest: the semantic resolution path, the wire
// folds (resolve/upsert/splice), the render grouping, and the per-card answer
// lock.
final class AgentFeedTests: XCTestCase {

    // MARK: - activeQuestionIds

    func testHandlesAnAllQuestionFeedAndAnEmptyFeed() {
        XCTAssertEqual(
            AgentFeed.activeQuestionIds([.question(question(1)), .question(question(2))]),
            [1, 2]
        )
        XCTAssertEqual(AgentFeed.activeQuestionIds([]), [])
    }

    func testQuestionsAreUnaffectedByToolRunsBeforeThem() {
        let feed: [AgentFeedItem] = [tool(1), tool(2), .question(question(3))]
        XCTAssertEqual(AgentFeed.activeQuestionIds(feed), [3])
    }

    func testPlanQuestionStaysActiveBehindLaggedToolAndNarrationFlushes() {
        let feed: [AgentFeedItem] = [
            .question(plan(1)),
            tool(2),
            .narration(id: 3, text: "Let me finalize the plan file:"),
        ]
        XCTAssertEqual(AgentFeed.activeQuestionIds(feed), [1])
    }

    func testPlanQuestionSurvivesAHumanMessage() {
        // Steering a message mid-plan leaves the picker up (EXP-249, web
        // parity) — only `question_resolved` retires a card.
        let feed: [AgentFeedItem] = [
            .question(plan(1)), tool(2), .userMessage(id: 3, text: "1"),
        ]
        XCTAssertEqual(AgentFeed.activeQuestionIds(feed), [1])
    }

    func testResolvedQuestionIsNeverActive() {
        var answered = question(1)
        answered.resolved = true
        answered.answers = ["Red"]
        XCTAssertEqual(AgentFeed.activeQuestionIds([.question(answered)]), [])
    }

    func testSemanticQuestionStaysActiveUntilItIsResolved() {
        let feed: [AgentFeedItem] = [
            .question(question(1, wireId: "tu_1")),
            tool(2),
            .narration(id: 3, text: "still working"),
        ]
        XCTAssertEqual(AgentFeed.activeQuestionIds(feed), [1])

        var resolved = question(1, wireId: "tu_1")
        resolved.resolved = true
        XCTAssertEqual(
            AgentFeed.activeQuestionIds([.question(resolved), tool(2)]),
            []
        )
    }

    func testEverySemanticStepOfAnAskIsActive() {
        let feed: [AgentFeedItem] = [
            .question(question(1, wireId: "tu#0", askId: "tu", index: 1, total: 2)),
            .question(question(2, wireId: "tu#1", askId: "tu", index: 2, total: 2)),
        ]
        XCTAssertEqual(AgentFeed.activeQuestionIds(feed), [1, 2])
    }

    // MARK: - question_resolved

    func testResolvesASingleCardByIdAndFoldsAllItsAnswersIn() {
        let feed: [AgentFeedItem] = [
            .question(question(1, wireId: "a")),
            .question(question(2, wireId: "b")),
        ]
        let out = AgentFeed.applyQuestionResolved(
            feed, id: "b", askId: nil, answers: ["Blue", "Green"]
        )
        XCTAssertEqual(out?[0].question?.resolved, false)
        XCTAssertEqual(out?[1].question?.resolved, true)
        XCTAssertEqual(out?[1].question?.answers, ["Blue", "Green"])
        XCTAssertEqual(out?[1].question?.answerSummary, "Blue, Green")
        XCTAssertNil(AgentFeed.applyQuestionResolved(feed, id: "missing", askId: nil))
    }

    func testResolvesEveryCardOfAnAskAndMapsAnswersOntoTheAnsweringSteps() {
        let feed: [AgentFeedItem] = [
            .question(question(1, wireId: "tu#0", askId: "tu", index: 1, total: 2)),
            // The submit step consumes none of the ask's answers.
            .question(question(2, wireId: "tu#submit", askId: "tu")),
            .question(question(3, wireId: "tu#1", askId: "tu", index: 2, total: 2)),
            .question(question(4, wireId: "other")),
        ]
        let out = AgentFeed.applyQuestionResolved(
            feed, id: nil, askId: "tu", answers: ["Red", "Blue"]
        )
        XCTAssertEqual(out?[0].question?.answers, ["Red"])
        XCTAssertEqual(out?[1].question?.answers, [])
        XCTAssertEqual(out?[1].question?.resolved, true)
        XCTAssertEqual(out?[2].question?.answers, ["Blue"])
        XCTAssertEqual(out?[3].question?.resolved, false)
    }

    func testDismissalCarriesNoAnswersAndWithoutIdsRetiresEveryUnresolvedCard() {
        let feed: [AgentFeedItem] = [
            .question(question(1, wireId: "a")),
            .question(plan(2)),
        ]
        let out = AgentFeed.applyQuestionResolved(
            feed, id: nil, askId: nil, answers: ["Red"], dismissed: true
        )
        XCTAssertEqual(out?[0].question?.dismissed, true)
        XCTAssertEqual(out?[0].question?.answers, [])
        XCTAssertEqual(out?[1].question?.dismissed, true)
    }

    // MARK: - upsertQuestion

    func testUpsertReplacesTheCardCarryingTheSameWireId() {
        let feed = AgentFeed.upsertQuestion([], question: question(1, wireId: "a"))
        let augmented = AgentQuestion(
            id: 7,
            wireId: "a",
            text: "Which color?",
            options: [
                AgentQuestionOption(label: "Red", key: "1"),
                AgentQuestionOption(label: "Type something", key: "2"),
            ]
        )
        let out = AgentFeed.upsertQuestion(feed, question: augmented)
        XCTAssertEqual(out.count, 1)
        // The render id is kept so the card is replaced, not re-created.
        XCTAssertEqual(out[0].id, 1)
        XCTAssertEqual(out[0].question?.options.count, 2)
    }

    func testUpsertKeepsAResolutionTheCardAlreadyHad() {
        var resolved = question(1, wireId: "a")
        resolved.resolved = true
        resolved.answers = ["Red"]
        let out = AgentFeed.upsertQuestion(
            [.question(resolved)], question: question(2, wireId: "a")
        )
        XCTAssertEqual(out[0].question?.resolved, true)
        XCTAssertEqual(out[0].question?.answers, ["Red"])
    }

    func testUpsertAppendsLegacyCardsAndUnknownWireIds() {
        var feed = AgentFeed.upsertQuestion([], question: question(1))
        feed = AgentFeed.upsertQuestion(feed, question: question(2))
        feed = AgentFeed.upsertQuestion(feed, question: question(3, wireId: "a"))
        XCTAssertEqual(feed.map(\.id), [1, 2, 3])
    }

    // MARK: - spliceBeforeQuestion (EXP-483)

    func testAnchoredNarrationSplicesAboveTheFirstCardOfItsAsk() {
        let feed: [AgentFeedItem] = [
            .narration(id: 1, text: "working"),
            .question(question(2, wireId: "tu_1#0", askId: "tu_1", index: 1, total: 2)),
            .question(question(3, wireId: "tu_1#1", askId: "tu_1", index: 2, total: 2)),
        ]
        let out = AgentFeed.spliceBeforeQuestion(
            feed, anchor: "tu_1", item: .narration(id: 4, text: "summary")
        )
        XCTAssertEqual(out?.map(\.id), [1, 4, 2, 3])
    }

    func testAnchoredNarrationMatchesAPlanCardByWireIdResolvedOrNot() {
        var planCard = question(1, wireId: "tu_plan")
        planCard.resolved = true
        let out = AgentFeed.spliceBeforeQuestion(
            [.question(planCard)], anchor: "tu_plan",
            item: .narration(id: 2, text: "plan prose")
        )
        XCTAssertEqual(out?.map(\.id), [2, 1])
    }

    func testSuccessiveAnchoredNarrationsKeepTheirOrder() {
        let feed: [AgentFeedItem] = [.question(question(1, wireId: "tu_1#0", askId: "tu_1"))]
        let once = AgentFeed.spliceBeforeQuestion(
            feed, anchor: "tu_1", item: .narration(id: 2, text: "first")
        )!
        let twice = AgentFeed.spliceBeforeQuestion(
            once, anchor: "tu_1", item: .narration(id: 3, text: "second")
        )
        XCTAssertEqual(twice?.map(\.id), [2, 3, 1])
    }

    func testSpliceIsNilWhenNoCardMatchesSoTheCallerAppends() {
        XCTAssertNil(AgentFeed.spliceBeforeQuestion(
            [.narration(id: 1, text: "working")], anchor: "tu_gone",
            item: .narration(id: 2, text: "late")
        ))
    }

    // MARK: - rows

    func testCollapsesRunsOfTwoOrMoreConsecutiveTools() {
        let feed: [AgentFeedItem] = [
            .narration(id: 1, text: "working"),
            tool(2), tool(3), tool(4),
            .userMessage(id: 5, text: "hi"),
            tool(6),
        ]
        XCTAssertEqual(
            AgentFeed.rows(feed),
            [
                .single(feed[0]),
                .toolRun([feed[1], feed[2], feed[3]]),
                .single(feed[4]),
                .single(feed[5]),
            ]
        )
    }

    func testALoneToolBetweenOtherKindsStaysASingleRow() {
        let feed: [AgentFeedItem] = [tool(1), .narration(id: 2, text: "x"), tool(3)]
        XCTAssertEqual(AgentFeed.rows(feed), feed.map { AgentFeedRow.single($0) })
    }

    func testTwoRunsSplitByANarrationStaySeparateRuns() {
        let feed: [AgentFeedItem] = [
            tool(1), tool(2), .narration(id: 3, text: "x"), tool(4), tool(5),
        ]
        XCTAssertEqual(
            AgentFeed.rows(feed),
            [
                .toolRun([feed[0], feed[1]]),
                .single(feed[2]),
                .toolRun([feed[3], feed[4]]),
            ]
        )
    }

    func testAnAllToolFeedIsOneRunAndAnEmptyFeedHasNoRows() {
        let feed: [AgentFeedItem] = [tool(1), tool(2), tool(3)]
        XCTAssertEqual(AgentFeed.rows(feed), [.toolRun(feed)])
        XCTAssertEqual(AgentFeed.rows([]), [])
    }

    func testARunIdStaysTheFirstToolsIdAsTheTrailingRunGrows() {
        let feed: [AgentFeedItem] = [.narration(id: 1, text: "x"), tool(2), tool(3)]
        XCTAssertEqual(AgentFeed.rows(feed)[1].id, 2)
        XCTAssertEqual(AgentFeed.rows(feed + [tool(4)])[1].id, 2)
    }

    func testAToolTaggedWithASubagentNeverJoinsAMainThreadRun() {
        let feed: [AgentFeedItem] = [tool(1), tool(2, subagentId: "s1")]
        let rows = AgentFeed.rows(feed)
        XCTAssertEqual(rows.count, 2)
        XCTAssertEqual(rows[0], .single(feed[0]))
        guard case let .subagentRun(run) = rows[1] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertEqual(run.toolCount, 1)
    }

    func testQuestionsAdjacentToToolsAreNeverAbsorbedIntoARun() {
        let feed: [AgentFeedItem] = [
            tool(1), tool(2), .question(question(3)), .question(question(4)),
        ]
        XCTAssertEqual(
            AgentFeed.rows(feed),
            [.toolRun([feed[0], feed[1]]), .single(feed[2]), .single(feed[3])]
        )
    }

    func testGroupsASubagentRunUnderItsStartMarker() {
        let feed: [AgentFeedItem] = [
            .subagent(
                id: 1, subagentId: "s1", agentType: "explorer",
                status: .started, detail: "map the repo"
            ),
            tool(2, subagentId: "s1"),
            tool(3, subagentId: "s1"),
            .subagent(id: 4, subagentId: "s1", agentType: "explorer", status: .completed, detail: nil),
            tool(5),
        ]
        let rows = AgentFeed.rows(feed)
        XCTAssertEqual(rows.count, 2)
        guard case let .subagentRun(run) = rows[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertEqual(run.id, 1)
        XCTAssertEqual(run.agentType, "explorer")
        XCTAssertEqual(run.toolCount, 2)
        XCTAssertTrue(run.done)
        XCTAssertEqual(rows[1], .single(feed[4]))
    }

    func testAnUnfinishedSubagentRunIsNotDoneAndAStrayMarkerStillOpensItsGroup() {
        let running: [AgentFeedItem] = [
            .subagent(id: 1, subagentId: "s1", agentType: "explorer", status: .started, detail: nil),
            tool(2, subagentId: "s1"),
        ]
        guard case let .subagentRun(run) = AgentFeed.rows(running)[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertFalse(run.done)

        // A `completed` whose `started` fell off the top of the feed.
        let stray: [AgentFeedItem] = [
            .subagent(id: 9, subagentId: "s9", agentType: "explorer", status: .completed, detail: nil),
        ]
        guard case let .subagentRun(orphan) = AgentFeed.rows(stray)[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertTrue(orphan.done)
        XCTAssertEqual(orphan.toolCount, 0)
        XCTAssertEqual(orphan.id, 9)
    }

    func testSubagentsListsEveryRunInFirstAppearanceOrder() {
        // EXP-356: one conversation tab per subagent, first-appearance order,
        // summarized exactly like the group rows.
        let feed: [AgentFeedItem] = [
            .narration(id: 1, text: "Delegating."),
            .subagent(id: 2, subagentId: "a", agentType: "Explore", status: .started, detail: "map"),
            tool(3, subagentId: "a"),
            tool(4),
            .subagent(id: 5, subagentId: "b", agentType: "review", status: .started, detail: nil),
            tool(6, subagentId: "a"),
            .subagent(id: 7, subagentId: "a", agentType: "Explore", status: .completed, detail: "map"),
        ]
        let agents = AgentFeed.subagents(feed)
        XCTAssertEqual(agents.map(\.subagentId), ["a", "b"])
        XCTAssertEqual(agents[0].agentType, "Explore")
        XCTAssertTrue(agents[0].done)
        XCTAssertEqual(agents[0].toolCount, 2)
        XCTAssertEqual(agents[1].agentType, "review")
        XCTAssertFalse(agents[1].done)
        XCTAssertEqual(agents[1].toolCount, 0)
        XCTAssertEqual(AgentFeed.subagents([tool(1), .narration(id: 2, text: "x")]), [])
    }

    func testVisibleTabsDropCompletedRunsExceptTheFocusedOne() {
        // EXP-387: the strip shows running subagents only — a completed run's
        // tab is dropped, unless it is the focused one (never yank the user
        // out mid-read); all-done with Main focused leaves the strip empty.
        let feed: [AgentFeedItem] = [
            .subagent(id: 1, subagentId: "a", agentType: "Explore", status: .started, detail: nil),
            .subagent(id: 2, subagentId: "b", agentType: "review", status: .started, detail: nil),
            .subagent(id: 3, subagentId: "a", agentType: "Explore", status: .completed, detail: nil),
        ]
        let agents = AgentFeed.subagents(feed)
        XCTAssertEqual(AgentFeed.visibleSubagentTabs(agents, selected: nil).map(\.subagentId), ["b"])
        XCTAssertEqual(AgentFeed.visibleSubagentTabs(agents, selected: "a").map(\.subagentId), ["a", "b"])
        XCTAssertEqual(AgentFeed.visibleSubagentTabs(agents, selected: "b").map(\.subagentId), ["b"])

        let done: [AgentFeedItem] = [
            .subagent(id: 1, subagentId: "a", agentType: "Explore", status: .completed, detail: nil),
        ]
        XCTAssertEqual(AgentFeed.visibleSubagentTabs(AgentFeed.subagents(done), selected: nil), [])
    }

    func testAFallbackTypedCompletedEdgeNeverDegradesTheLabel() {
        // Old desktops stamp the fallback "agent" onto the completed edge
        // (claude's SubagentStop hook carries no agent_type) — the started
        // marker's real type must win (EXP-350).
        let feed: [AgentFeedItem] = [
            .subagent(id: 1, subagentId: "s1", agentType: "explore", status: .started, detail: "map"),
            tool(2, subagentId: "s1"),
            .subagent(id: 3, subagentId: "s1", agentType: "agent", status: .completed, detail: nil),
        ]
        guard case let .subagentRun(run) = AgentFeed.rows(feed)[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertEqual(run.agentType, "explore")
        XCTAssertTrue(run.done)
        XCTAssertEqual(run.detail, "map")
    }

    func testACompletedOnlyMarkerKeepsItsRealTypeAndAnHonestAgentStaysAgent() {
        let typed: [AgentFeedItem] = [
            .subagent(id: 1, subagentId: "s1", agentType: "review", status: .completed, detail: nil),
        ]
        guard case let .subagentRun(run) = AgentFeed.rows(typed)[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertEqual(run.agentType, "review")

        let fallback: [AgentFeedItem] = [
            .subagent(id: 1, subagentId: "s1", agentType: "agent", status: .completed, detail: nil),
        ]
        guard case let .subagentRun(bare) = AgentFeed.rows(fallback)[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertEqual(bare.agentType, "agent")
    }

    func testASubagentRunIsExpandableOnlyOnceItHasToolCalls() {
        let markerOnly: [AgentFeedItem] = [
            .subagent(id: 1, subagentId: "s1", agentType: "explore", status: .started, detail: "map"),
        ]
        guard case let .subagentRun(bare) = AgentFeed.rows(markerOnly)[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertFalse(bare.expandable)

        guard case let .subagentRun(working) = AgentFeed.rows(markerOnly + [tool(2, subagentId: "s1")])[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertTrue(working.expandable)
    }

    func testGroupedItemsJoinTheirRowEvenWhenSomethingElseLandsBetween() {
        let feed: [AgentFeedItem] = [
            .subagent(id: 1, subagentId: "s1", agentType: "explorer", status: .started, detail: nil),
            .question(question(2, wireId: "tu#0", askId: "tu", index: 1, total: 2)),
            .narration(id: 3, text: "thinking"),
            tool(4, subagentId: "s1"),
            .question(question(5, wireId: "tu#1", askId: "tu", index: 2, total: 2)),
        ]
        let rows = AgentFeed.rows(feed)
        XCTAssertEqual(rows.count, 3)
        guard case let .subagentRun(run) = rows[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertEqual(run.toolCount, 1)
        guard case let .ask(group) = rows[1] else { return XCTFail("expected an ask group") }
        XCTAssertEqual(group.questions.map(\.id), [2, 5])
        XCTAssertEqual(rows[2], .single(feed[2]))
    }

    func testGroupsTheStepsOfOneAskWithTheSubmitStepLast() {
        let feed: [AgentFeedItem] = [
            .question(question(1, wireId: "tu#0", askId: "tu", index: 1, total: 2)),
            .question(question(2, wireId: "tu#submit", askId: "tu")),
            .question(question(3, wireId: "tu#1", askId: "tu", index: 2, total: 2)),
            .question(question(4, wireId: "other")),
        ]
        let rows = AgentFeed.rows(feed)
        XCTAssertEqual(rows.count, 2)
        guard case let .ask(group) = rows[0] else { return XCTFail("expected an ask group") }
        XCTAssertEqual(group.askId, "tu")
        XCTAssertEqual(group.questions.map(\.id), [1, 3, 2])
        XCTAssertEqual(group.stepCount, 2)
        XCTAssertEqual(group.id, 1)
        XCTAssertEqual(rows[1], .single(feed[3]))
    }

    // MARK: - Stepper progression

    func testTheCurrentStepAdvancesOnAcknowledgementAndEndsWhenResolved() {
        var second = question(2, wireId: "tu#1", askId: "tu", index: 2, total: 2)
        let group = AgentAskGroup(
            askId: "tu",
            questions: [question(1, wireId: "tu#0", askId: "tu", index: 1, total: 2), second]
        )
        XCTAssertEqual(AgentFeed.currentStepIndex(of: group, done: []), 0)
        XCTAssertEqual(AgentFeed.currentStepIndex(of: group, done: ["tu#0"]), 1)
        XCTAssertNil(AgentFeed.currentStepIndex(of: group, done: ["tu#0", "tu#1"]))

        second.resolved = true
        let partly = AgentAskGroup(askId: "tu", questions: [group.questions[0], second])
        XCTAssertEqual(AgentFeed.currentStepIndex(of: partly, done: []), 0)
        XCTAssertNil(AgentFeed.currentStepIndex(of: partly, done: ["tu#0"]))
    }

    // MARK: - Answer lock

    func testALockedCardStaysLockedUntilItExpiresAndAckedCardsNever() {
        var tracker = AgentAnswerTracker()
        let sentAt = Date()
        tracker.markSent("tu#0", at: sentAt)
        XCTAssertTrue(tracker.isLocked("tu#0"))
        XCTAssertTrue(tracker.isPending("tu#0"))
        XCTAssertFalse(tracker.isLocked("tu#1"))

        // Nothing came back — the optimistic lock frees the card again.
        let tooEarly = tracker.expire(now: sentAt.addingTimeInterval(5), timeout: 10)
        XCTAssertFalse(tooEarly)
        let expired = tracker.expire(now: sentAt.addingTimeInterval(11), timeout: 10)
        XCTAssertTrue(expired)
        XCTAssertFalse(tracker.isLocked("tu#0"))

        tracker.markSent("tu#1", at: sentAt)
        tracker.acknowledge("tu#1")
        XCTAssertTrue(tracker.isAcked("tu#1"))
        XCTAssertFalse(tracker.isPending("tu#1"))
        let ackedSurvives = tracker.expire(now: sentAt.addingTimeInterval(999), timeout: 10)
        XCTAssertFalse(ackedSurvives)
        XCTAssertTrue(tracker.isLocked("tu#1"))

        tracker.reset()
        XCTAssertFalse(tracker.isLocked("tu#1"))
    }

    // EXP-588: a locked step remembers WHAT was picked until the desktop's
    // resolution fills the real answer in; a rolled-back lock forgets it.
    func testLockedCardsRememberTheirPickedLabels() {
        var tracker = AgentAnswerTracker()
        XCTAssertNil(tracker.answerSummary("tu#0"))
        tracker.markSent("tu#0", labels: ["Blue", "Green"])
        XCTAssertEqual(tracker.answerSummary("tu#0"), "Blue, Green")
        tracker.acknowledge("tu#0")
        XCTAssertEqual(tracker.answerSummary("tu#0"), "Blue, Green")

        tracker.markSent("tu#1", labels: ["Yes"])
        tracker.expire("tu#1")
        XCTAssertNil(tracker.answerSummary("tu#1"))

        // A lock taken with no labels has no summary, not "".
        tracker.markSent("tu#2")
        XCTAssertNil(tracker.answerSummary("tu#2"))

        tracker.reset()
        XCTAssertNil(tracker.answerSummary("tu#0"))
    }

    func testResolvingDropsTheOptimisticLock() {
        var tracker = AgentAnswerTracker()
        tracker.markSent("plan")
        tracker.resolve("plan")
        XCTAssertFalse(tracker.isLocked("plan"))
        XCTAssertFalse(tracker.isFailed("plan"))
    }

    func testPerKeyExpiryLeavesOtherPendingLocksAlone() {
        // EXP-334: the shared timeout sweep dropped EVERY pending lock at
        // once, rolling a stepper back past steps answered moments before.
        var tracker = AgentAnswerTracker()
        tracker.markSent("tu#0")
        tracker.markSent("tu#1")
        tracker.expire("tu#0")
        XCTAssertFalse(tracker.isLocked("tu#0"))
        XCTAssertTrue(tracker.isFailed("tu#0"))
        XCTAssertTrue(tracker.isLocked("tu#1"), "the newer lock must survive")
        XCTAssertFalse(tracker.isFailed("tu#1"))

        // An acked card ignores a stray expiry.
        tracker.acknowledge("tu#1")
        tracker.expire("tu#1")
        XCTAssertTrue(tracker.isLocked("tu#1"))
        XCTAssertFalse(tracker.isFailed("tu#1"))
    }

    func testFailedClearsOnRetryAckAndResolve() {
        var tracker = AgentAnswerTracker()
        tracker.markSent("tu#0")
        tracker.expire("tu#0")
        XCTAssertTrue(tracker.isFailed("tu#0"))

        // Re-tapping (a retry) re-locks and clears the hint.
        tracker.markSent("tu#0")
        XCTAssertFalse(tracker.isFailed("tu#0"))
        XCTAssertTrue(tracker.isLocked("tu#0"))

        // A LATE ack after an expiry re-locks the card for good.
        tracker.expire("tu#0")
        tracker.acknowledge("tu#0")
        XCTAssertFalse(tracker.isFailed("tu#0"))
        XCTAssertTrue(tracker.isAcked("tu#0"))

        // And a resolution clears a failed flag outright.
        tracker.markSent("tu#1")
        tracker.expire("tu#1")
        tracker.resolve("tu#1")
        XCTAssertFalse(tracker.isFailed("tu#1"))
    }

    // MARK: - Staged replay ids (EXP-656)

    // The model commits a staged join replay as ONE swap and rewinds its event
    // counter to the id of the oldest visible item first, so the unchanged
    // prefix of the replayed history comes back with the ids it already had —
    // which is what keeps every SwiftUI row identity (and the reader's scroll
    // anchor) across the commit. This locks the arithmetic that swap relies on.
    func testAReplayWithARewoundCounterReproducesThePrefixIds() {
        // A live feed whose first item is not id 0 (the relay's log wrapped, or
        // an earlier replay already consumed ids).
        var nextEventId = 5
        func takeEventId() -> Int {
            defer { nextEventId += 1 }
            return nextEventId
        }
        let history = ["one", "two", "three"]
        var feed: [AgentFeedItem] = history.map { .narration(id: takeEventId(), text: $0) }
        XCTAssertEqual(feed.map(\.id), [5, 6, 7])
        XCTAssertEqual(nextEventId, 8)

        // Commit: rewind to the oldest visible id, drop the feed, fold the
        // replay (the same history plus one event the client hadn't seen).
        let anchorId = feed.first?.id
        feed = []
        if let anchorId { nextEventId = anchorId }
        for text in history + ["four"] {
            feed.append(.narration(id: takeEventId(), text: text))
        }
        XCTAssertEqual(feed.map(\.id), [5, 6, 7, 8])
        // The prefix kept its identities; only the new tail row is new.
        XCTAssertEqual(nextEventId, 9)
    }

    // MARK: - Fixtures

    private func tool(_ id: Int, subagentId: String? = nil) -> AgentFeedItem {
        .tool(id: id, name: "Edit", detail: "src/a.ts", subagentId: subagentId)
    }

    private func question(
        _ id: Int,
        wireId: String? = nil,
        askId: String? = nil,
        index: Int? = nil,
        total: Int? = nil
    ) -> AgentQuestion {
        AgentQuestion(
            // Every card carries a wire id (EXP-613) — the fixtures derive one
            // unless the test pins a specific value.
            id: id,
            wireId: wireId ?? "q\(id)",
            askId: askId,
            index: index,
            total: total,
            text: "Which color?",
            options: [
                AgentQuestionOption(label: "Red", key: "1"),
                AgentQuestionOption(label: "Blue", key: "2"),
            ]
        )
    }

    private func plan(_ id: Int) -> AgentQuestion {
        AgentQuestion(
            id: id,
            wireId: "plan\(id)",
            text: "# Plan\n\n- step one",
            options: [AgentQuestionOption(label: "Approve", key: "1")],
            planMode: true
        )
    }
}
