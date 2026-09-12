import Foundation
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

    /// EXP-783: a window restricts the projection without changing it, and a
    /// cut through a tool run re-keys the boundary row onto the first item the
    /// reader can actually see.
    func testAWindowRestrictsTheProjectionWithoutChangingIt() {
        let feed: [AgentFeedItem] =
            [.narration(id: 0, text: "hello")] + (1...6).map { tool($0) }
        XCTAssertEqual(AgentFeed.rows(feed, from: 0), AgentFeed.rows(feed))
        let windowed = AgentFeed.rows(feed, from: 4)
        XCTAssertEqual(windowed.count, 1)
        XCTAssertEqual(windowed[0].id, 4)
    }

    /// EXP-783: the byte budget evicts from the OLDEST end and always keeps
    /// the newest row, however large it is.
    func testTheByteBudgetEvictsTheOldestAndAlwaysKeepsOne() {
        let chunk = String(repeating: "x", count: 64 * 1024)
        let overflow: [AgentFeedItem] = (0..<400).map {
            .narration(id: $0, text: "\($0)\(chunk)")
        }
        let bytes = overflow.reduce(0) { $0 + AgentFeed.itemBytes($1) }
        let trimmed = AgentFeed.trim(feed: overflow, bytes: bytes)
        XCTAssertLessThan(trimmed.feed.count, overflow.count)
        XCTAssertFalse(trimmed.feed.isEmpty)
        XCTAssertLessThanOrEqual(trimmed.bytes, AgentFeed.feedByteCap)
        XCTAssertEqual(trimmed.feed.last, overflow.last)

        // A single row past the budget survives on its own.
        let huge: [AgentFeedItem] = [
            .narration(id: 0, text: "a"),
            .narration(id: 1, text: String(repeating: "y", count: AgentFeed.feedByteCap + 1)),
        ]
        let one = AgentFeed.trim(
            feed: huge, bytes: huge.reduce(0) { $0 + AgentFeed.itemBytes($1) }
        )
        XCTAssertEqual(one.feed.count, 1)
        XCTAssertEqual(one.feed.first, huge.last)
    }

    /// EXP-783: `withId` re-keys a prepended page without changing anything
    /// else about the row.
    func testWithIdOnlyChangesTheId() {
        let item = AgentFeedItem.tool(id: 1, name: "Edit", detail: "a.ts", subagentId: "s1")
        XCTAssertEqual(
            item.withId(9),
            .tool(id: 9, name: "Edit", detail: "a.ts", subagentId: "s1")
        )
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

    func testAReportedToolCallCountWinsOverTheVisibleOne() {
        // EXP-748: replay evicts a subagent's tool events first, so the rows
        // left behind undercount. The completed edge carries the publisher's
        // own count and that is what the row shows.
        let evicted: [AgentFeedItem] = [
            .subagent(id: 1, subagentId: "s1", agentType: "explorer", status: .started, detail: nil),
            tool(2, subagentId: "s1"),
            .subagent(
                id: 3, subagentId: "s1", agentType: "explorer",
                status: .completed, detail: nil, toolCalls: 7
            ),
        ]
        guard case let .subagentRun(run) = AgentFeed.rows(evicted)[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertEqual(run.reportedToolCalls, 7)
        XCTAssertEqual(run.toolCount, 7)

        // A count LOWER than what is visible never shrinks the row, and a
        // marker without one leaves the visible count alone.
        let visible: [AgentFeedItem] = [
            .subagent(id: 1, subagentId: "s2", agentType: "explorer", status: .started, detail: nil),
            tool(2, subagentId: "s2"),
            tool(3, subagentId: "s2"),
            .subagent(
                id: 4, subagentId: "s2", agentType: "explorer",
                status: .completed, detail: nil, toolCalls: 1
            ),
        ]
        guard case let .subagentRun(kept) = AgentFeed.rows(visible)[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertEqual(kept.toolCount, 2)

        let unreported: [AgentFeedItem] = [
            .subagent(id: 1, subagentId: "s3", agentType: "explorer", status: .started, detail: nil),
            tool(2, subagentId: "s3"),
        ]
        guard case let .subagentRun(old) = AgentFeed.rows(unreported)[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertNil(old.reportedToolCalls)
        XCTAssertEqual(old.toolCount, 1)
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
        // EXP-850 §9: the ask is still unanswered, so it sits BELOW the prose
        // that arrived after it — the card a human has to answer is always the
        // last thing in the transcript.
        XCTAssertEqual(rows[1], .single(feed[2]))
        guard case let .ask(group) = rows[2] else { return XCTFail("expected an ask group") }
        XCTAssertEqual(group.questions.map(\.id), [2, 5])
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

    // MARK: - Ask completion (EXP-820)

    func testAnAskCompletesOnlyWhenItsSubmitStepResolves() {
        var submit = question(3, wireId: "tu#submit", askId: "tu")
        let steps = [
            question(1, wireId: "tu#0", askId: "tu", index: 1, total: 2),
            question(2, wireId: "tu#1", askId: "tu", index: 2, total: 2),
        ]
        XCTAssertFalse(AgentFeed.askComplete(AgentAskGroup(askId: "tu", questions: steps + [submit])))
        // Every numbered step resolved but the review still open: not over —
        // the steerer can still go back to a step.
        var resolvedSteps = steps
        for i in resolvedSteps.indices { resolvedSteps[i].resolved = true }
        XCTAssertFalse(
            AgentFeed.askComplete(AgentAskGroup(askId: "tu", questions: resolvedSteps + [submit]))
        )
        submit.resolved = true
        XCTAssertTrue(AgentFeed.askComplete(AgentAskGroup(askId: "tu", questions: steps + [submit])))
    }

    func testALoneStepAskCompletesWhenThatStepResolves() {
        var only = question(1, wireId: "tu#0", askId: "tu", index: 1, total: 1)
        XCTAssertFalse(AgentFeed.askComplete(AgentAskGroup(askId: "tu", questions: [only])))
        only.resolved = true
        XCTAssertTrue(AgentFeed.askComplete(AgentAskGroup(askId: "tu", questions: [only])))
    }

    func testADismissedStepEndsTheWholeAsk() {
        var first = question(1, wireId: "tu#0", askId: "tu", index: 1, total: 2)
        let second = question(2, wireId: "tu#1", askId: "tu", index: 2, total: 2)
        XCTAssertFalse(AgentFeed.askComplete(AgentAskGroup(askId: "tu", questions: [first, second])))
        first.resolved = true
        first.dismissed = true
        XCTAssertTrue(AgentFeed.askComplete(AgentAskGroup(askId: "tu", questions: [first, second])))
    }

    func testAReResolvedStepReplacesItsRecordedAnswer() {
        // EXP-820 "go back": the engine re-publishes `question_resolved` for
        // an earlier step that was answered again.
        var step = question(1, wireId: "tu#0", askId: "tu", index: 1, total: 2)
        step.resolved = true
        step.answers = ["Red"]
        let feed: [AgentFeedItem] = [
            .question(step),
            .question(question(2, wireId: "tu#1", askId: "tu", index: 2, total: 2)),
        ]
        let out = AgentFeed.applyQuestionResolved(feed, id: "tu#0", askId: nil, answers: ["Blue"])
        XCTAssertEqual(out?[0].question?.answers, ["Blue"])
        XCTAssertEqual(out?[0].question?.resolved, true)
        // The other step is untouched.
        XCTAssertEqual(out?[1].question?.resolved, false)
    }

    func testAReAnswerKeepsTheStepAckedWhileItsNewFrameIsPending() {
        var tracker = AgentAnswerTracker()
        tracker.markSent("tu#0", labels: ["Red"])
        tracker.acknowledge("tu#0")
        tracker.markSent("tu#0", labels: ["Blue"])
        XCTAssertTrue(tracker.isPending("tu#0"))
        XCTAssertTrue(tracker.isAcked("tu#0"))
        XCTAssertEqual(tracker.answerSummary("tu#0"), "Blue")
        // No confirmation: the step stays LOCKED (it never rolls back into the
        // stepper's current slot); only the fresh label goes.
        tracker.expire("tu#0")
        XCTAssertTrue(tracker.isLocked("tu#0"))
        XCTAssertNil(tracker.answerSummary("tu#0"))
        // Confirmed: pending clears, acked stands, the new label shows.
        tracker.markSent("tu#0", labels: ["Blue"])
        tracker.acknowledge("tu#0")
        XCTAssertFalse(tracker.isPending("tu#0"))
        XCTAssertEqual(tracker.answerSummary("tu#0"), "Blue")
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

    // MARK: - Compaction (EXP-724)

    func testCompactionStartedOpensTheWindowWithItsTrigger() {
        let state = AgentFeed.applyCompaction(
            nil, event: ["phase": "started", "trigger": "manual"]
        )
        XCTAssertEqual(state, AgentCompaction(trigger: "manual"))
    }

    func testCompactionStartedWithoutATriggerStillOpensTheWindow() {
        let state = AgentFeed.applyCompaction(nil, event: ["phase": "started"])
        XCTAssertNotNil(state)
        XCTAssertNil(state?.trigger)
    }

    func testCompactionEndedClosesTheWindow() {
        let open = AgentCompaction(trigger: "auto")
        XCTAssertNil(AgentFeed.applyCompaction(open, event: ["phase": "ended"]))
        // An unmatched `ended` (codex publishes no start marker for auto
        // compaction) is simply a no-op on the state.
        XCTAssertNil(AgentFeed.applyCompaction(nil, event: ["phase": "ended"]))
    }

    func testUnknownOrMissingCompactionPhaseLeavesTheStateAlone() {
        let open = AgentCompaction(trigger: "manual")
        XCTAssertEqual(AgentFeed.applyCompaction(open, event: ["phase": "paused"]), open)
        XCTAssertEqual(AgentFeed.applyCompaction(open, event: [:]), open)
        XCTAssertNil(AgentFeed.applyCompaction(nil, event: ["phase": "paused"]))
        XCTAssertNil(AgentFeed.applyCompaction(nil, event: ["trigger": "manual"]))
    }

    func testCompactionMarkerIsAnOrdinarySingleFeedRow() {
        let rows = AgentFeed.rows([.narration(id: 1, text: "hi"), .compaction(id: 2)])
        XCTAssertEqual(rows.count, 2)
        XCTAssertEqual(rows[1].id, 2)
        XCTAssertEqual(rows[1], .single(.compaction(id: 2)))
    }

    /// The strings the four clients draw — byte-identical (EXP-724). The
    /// ellipsis is ONE character (U+2026).
    func testCompactionLabelsAreTheLockedLiterals() {
        XCTAssertEqual(AgentFeed.compactingLabel, "Compacting context\u{2026}")
        XCTAssertEqual(AgentFeed.compactedLabel, "Context compacted")
        XCTAssertEqual(AgentFeed.compactingLabel.count, "Compacting context".count + 1)
        XCTAssertEqual(AgentFeed.compactionTimeoutSeconds, 180)
    }

    // MARK: - Live agent config + usage (EXP-746)

    func testApplyConfigStateFoldsAFullSnapshot() {
        let config = AgentFeed.applyConfigState(nil, event: [
            "kind": "config_state",
            "options": [
                ["id": "model", "label": "Model", "category": "model", "value": "opus",
                 "values": [["id": "opus", "label": "Opus"], ["id": "sonnet", "label": "Sonnet"]]],
                ["id": "effort", "label": "Effort"],
            ],
            "currentMode": "plan",
            "modes": [
                ["id": "plan", "label": "Plan", "description": "Ask before editing"],
                ["id": "bypassPermissions", "label": "Bypass permissions"],
            ],
            "commands": [["name": "review", "description": "Review the diff", "hint": "<path>"]],
        ])
        XCTAssertEqual(config?.options.count, 2)
        XCTAssertEqual(config?.options.first?.values.map(\.id), ["opus", "sonnet"])
        XCTAssertEqual(config?.options.first?.value, "opus")
        XCTAssertEqual(config?.currentMode, "plan")
        XCTAssertEqual(config?.modes.map(\.id), ["plan", "bypassPermissions"])
        XCTAssertEqual(config?.commands.first?.hint, "<path>")
    }

    /// A newer snapshot REPLACES the previous one — the chips are a slot, not
    /// a feed.
    func testANewerConfigStateReplacesTheSnapshot() {
        let first = AgentFeed.applyConfigState(nil, event: [
            "options": [["id": "model", "label": "Model", "value": "opus"]],
        ])
        let second = AgentFeed.applyConfigState(first, event: [
            "options": [["id": "model", "label": "Model", "value": "sonnet"]],
        ])
        XCTAssertEqual(second?.options.count, 1)
        XCTAssertEqual(second?.options.first?.value, "sonnet")
    }

    func testAMalformedConfigStateKeepsThePreviousSnapshot() {
        let previous = AgentFeed.applyConfigState(nil, event: [
            "options": [["id": "model", "label": "Model", "value": "opus"]],
        ])
        XCTAssertEqual(AgentFeed.applyConfigState(previous, event: [:]), previous)
        XCTAssertEqual(
            AgentFeed.applyConfigState(previous, event: ["options": "nope"]), previous
        )
        // An id-less option is dropped, not fatal.
        let partial = AgentFeed.applyConfigState(previous, event: [
            "options": [["label": "Model"], ["id": "effort", "label": "Effort"]],
        ])
        XCTAssertEqual(partial?.options.map(\.id), ["effort"])
    }

    // EXP-785/786: `tool_update` folds into the tool row by call id.
    func testToolUpdateFoldsIntoItsRowAndNeverAddsOne() {
        let feed: [AgentFeedItem] = [
            .tool(id: 1, name: "Edit", detail: "a.ts", subagentId: nil, callId: "tc-1", toolKind: "edit"),
            .narration(id: 2, text: "between"),
            .tool(id: 3, name: "Bash", detail: "bun", subagentId: nil, callId: "tc-2", toolKind: "execute"),
        ]
        let settled = AgentFeed.applyToolUpdate(feed: feed, event: [
            "id": "tc-1", "status": "completed", "diff": "--- a/a.ts\n+++ b/a.ts\n",
        ])
        XCTAssertEqual(settled?.count, 3)
        XCTAssertEqual(
            settled?[0],
            .tool(
                id: 1, name: "Edit", detail: "a.ts", subagentId: nil, callId: "tc-1",
                toolKind: "edit", settled: true, failed: false, diff: "--- a/a.ts\n+++ b/a.ts\n"
            )
        )
        XCTAssertEqual(settled?[2], feed[2])
        // A failed settle wins over the completed one; the diff stays.
        let failed = AgentFeed.applyToolUpdate(feed: settled!, event: ["id": "tc-1", "status": "failed"])
        guard case let .tool(_, _, _, _, _, _, isSettled, isFailed, diff, _)? = failed?[0] else {
            return XCTFail("not a tool row")
        }
        XCTAssertTrue(isSettled)
        XCTAssertTrue(isFailed)
        XCTAssertEqual(diff, "--- a/a.ts\n+++ b/a.ts\n")
        // A status-less update carrying only a diff never settles.
        let diffed = AgentFeed.applyToolUpdate(feed: feed, event: ["id": "tc-2", "diff": "+x\n"])
        XCTAssertEqual(
            diffed?[2],
            .tool(id: 3, name: "Bash", detail: "bun", subagentId: nil, callId: "tc-2",
                  toolKind: "execute", settled: false, failed: false, diff: "+x\n")
        )
        // The diff weighs against the budget.
        XCTAssertEqual(
            AgentFeed.itemBytes(diffed![2]) - AgentFeed.itemBytes(feed[2]), "+x\n".utf8.count
        )
    }

    func testToolUpdateForAnUnknownIdIsDroppedAndTheNewestRowWins() {
        let feed: [AgentFeedItem] = [
            .tool(id: 1, name: "Edit", detail: nil, subagentId: nil, callId: "tc-1"),
            .tool(id: 2, name: "Grep", detail: nil, subagentId: nil),
            .tool(id: 3, name: "Edit", detail: nil, subagentId: nil, callId: "tc-1"),
        ]
        XCTAssertNil(AgentFeed.applyToolUpdate(feed: feed, event: ["id": "tc-nope", "status": "failed"]))
        XCTAssertNil(AgentFeed.applyToolUpdate(feed: feed, event: ["status": "failed"]))
        let next = AgentFeed.applyToolUpdate(feed: feed, event: ["id": "tc-1", "status": "completed"])
        XCTAssertEqual(next?[0], feed[0], "the older twin is untouched")
        XCTAssertEqual(
            next?[2],
            .tool(id: 3, name: "Edit", detail: nil, subagentId: nil, callId: "tc-1", settled: true)
        )
        XCTAssertEqual(AgentFeed.toolKind("switch_mode"), "switch_mode")
        XCTAssertNil(AgentFeed.toolKind("teleport"))
        XCTAssertEqual(AgentFeed.toolKindValues, DomainContract.toolKindValues)
    }

    // EXP-784: the rate-limit slot.
    func testApplyRateLimitKeepsAWindowAndClearsOnOkOrEmpty() {
        let limited = AgentFeed.applyRateLimit(nil, event: [
            "status": " allowed_warning ", "resetsAt": 1_700_000_000_000, "message": " 80% used ",
        ])
        XCTAssertEqual(limited, AgentSessionRateLimit(
            status: "allowed_warning", resetsAt: 1_700_000_000_000, message: "80% used"
        ))
        XCTAssertEqual(
            AgentFeed.applyRateLimit(limited, event: ["status": "rejected", "resetsAt": -1]),
            AgentSessionRateLimit(status: "rejected")
        )
        XCTAssertNil(AgentFeed.applyRateLimit(limited, event: ["status": "ok"]))
        XCTAssertNil(AgentFeed.applyRateLimit(limited, event: ["status": ""]))
        // Unreadable clears too: a stale banner beside a live run is worse.
        XCTAssertNil(AgentFeed.applyRateLimit(limited, event: [:]))
        XCTAssertTrue(AgentFeed.rateLimitClears(" OK "))
        XCTAssertFalse(AgentFeed.rateLimitClears("allowed"))
    }

    // EXP-786: the publisher's cut note is a footer, never a diff line.
    // Mirrors web `per-call diff truncation (EXP-786)` case for case.
    func testPerCallDiffTruncationSplitsTheTrailingNoteOff() {
        let diff = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b"
        var split = AgentFeed.splitTruncatedDiff("\(diff)\n\\ 120 more lines truncated")
        XCTAssertEqual(split.diff, diff)
        XCTAssertEqual(split.truncated, 120)
        // Singular wording, and trailing whitespace after the note.
        split = AgentFeed.splitTruncatedDiff("\(diff)\n\\ 1 more line truncated\n")
        XCTAssertEqual(split.diff, diff)
        XCTAssertEqual(split.truncated, 1)
    }

    func testPerCallDiffTruncationLeavesAnUncutDiffAlone() {
        let diff = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b"
        // git's OWN `\ ` marker shares the prefix and must survive as a line.
        let eof = "\(diff)\n\\ No newline at end of file"
        var split = AgentFeed.splitTruncatedDiff(eof)
        XCTAssertEqual(split.diff, eof)
        XCTAssertNil(split.truncated)
        // The note only counts as the LAST line.
        let mid = "\\ 12 more lines truncated\n\(diff)"
        split = AgentFeed.splitTruncatedDiff(mid)
        XCTAssertEqual(split.diff, mid)
        XCTAssertNil(split.truncated)
        split = AgentFeed.splitTruncatedDiff("")
        XCTAssertEqual(split.diff, "")
        XCTAssertNil(split.truncated)
    }

    func testPerCallDiffTruncationWordsTheFooter() {
        XCTAssertEqual(AgentFeed.diffTruncationNote(1), "1 more line truncated")
        XCTAssertEqual(AgentFeed.diffTruncationNote(120), "120 more lines truncated")
    }

    func testApplyUsageRefusesAZeroContextSize() {
        let usage = AgentFeed.applyUsage(nil, event: [
            "contextUsed": 124_000, "contextSize": 200_000, "costUsd": 1.24,
        ])
        XCTAssertEqual(usage, AgentSessionUsage(
            contextUsed: 124_000, contextSize: 200_000, costUsd: 1.24
        ))
        XCTAssertEqual(usage?.percent, 62)
        // A size of zero knows nothing worth drawing: the slot CLEARS.
        XCTAssertNil(AgentFeed.applyUsage(usage, event: ["contextUsed": 10, "contextSize": 0]))
        // An unreadable payload leaves the previous numbers standing.
        XCTAssertEqual(AgentFeed.applyUsage(usage, event: ["contextUsed": 10]), usage)
    }

    /// EXP-772: the mode is the ONLY steering chip left. Advertised options
    /// (the engine now publishes none) never reach the composer again.
    func testOnlyTheModeReachesTheComposer() {
        let config = AgentSessionConfig(
            options: [
                AgentConfigOption(
                    id: "model", label: "Model", value: "opus",
                    values: [AgentConfigValue(id: "opus", label: "Opus")]
                ),
                AgentConfigOption(id: "effort", label: "Effort"),
            ],
            currentMode: "plan",
            modes: [
                AgentConfigMode(id: "plan", label: "Plan"),
                AgentConfigMode(id: "auto", label: "Auto"),
                AgentConfigMode(id: "ask", label: "Ask"),
            ]
        )
        let chip = try? XCTUnwrap(AgentFeed.modeChip(config))
        XCTAssertEqual(chip?.valueLabel, "Plan")
        XCTAssertEqual(chip?.values.map(\.id), ["plan", "auto", "ask"])
        // Three modes are a picker, not a switch.
        XCTAssertNil(chip?.planToggle)
        XCTAssertNil(AgentFeed.modeChip(nil))
    }

    /// A run that advertises NO modes (codex) draws nothing at all — an inert
    /// badge would be a control that cannot be operated.
    func testAModelessRunDrawsNoChip() {
        XCTAssertNil(AgentFeed.modeChip(AgentSessionConfig(
            options: [AgentConfigOption(id: "effort", label: "Effort")]
        )))
    }

    /// EXP-772: `plan` plus exactly one other mode is a yes/no question, so it
    /// collapses into the compact Plan switch whose off position is the other
    /// mode — claude's `plan` / `bypassPermissions` pair.
    func testPlanPlusOneOtherModeCollapsesIntoTheSwitch() {
        let claude = AgentSessionConfig(
            currentMode: "bypassPermissions",
            modes: [
                AgentConfigMode(id: "plan", label: "Plan"),
                AgentConfigMode(id: "bypassPermissions", label: "Build"),
            ]
        )
        let off = try? XCTUnwrap(AgentFeed.modeChip(claude)?.planToggle)
        XCTAssertEqual(off?.on, false)
        XCTAssertEqual(off?.planId, "plan")
        XCTAssertEqual(off?.otherId, "bypassPermissions")

        let planning = AgentSessionConfig(
            currentMode: "plan",
            modes: claude.modes
        )
        XCTAssertEqual(AgentFeed.modeChip(planning)?.planToggle?.on, true)

        // A PAIR without a plan mode stays an ordinary picker.
        let pair = AgentSessionConfig(
            currentMode: "a",
            modes: [AgentConfigMode(id: "a", label: "A"), AgentConfigMode(id: "b", label: "B")]
        )
        XCTAssertNil(AgentFeed.modeChip(pair)?.planToggle)
    }

    /// A mode in force that the publisher never advertised still reads as
    /// itself, never as the CLI default.
    func testAnUnadvertisedCurrentModeReadsAsItself() {
        let chip = AgentFeed.modeChip(AgentSessionConfig(
            currentMode: "sneaky",
            modes: [AgentConfigMode(id: "plan", label: "Plan")]
        ))
        XCTAssertEqual(chip?.valueLabel, "sneaky")
    }

    /// The labels every client draws (EXP-746/EXP-772), byte-for-byte.
    func testTheConfigDefaultLabelIsTheOneEveryClientShows() {
        XCTAssertEqual(AgentFeed.configDefaultValueLabel, "CLI default")
        XCTAssertEqual(AgentFeed.configModeLabel, "Mode")
        XCTAssertEqual(AgentFeed.planToggleLabel, "Plan")
        XCTAssertEqual(AgentFeed.planModeId, "plan")
    }

    // MARK: - Narration merging + subagent scoping (EXP-772/EXP-773)

    /// EXP-772: the engine flushes one assistant message in several narration
    /// events. Consecutive flushes of the SAME message id grow one bubble;
    /// anything in between opens a new one.
    func testConsecutiveFlushesOfOneMessageMergeIntoOneBubble() {
        let first: [AgentFeedItem] = [.narration(id: 1, text: "Reading ", messageId: "m1")]
        let merged = try? XCTUnwrap(
            AgentFeed.mergeNarration(first, text: "the file.", messageId: "m1", subagentId: nil)
        )
        XCTAssertEqual(merged?.count, 1)
        XCTAssertEqual(merged?.first, .narration(id: 1, text: "Reading the file.", messageId: "m1"))

        // A different message never merges, and neither does a tool call in
        // between — the prose resumed after something happened.
        XCTAssertNil(AgentFeed.mergeNarration(first, text: "x", messageId: "m2", subagentId: nil))
        XCTAssertNil(AgentFeed.mergeNarration(
            first + [tool(2)], text: "x", messageId: "m1", subagentId: nil
        ))
        // An id-less event (an older publisher) always opens its own bubble.
        XCTAssertNil(AgentFeed.mergeNarration(first, text: "x", messageId: nil, subagentId: nil))
        XCTAssertNil(AgentFeed.mergeNarration([], text: "x", messageId: "m1", subagentId: nil))
        // Same message id from a different scope is a different bubble.
        XCTAssertNil(AgentFeed.mergeNarration(first, text: "x", messageId: "m1", subagentId: "s1"))
    }

    /// EXP-846: the look-back is LANE-scoped, not tail-only. The real shape
    /// off the wire while a subagent works: main-lane prose, the subagent's
    /// spawn edge, its tool call, that call's `tool_update` (which PATCHES the
    /// row in place — `settled`/`preview` — rather than appending one), then
    /// the rest of the SAME main-lane message. One bubble, not two.
    func testNarrationLooksBackPastAnotherLanesRows() {
        // The subagent's call as its `tool_update` left it.
        let updatedCall = AgentFeedItem.tool(
            id: 3, name: "Read", detail: "src/a.ts", subagentId: "s1",
            callId: "call-1", toolKind: "read", settled: true
        )
        let feed: [AgentFeedItem] = [
            .narration(id: 1, text: "A", messageId: "m1"),
            .subagent(id: 2, subagentId: "s1", agentType: "explore", status: .started, detail: nil),
            updatedCall,
        ]
        let merged = AgentFeed.mergeNarration(feed, text: "B", messageId: "m1", subagentId: nil)
        XCTAssertEqual(merged?.count, 3)
        XCTAssertEqual(merged?.first, .narration(id: 1, text: "AB", messageId: "m1"))
        // The rows in between keep their place, their ids and their patches.
        XCTAssertEqual(merged?[1], feed[1])
        XCTAssertEqual(merged?[2], updatedCall)

        // Symmetrically, a subagent's own message reaches past the MAIN lane's
        // interleaved rows (its lane is the one that must be quiet).
        let inSubagent: [AgentFeedItem] = [
            .narration(id: 1, text: "A", messageId: "m9", subagentId: "s1"),
            tool(2),
            .narration(id: 3, text: "Main prose.", messageId: "m1"),
        ]
        let inLane = AgentFeed.mergeNarration(
            inSubagent, text: "B", messageId: "m9", subagentId: "s1"
        )
        XCTAssertEqual(inLane?.count, 3)
        XCTAssertEqual(
            inLane?[0],
            AgentFeedItem.narration(id: 1, text: "AB", messageId: "m9", subagentId: "s1")
        )

        // A row in the fragment's OWN lane still ends the run, however far
        // back the matching message is: a main-lane tool call…
        XCTAssertNil(AgentFeed.mergeNarration(
            [feed[0], feed[1], updatedCall, tool(4)],
            text: "B", messageId: "m1", subagentId: nil
        ))
        // …a human turn…
        XCTAssertNil(AgentFeed.mergeNarration(
            [feed[0], .userMessage(id: 4, text: "stop")],
            text: "B", messageId: "m1", subagentId: nil
        ))
        // …a question…
        XCTAssertNil(AgentFeed.mergeNarration(
            [feed[0], .question(question(4))], text: "B", messageId: "m1", subagentId: nil
        ))
        // …and the lane's own prose from a DIFFERENT message.
        XCTAssertNil(AgentFeed.mergeNarration(
            [feed[0], .narration(id: 4, text: "Other.", messageId: "m2")],
            text: "B", messageId: "m1", subagentId: nil
        ))
    }

    /// EXP-773: a subagent's prose and the turns addressed to it leave the
    /// main feed and render inside that subagent's run, in publish order.
    func testSubagentProseAndTurnsGroupUnderTheirRun() {
        let feed: [AgentFeedItem] = [
            .narration(id: 1, text: "Delegating."),
            .subagent(id: 2, subagentId: "s1", agentType: "explore", status: .started, detail: nil),
            .userMessage(id: 3, text: "map the repo", subagentId: "s1"),
            .narration(id: 4, text: "Looking.", subagentId: "s1"),
            tool(5, subagentId: "s1"),
            .narration(id: 6, text: "Back on the main thread."),
        ]
        let rows = AgentFeed.rows(feed)
        // Main feed: the two unscoped narrations plus the group row.
        XCTAssertEqual(rows.count, 3)
        XCTAssertEqual(rows[0], .single(feed[0]))
        XCTAssertEqual(rows[2], .single(feed[5]))
        guard case let .subagentRun(run) = rows[1] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertEqual(run.items.map(\.id), [3, 4, 5])
        // The caption still counts TOOL calls, not conversation rows.
        XCTAssertEqual(run.toolCount, 1)
        XCTAssertTrue(run.expandable)
    }

    // MARK: - Quiet live runs (FEED-26)

    func testStaleActivityStaysSilentInsideTheThreshold() {
        let now = Date()
        XCTAssertNil(AgentFeed.staleActivityMinutes(since: now, now: now))
        XCTAssertNil(
            AgentFeed.staleActivityMinutes(since: now.addingTimeInterval(-599), now: now)
        )
    }

    func testStaleActivityCountsWholeMinutesFromTheThreshold() {
        let now = Date()
        XCTAssertEqual(
            AgentFeed.staleActivityMinutes(since: now.addingTimeInterval(-600), now: now),
            10
        )
        // 27m40s reads as 27, never rounded up: the caption promises elapsed
        // time, not the nearest minute.
        XCTAssertEqual(
            AgentFeed.staleActivityMinutes(since: now.addingTimeInterval(-1660), now: now),
            27
        )
    }

    func testStaleActivityIsNilWithoutAKnownLastActivity() {
        XCTAssertNil(AgentFeed.staleActivityMinutes(since: nil, now: Date()))
    }

    func testStaleActivityThresholdIsTenMinutes() {
        // Byte-identical ×4 — moving it means moving web, Android and desktop.
        XCTAssertEqual(AgentFeed.staleActivityAfter, 600)
    }

    func testStaleActivityLabelCarriesTheDeviceSuffix() {
        XCTAssertEqual(
            AgentFeed.staleActivityLabel(minutes: 27, deviceLabel: "macbook"),
            "No activity for 27 min · macbook"
        )
        XCTAssertEqual(
            AgentFeed.staleActivityLabel(minutes: 10, deviceLabel: nil),
            "No activity for 10 min"
        )
        // A blank label is an absent one — never a dangling separator.
        XCTAssertEqual(
            AgentFeed.staleActivityLabel(minutes: 10, deviceLabel: ""),
            "No activity for 10 min"
        )
    }

    // MARK: - Transcript gap ladder (EXP-787)

    func testTranscriptGapLadder() {
        // The first row has nothing above it.
        for cur in [AgentRowClass.turn, .prose, .tool] {
            XCTAssertEqual(AgentFeed.transcriptGap(prev: nil, cur: cur), .none)
        }
        // All nine ordered pairs, in the ladder's own order: a turn on EITHER
        // side wins first, then tool↔tool, then the mixed step, then prose.
        let expected: [(AgentRowClass, AgentRowClass, AgentTranscriptGap)] = [
            (.turn, .turn, .turn),
            (.turn, .prose, .turn),
            (.turn, .tool, .turn),
            (.prose, .turn, .turn),
            (.tool, .turn, .turn),
            (.tool, .tool, .default),
            (.prose, .tool, .tool),
            (.tool, .prose, .tool),
            (.prose, .prose, .block),
        ]
        for (prev, cur, gap) in expected {
            XCTAssertEqual(
                AgentFeed.transcriptGap(prev: prev, cur: cur), gap,
                "gap above \(cur) after \(prev)"
            )
        }
    }

    func testRowClassSortsEveryRowKind() {
        XCTAssertEqual(AgentFeedRow.single(.userMessage(id: 1, text: "go")).rowClass, .turn)
        XCTAssertEqual(AgentFeedRow.single(.narration(id: 2, text: "hi")).rowClass, .prose)
        XCTAssertEqual(AgentFeedRow.single(.question(question(3))).rowClass, .prose)
        XCTAssertEqual(AgentFeedRow.single(.compaction(id: 4)).rowClass, .prose)
        XCTAssertEqual(
            AgentFeedRow.ask(AgentAskGroup(askId: "a", questions: [question(5)])).rowClass,
            .prose
        )
        XCTAssertEqual(AgentFeedRow.single(tool(6)).rowClass, .tool)
        XCTAssertEqual(AgentFeedRow.toolRun([tool(7), tool(8)]).rowClass, .tool)
        XCTAssertEqual(
            AgentFeedRow.single(
                .subagent(
                    id: 9, subagentId: "s1", agentType: "explore",
                    status: .started, detail: nil
                )
            ).rowClass,
            .tool
        )
        XCTAssertEqual(
            AgentFeedRow.single(.permission(id: 10, tool: "Bash", detail: nil)).rowClass,
            .tool
        )
        XCTAssertEqual(
            AgentFeedRow.subagentRun(
                AgentSubagentRun(
                    anchorId: 11, subagentId: "s2", agentType: "explore",
                    detail: nil, done: false, items: [tool(12)]
                )
            ).rowClass,
            .tool
        )
    }

    // MARK: - Turn edges (EXP-848)

    func testTurnStateRawValuesMatchTheContract() {
        XCTAssertEqual(AgentTurnState.allCases.map(\.rawValue), DomainContract.turnStateValues)
    }

    func testApplyTurnIsLatestWinsAndIgnoresNonsense() {
        func state(_ current: AgentTurnState, _ event: [String: Any]) -> AgentTurnState {
            AgentFeed.applyTurn(AgentTurnSlot(state: current), event: event).state
        }
        XCTAssertEqual(state(.ended, ["state": "started"]), .started)
        XCTAssertEqual(state(.started, ["state": "ended"]), .ended)
        // A malformed frame never flips the indicator (the applyCompaction
        // contract).
        XCTAssertEqual(state(.started, ["state": "halfway"]), .started)
        XCTAssertEqual(state(.started, [:]), .started)
        XCTAssertEqual(state(.ended, ["state": 1]), .ended)
    }

    func testWorkingNeedsAnOpenTurnAndNothingWaitingOnAHuman() {
        func working(
            live: Bool = true,
            ended: Bool = false,
            turn: AgentTurnState = .started,
            awaiting: Bool = false,
            needsInput: Bool = false,
            blocked: Bool = false,
            compacting: Bool = false
        ) -> Bool {
            AgentFeed.working(
                live: live, sessionEnded: ended, turnState: turn, awaitingInput: awaiting,
                needsInput: needsInput, blocked: blocked, compacting: compacting
            )
        }
        XCTAssertTrue(working())
        // The default is IDLE: a run whose publisher never sent a turn edge
        // must not pulse.
        XCTAssertFalse(working(turn: .ended))
        XCTAssertFalse(working(live: false))
        XCTAssertFalse(working(ended: true))
        XCTAssertFalse(working(awaiting: true))
        XCTAssertFalse(working(needsInput: true))
        XCTAssertFalse(working(blocked: true))
        XCTAssertFalse(working(compacting: true))
    }

    // MARK: - Subagent titles (EXP-847)

    func testSubagentTitleLeadsAndTheTypeIsItsFallback() {
        let titled: [AgentFeedItem] = [
            .subagent(
                id: 1, subagentId: "s1", agentType: "explore", status: .started,
                detail: "map", title: "Find every caller of resolveTeamAccess"
            ),
            tool(2, subagentId: "s1"),
            // A completed edge with no title must not blank the label.
            .subagent(
                id: 3, subagentId: "s1", agentType: "explore", status: .completed, detail: nil
            ),
        ]
        guard case let .subagentRun(run) = AgentFeed.rows(titled)[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertEqual(run.title, "Find every caller of resolveTeamAccess")
        XCTAssertEqual(run.label, "Find every caller of resolveTeamAccess")
        XCTAssertEqual(run.agentType, "explore")
        // The title weighs against the feed budget.
        XCTAssertGreaterThan(
            AgentFeed.itemBytes(titled[0]),
            AgentFeed.itemBytes(.subagent(
                id: 1, subagentId: "s1", agentType: "explore", status: .started, detail: "map"
            ))
        )

        let untitled: [AgentFeedItem] = [
            .subagent(id: 1, subagentId: "s1", agentType: "explore", status: .started, detail: nil),
        ]
        guard case let .subagentRun(bare) = AgentFeed.rows(untitled)[0] else {
            return XCTFail("expected a subagent run")
        }
        XCTAssertNil(bare.title)
        XCTAssertEqual(bare.label, "explore")
    }

    // MARK: - Exponential tool previews (EXP-846)

    func testToolPreviewFoldsOntoItsRowAndSurvivesALaterSettle() {
        let feed: [AgentFeedItem] = [
            .tool(
                id: 1, name: "exponential_issues_create", detail: nil, subagentId: nil,
                callId: "tc-1", toolKind: "other"
            ),
        ]
        let previewed = AgentFeed.applyToolUpdate(feed: feed, event: [
            "id": "tc-1",
            "preview": ["identifier": "EXP-849", "title": "Drop pi", "count": 3],
        ])
        guard case let .tool(_, _, _, _, _, _, settled, _, _, preview)? = previewed?[0] else {
            return XCTFail("not a tool row")
        }
        XCTAssertFalse(settled, "a preview alone never settles the call")
        XCTAssertEqual(preview?.identifier, "EXP-849")
        XCTAssertEqual(preview?.title, "Drop pi")
        XCTAssertEqual(preview?.count, 3)
        XCTAssertNil(preview?.url)

        // A later settle keeps the preview the row already holds.
        let settledNext = AgentFeed.applyToolUpdate(
            feed: previewed!, event: ["id": "tc-1", "status": "completed"]
        )
        guard case let .tool(_, _, _, _, _, _, isSettled, _, _, kept)? = settledNext?[0] else {
            return XCTFail("not a tool row")
        }
        XCTAssertTrue(isSettled)
        XCTAssertEqual(kept?.identifier, "EXP-849")
    }

    func testToolPreviewIsNilForAnUnusableOrEmptyPayload() {
        XCTAssertNil(AgentFeed.toolPreview(nil))
        XCTAssertNil(AgentFeed.toolPreview("EXP-849"))
        XCTAssertNil(AgentFeed.toolPreview([:] as [String: Any]))
        // Blank strings carry nothing — the whole preview drops.
        XCTAssertNil(AgentFeed.toolPreview(["title": "   ", "url": ""]))
        XCTAssertEqual(AgentFeed.toolPreview(["count": 0])?.count, 0)
    }

    // MARK: - EXP-850 §9: the pending card sits at the bottom

    func testAPendingQuestionMovesAfterEveryLaterRow() {
        let feed: [AgentFeedItem] = [
            .question(question(1)),
            tool(2), tool(3),
            .narration(id: 4, text: "still working"),
        ]
        let rows = AgentFeed.rows(feed)
        // The card is answerable, so it is the LAST thing in the transcript —
        // the tool run and the prose that arrived after it keep their order.
        XCTAssertEqual(rows.count, 3)
        XCTAssertEqual(rows.map(\.id), [2, 4, 1])
        XCTAssertTrue(AgentFeed.isPendingCard(rows[2]))
    }

    func testAResolvedCardReturnsToItsNaturalPosition() {
        var answered = question(1)
        answered.resolved = true
        let feed: [AgentFeedItem] = [
            .question(answered), tool(2), .narration(id: 3, text: "done"),
        ]
        XCTAssertEqual(AgentFeed.rows(feed).map(\.id), [1, 2, 3])
        // A DISMISSED card is not pending either — the engine tore it down.
        var dismissed = question(4)
        dismissed.dismissed = true
        XCTAssertFalse(AgentFeed.isPendingCard(.single(.question(dismissed))))
    }

    func testPendingCardsKeepTheirRelativeOrderAtTheBottom() {
        let feed: [AgentFeedItem] = [
            .question(question(1, wireId: "q1", askId: "ask_1", index: 1, total: 2)),
            tool(2),
            .question(question(3)),
            .narration(id: 4, text: "prose"),
        ]
        let rows = AgentFeed.rows(feed)
        // The ask group opened first, so it stays ahead of the lone card.
        XCTAssertEqual(rows.map(\.id), [2, 4, 1, 3])
    }

    // MARK: - EXP-850 §3: the workflow card's row projection

    private func workflowTool(_ id: Int, callId: String) -> AgentFeedItem {
        .tool(id: id, name: "Workflow", detail: "wire-probe", subagentId: nil, callId: callId)
    }

    func testAWorkflowToolRowNeverCollapsesIntoAToolRun() {
        let feed: [AgentFeedItem] = [
            workflowTool(1, callId: "w1"),
            .tool(id: 2, name: "Read", detail: "a.ts", subagentId: nil, callId: "t2"),
            .tool(id: 3, name: "Edit", detail: "b.ts", subagentId: nil, callId: "t3"),
        ]
        // Without the card the three calls are one collapsed run…
        XCTAssertEqual(AgentFeed.rows(feed).count, 1)
        // …with it, the Workflow call is its own row (the card renders THERE)
        // and only the other two collapse.
        let rows = AgentFeed.rows(feed, workflowIds: ["w1"])
        XCTAssertEqual(rows.count, 2)
        guard case .single = rows[0] else { return XCTFail("the card row must stand alone") }
        guard case let .toolRun(run) = rows[1] else { return XCTFail("expected a tool run") }
        XCTAssertEqual(run.map(\.id), [2, 3])
    }

    func testWorkflowAgentsAreNeitherTranscriptRowsNorTabs() {
        let feed: [AgentFeedItem] = [
            .subagent(
                id: 1, subagentId: "a1", agentType: "general-purpose", status: .started,
                detail: "phase one", title: "alpha:one", workflowId: "w1"
            ),
            tool(2, subagentId: "a1"),
            .subagent(
                id: 3, subagentId: "b1", agentType: "general-purpose", status: .started,
                detail: "an ordinary detour", title: "reviewer"
            ),
            tool(4, subagentId: "b1"),
        ]
        // The card nests its own agent, so the transcript shows only the
        // ordinary subagent's group.
        let rows = AgentFeed.rows(feed, workflowIds: ["w1"])
        XCTAssertEqual(rows.count, 1)
        guard case let .subagentRun(ordinary) = rows[0] else { return XCTFail("expected a run") }
        XCTAssertEqual(ordinary.subagentId, "b1")
        // The runs list keeps BOTH — that is how the card finds its agent.
        let runs = AgentFeed.subagents(feed)
        XCTAssertEqual(runs.map(\.subagentId), ["a1", "b1"])
        XCTAssertEqual(runs[0].workflowId, "w1")
        // …and the tab strip offers only the steerable one.
        XCTAssertEqual(
            AgentFeed.visibleSubagentTabs(runs, selected: nil).map(\.subagentId), ["b1"]
        )
        // Even when it is the selected tab, a workflow agent is never offered.
        XCTAssertEqual(
            AgentFeed.visibleSubagentTabs(runs, selected: "a1").map(\.subagentId), ["b1"]
        )
    }

    // MARK: - EXP-856 §4: the duplicate warning

    func testADuplicateEdgeWarnsBesideItsRunWithoutFinishingIt() {
        let sentence = "Second copy of slowpoke started while the first is still running (resumed by SendMessage)"
        let feed: [AgentFeedItem] = [
            .subagent(
                id: 1, subagentId: "a1", agentType: "general-purpose", status: .started,
                detail: "the first copy", title: "slowpoke"
            ),
            tool(2, subagentId: "a1"),
            .subagent(
                id: 3, subagentId: "a1", agentType: "general-purpose", status: .duplicate,
                detail: sentence, title: "slowpoke"
            ),
        ]
        let runs = AgentFeed.subagents(feed)
        let run = runs.first
        XCTAssertEqual(runs.count, 1)
        XCTAssertEqual(run?.duplicateDetail, sentence)
        XCTAssertEqual(run?.duplicate, true)
        // The warning never finishes the run and never replaces its own
        // delegation detail.
        XCTAssertEqual(run?.done, false)
        XCTAssertEqual(run?.detail, "the first copy")
        XCTAssertEqual(run?.label, "slowpoke")
    }

    // MARK: - EXP-850 §2/§3: the latest-wins slots

    func testApplyWorkflowIsLatestWinsPerIdInFirstAppearanceOrder() {
        let first = AgentWorkflow(id: "w1", name: "one")
        let second = AgentWorkflow(id: "w2", name: "two")
        var cards = AgentFeed.applyWorkflow([], workflow: first)
        cards = AgentFeed.applyWorkflow(cards, workflow: second)
        cards = AgentFeed.applyWorkflow(
            cards, workflow: AgentWorkflow(id: "w1", name: "one", status: .completed)
        )
        XCTAssertEqual(cards.map(\.id), ["w1", "w2"])
        XCTAssertEqual(cards[0].status, .completed)
        // The NEWEST still-running card speaks for the run.
        XCTAssertEqual(AgentFeed.runningWorkflow(cards)?.id, "w2")
        let done = cards.map { AgentWorkflow(id: $0.id, name: $0.name, status: .completed) }
        XCTAssertNil(AgentFeed.runningWorkflow(done))
    }

    func testTheStripListsEveryTaskThenEveryOpenWait() {
        let feed: [AgentFeedItem] = [
            .tool(
                id: 1, name: "TaskOutput", detail: "Sleep in the background",
                subagentId: nil, callId: "toolu_1", toolKind: "wait"
            ),
            .tool(
                id: 2, name: "Monitor", detail: "Watch the build",
                subagentId: nil, callId: "toolu_2", toolKind: "wait", settled: true
            ),
            .tool(
                id: 3, name: "Monitor", detail: nil,
                subagentId: nil, callId: "toolu_3", toolKind: "wait"
            ),
            tool(4),
        ]
        let tasks = [
            AgentBackgroundTask(id: "b1", kind: "shell", description: "Sleep in the background")
        ]
        let lines = AgentFeed.stripLines(backgroundTasks: tasks, feed: feed)
        XCTAssertEqual(
            lines.map(\.text),
            [
                "Sleep in the background",
                "Waiting on Sleep in the background",
                // A detail-less wait row falls back to the tool's own name.
                "Waiting on Monitor",
            ]
        )
        XCTAssertEqual(lines.map(\.kind), [.backgroundTask, .wait, .wait])
        // A SETTLED wait row is off the strip; nothing at all means no strip.
        XCTAssertTrue(AgentFeed.stripLines(backgroundTasks: [], feed: [tool(1)]).isEmpty)
    }

    func testTheWorkflowCardsPhaseAndAgentCaptions() {
        XCTAssertEqual(
            AgentFeed.workflowPhaseCaption(
                AgentWorkflowPhaseCounts(queued: 1, running: 2, done: 3, error: 1)
            ),
            "3 done · 2 running · 1 queued · 1 failed"
        )
        // Zero segments are omitted; nothing at all is an empty caption.
        XCTAssertEqual(
            AgentFeed.workflowPhaseCaption(AgentWorkflowPhaseCounts(done: 2)), "2 done"
        )
        XCTAssertEqual(AgentFeed.workflowPhaseCaption(AgentWorkflowPhaseCounts()), "")

        XCTAssertEqual(
            AgentFeed.workflowAgentTelemetry(AgentWorkflowAgent(
                index: 1, label: "alpha", state: .done,
                tokens: 12_400, toolCalls: 1, durationMs: 65_000
            )),
            "12.4k tokens · 1 tool · 1m 05s"
        )
        // A queued agent has reported nothing yet — no trailing telemetry.
        XCTAssertNil(
            AgentFeed.workflowAgentTelemetry(AgentWorkflowAgent(index: 2, label: "beta"))
        )
        XCTAssertEqual(
            AgentFeed.workflowAgentTelemetry(AgentWorkflowAgent(
                index: 3, label: "gamma", state: .running, toolCalls: 7
            )),
            "7 tools"
        )
    }

    /// The card's phase counts come off its own agents, phase by phase.
    func testPhaseCountsGroupTheCardsAgents() {
        let workflow = AgentWorkflow(
            id: "w1", name: "probe",
            phases: [
                AgentWorkflowPhase(index: 1, title: "Alpha"),
                AgentWorkflowPhase(index: 2, title: "Beta"),
            ],
            agents: [
                AgentWorkflowAgent(index: 1, label: "a", phaseIndex: 1, state: .done),
                AgentWorkflowAgent(index: 2, label: "b", phaseIndex: 1, state: .error),
                AgentWorkflowAgent(index: 3, label: "c", phaseIndex: 2, state: .running),
                AgentWorkflowAgent(index: 4, label: "d", phaseIndex: 2, state: .queued),
            ]
        )
        XCTAssertEqual(workflow.counts(inPhase: 1), AgentWorkflowPhaseCounts(done: 1, error: 1))
        XCTAssertEqual(
            workflow.counts(inPhase: 2), AgentWorkflowPhaseCounts(queued: 1, running: 1)
        )
        XCTAssertEqual(workflow.counts(inPhase: 7).total, 0)
        XCTAssertEqual(workflow.agents(inPhase: 2).map(\.label), ["c", "d"])
    }

    // MARK: - EXP-850 §5: the working caption

    func testTheTurnSlotKeepsWhatItLearnedAndResetsTokensOnANewTurn() {
        var slot = AgentFeed.applyTurn(
            AgentTurnSlot(), edge: AgentTurnEdge(state: .started, startedAt: 1000, tokens: 10)
        )
        XCTAssertEqual(slot, AgentTurnSlot(state: .started, startedAt: 1000, tokens: 10))
        // A republish of the same turn tops the count up…
        slot = AgentFeed.applyTurn(slot, edge: AgentTurnEdge(startedAt: 1000, tokens: 40))
        XCTAssertEqual(slot.tokens, 40)
        // …and an edge that carries neither never blanks what it learned.
        slot = AgentFeed.applyTurn(slot, edge: AgentTurnEdge(state: .started))
        XCTAssertEqual(slot, AgentTurnSlot(state: .started, startedAt: 1000, tokens: 40))
        // A NEW turn resets the count, because tokens are per turn.
        slot = AgentFeed.applyTurn(slot, edge: AgentTurnEdge(state: .started, startedAt: 9000))
        XCTAssertEqual(slot, AgentTurnSlot(state: .started, startedAt: 9000, tokens: nil))
    }

    func testWorkingDurationAndTokenFormats() {
        XCTAssertEqual(AgentFeed.workingDuration(ms: 0), "0s")
        XCTAssertEqual(AgentFeed.workingDuration(ms: -500), "0s")
        XCTAssertEqual(AgentFeed.workingDuration(ms: 37_400), "37s")
        XCTAssertEqual(AgentFeed.workingDuration(ms: 59_999), "59s")
        XCTAssertEqual(AgentFeed.workingDuration(ms: 124_000), "2m 04s")
        XCTAssertEqual(AgentFeed.workingDuration(ms: 600_000), "10m 00s")
        XCTAssertEqual(AgentFeed.workingDuration(ms: 3_780_000), "1h 03m")

        XCTAssertEqual(AgentFeed.workingTokens(0), "0")
        XCTAssertEqual(AgentFeed.workingTokens(812), "812")
        XCTAssertEqual(AgentFeed.workingTokens(999), "999")
        XCTAssertEqual(AgentFeed.workingTokens(2000), "2.0k")
        XCTAssertEqual(AgentFeed.workingTokens(1432), "1.4k")
        // Truncated, never rounded up — a count can never read `1000.0k`.
        XCTAssertEqual(AgentFeed.workingTokens(999_950), "999.9k")
        XCTAssertEqual(AgentFeed.workingTokens(1_234_567), "1.2M")
    }

    func testTheWorkingCaptionPicksItsVerbOffTheTurnStart() {
        let startedAt = 1_789_204_409_163
        // `verbs[startedAt % verbs.count]` — every client picks the same word
        // for the same turn, so it never flickers.
        XCTAssertEqual(AgentFeed.workingVerb(startedAt: startedAt), "Brewing")
        XCTAssertEqual(
            AgentFeed.workingVerb(startedAt: startedAt),
            DomainContract.steerWorkingVerbs[startedAt % DomainContract.steerWorkingVerbs.count]
        )
        let now = Date(timeIntervalSince1970: Double(startedAt + 124_000) / 1000)
        XCTAssertEqual(
            AgentFeed.workingCaption(startedAt: startedAt, tokens: 1432, now: now),
            "Brewing… (2m 04s · ↓ 1.4k tokens)"
        )
        // No token count yet: the duration stands alone.
        XCTAssertEqual(
            AgentFeed.workingCaption(startedAt: startedAt, tokens: nil, now: now),
            "Brewing… (2m 04s)"
        )
        // A publisher too old to stamp the turn's start falls back entirely.
        XCTAssertEqual(
            AgentFeed.workingCaption(startedAt: nil, tokens: 900, now: now), "Working…"
        )
    }

    func testARunningWorkflowTakesOverTheWorkingCaption() {
        let startedAt = 1_789_204_409_163
        let now = Date(timeIntervalSince1970: Double(startedAt + 124_000) / 1000)
        let workflow = AgentWorkflow(
            id: "w1", name: "wire-probe", status: .running,
            phases: [AgentWorkflowPhase(index: 1, title: "Alpha")],
            agents: [AgentWorkflowAgent(index: 1, label: "alpha:one", phaseIndex: 1, state: .running)]
        )
        XCTAssertEqual(
            AgentFeed.workingCaption(
                startedAt: startedAt, tokens: 1432, now: now, workflow: workflow
            ),
            "Workflow wire-probe · 0/1 agents done · Alpha (2m 04s · ↓ 1.4k tokens)"
        )
        // Even with no turn clock the card still names the work.
        XCTAssertEqual(
            AgentFeed.workingCaption(startedAt: nil, tokens: nil, now: now, workflow: workflow),
            "Workflow wire-probe · 0/1 agents done · Alpha"
        )
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
