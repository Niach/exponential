package com.exponential.app.ui.session

import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.AgentRowClass
import com.exponential.app.domain.AnswerState
import com.exponential.app.domain.COMPACTED_LABEL
import com.exponential.app.domain.COMPACTING_LABEL
import com.exponential.app.domain.COMPACTION_TIMEOUT_MS
import com.exponential.app.domain.CONFIG_DEFAULT_VALUE_LABEL
import com.exponential.app.domain.PLAN_MODE_ID
import com.exponential.app.domain.PLAN_TOGGLE_LABEL
import com.exponential.app.domain.CONFIG_MODE_LABEL
import com.exponential.app.domain.CompactionState
import com.exponential.app.domain.ConfigCommand
import com.exponential.app.domain.ConfigMode
import com.exponential.app.domain.ConfigOption
import com.exponential.app.domain.ConfigValue
import com.exponential.app.domain.SessionConfigState
import com.exponential.app.domain.SessionRateLimitState
import com.exponential.app.domain.SessionUsageState
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.parseToolKind
import com.exponential.app.domain.rateLimitBannerShows
import com.exponential.app.domain.rateLimitClears
import com.exponential.app.domain.rateLimitExpired
import com.exponential.app.domain.rateLimitIsWall
import com.exponential.app.domain.feedItemBytes
import com.exponential.app.domain.TranscriptGap
import com.exponential.app.domain.modeChip
import com.exponential.app.domain.QuestionOption
import com.exponential.app.domain.SUBAGENT_FALLBACK_TYPE
import com.exponential.app.domain.TURN_STATE_ENDED
import com.exponential.app.domain.TURN_STATE_STARTED
import com.exponential.app.domain.agentWorking
import com.exponential.app.domain.clearTurn
import com.exponential.app.domain.label
import com.exponential.app.domain.activeQuestionIds
import com.exponential.app.domain.appendUserMessage
import com.exponential.app.domain.applyActivityEvent
import com.exponential.app.domain.askComplete
import com.exponential.app.domain.FEED_BYTE_CAP
import com.exponential.app.domain.feedItemBytes
import com.exponential.app.domain.trimmed
import com.exponential.app.domain.clearCompaction
import com.exponential.app.domain.collectSubagents
import com.exponential.app.domain.completeSubagent
import com.exponential.app.domain.currentStepperStep
import com.exponential.app.domain.failUnacknowledged
import com.exponential.app.domain.groupFeedRows
import com.exponential.app.domain.localAnswerSummary
import com.exponential.app.domain.lockAnswer
import com.exponential.app.domain.locksCard
import com.exponential.app.domain.resolveQuestions
import com.exponential.app.domain.rowClass
import com.exponential.app.domain.transcriptGap
import com.exponential.app.domain.upsertQuestion
import com.exponential.app.domain.visibleSubagentTabs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-249: a question card's lifetime is stated outright by the desktop's
// structured stream — a card carrying a WIRE ID stays answerable until its own
// `question_resolved` lands, wherever it sits in the feed. EXP-672: a card
// WITHOUT one came from a pre-EXP-249 desktop and is never answerable here (the
// raw-keystroke fallback that used to drive those is gone) — the screen renders
// it read-only with an update hint.
class AgentFeedTest {

    @Test
    fun `a wire-id question stays active behind any later event`() {
        val feed = listOf(
            question(1),
            tool(2),
            AgentFeedItem.Narration(3, "still working"),
            AgentFeedItem.Permission(4, "Bash"),
        )
        assertEquals(setOf(1L), activeQuestionIds(feed))
    }

    @Test
    fun `every unresolved wire-id card is active, in any position`() {
        val feed = listOf(question(1), tool(2), plan(3), AgentFeedItem.Narration(4, "x"))
        assertEquals(setOf(1L, 3L), activeQuestionIds(feed))
        assertEquals(emptySet<Long>(), activeQuestionIds(emptyList()))
    }

    @Test
    fun `a wire-id plan card survives lagged flushes and dies on resolution`() {
        val pending = listOf(plan(1), tool(2), AgentFeedItem.Narration(3, "x"))
        assertEquals(setOf(1L), activeQuestionIds(pending))
        val resolved = resolveQuestions(pending, id = "q1", askId = null, answers = listOf("Approve"))!!
        assertEquals(emptySet<Long>(), activeQuestionIds(resolved))
        assertEquals("Approve", (resolved[0] as AgentFeedItem.Question).answer)
    }

    @Test
    fun `a resolved question is never active`() {
        assertEquals(
            emptySet<Long>(),
            activeQuestionIds(listOf(question(1).copy(resolved = true, answer = "Red"))),
        )
        assertEquals(
            emptySet<Long>(),
            activeQuestionIds(listOf(plan(1).copy(resolved = true), question(2).copy(resolved = true))),
        )
    }

    // EXP-249: re-emission replaces a card in place.

    @Test
    fun `re-emitting a wire id replaces the card and keeps its feed id`() {
        val feed = listOf<AgentFeedItem>(tool(1), question(2).copy(wireId = "q1"))
        val augmented = question(9).copy(
            wireId = "q1",
            options = listOf(QuestionOption("Red", "1"), QuestionOption("Type something", "3")),
        )
        val out = upsertQuestion(feed, augmented)
        assertEquals(2, out.size)
        val card = out[1] as AgentFeedItem.Question
        assertEquals(2L, card.id)
        assertEquals(2, card.options.size)
        assertEquals("Type something", card.options[1].label)
    }

    @Test
    fun `re-emission never resurrects a resolved card`() {
        val feed = listOf<AgentFeedItem>(question(1).copy(wireId = "q1", resolved = true, answer = "Red"))
        val out = upsertQuestion(feed, question(2).copy(wireId = "q1"))
        val card = out[0] as AgentFeedItem.Question
        assertTrue(card.resolved)
        assertEquals("Red", card.answer)
    }

    @Test
    fun `a card with an unknown wire id appends`() {
        val feed = listOf<AgentFeedItem>(question(1).copy(wireId = "q1"))
        assertEquals(2, upsertQuestion(feed, question(2).copy(wireId = "q2")).size)
    }

    // EXP-249: question_resolved retires by id, else by askId.

    @Test
    fun `resolution by id retires only that card`() {
        val feed = listOf<AgentFeedItem>(
            question(1).copy(wireId = "a"),
            question(2).copy(wireId = "b"),
        )
        val out = resolveQuestions(feed, id = "a", askId = null, answers = listOf("Red"))!!
        assertEquals(true, (out[0] as AgentFeedItem.Question).resolved)
        assertEquals("Red", (out[0] as AgentFeedItem.Question).answer)
        assertEquals(false, (out[1] as AgentFeedItem.Question).resolved)
    }

    @Test
    fun `resolution by askId retires every step and maps answers in step order`() {
        val feed = listOf<AgentFeedItem>(
            step("ask1", index = 2, feedId = 1),
            tool(2),
            step("ask1", index = 1, feedId = 3),
            submitStep("ask1", feedId = 4),
        )
        val out = resolveQuestions(feed, id = null, askId = "ask1", answers = listOf("One", "Two"))!!
        val byFeedId = out.filterIsInstance<AgentFeedItem.Question>().associateBy { it.id }
        assertEquals("One", byFeedId[3L]?.answer)
        assertEquals("Two", byFeedId[1L]?.answer)
        // The submit step has no answer of its own but is retired too.
        assertEquals(null, byFeedId[4L]?.answer)
        assertTrue(byFeedId.values.all { it.resolved })
    }

    @Test
    fun `a dismissal keeps the cards answerless`() {
        val feed = listOf<AgentFeedItem>(question(1).copy(wireId = "a"))
        val out = resolveQuestions(feed, id = "a", askId = null, dismissed = true)!!
        assertTrue((out[0] as AgentFeedItem.Question).resolved)
        assertNull((out[0] as AgentFeedItem.Question).answer)
    }

    @Test
    fun `resolution of an unknown id or ask changes nothing`() {
        val feed = listOf<AgentFeedItem>(question(1).copy(wireId = "a"))
        assertNull(resolveQuestions(feed, id = "ghost", askId = null))
        assertNull(resolveQuestions(feed, id = null, askId = "ghost"))
    }

    // A `question_resolved` naming NEITHER an id nor an ask id (older
    // publishers emit those): retire every card still unresolved and land the
    // answers positionally (web/iOS parity).

    @Test
    fun `an id-less resolution retires every unresolved card in answer order`() {
        val feed = listOf<AgentFeedItem>(
            question(1).copy(resolved = true, answer = "Old"),
            question(2),
            tool(3),
            plan(4),
        )
        val out = resolveQuestions(feed, id = null, askId = null, answers = listOf("Red", "Approve"))!!
        val byFeedId = out.filterIsInstance<AgentFeedItem.Question>().associateBy { it.id }
        // The already-resolved card consumes no answer and keeps its own.
        assertEquals("Old", byFeedId[1L]?.answer)
        assertEquals("Red", byFeedId[2L]?.answer)
        assertEquals("Approve", byFeedId[4L]?.answer)
        assertTrue(byFeedId.values.all { it.resolved })
    }

    @Test
    fun `an id-less resolution skips submit steps and tolerates missing answers`() {
        val feed = listOf<AgentFeedItem>(
            step("ask1", index = 1, feedId = 1),
            submitStep("ask1", feedId = 2),
            question(3),
        )
        val out = resolveQuestions(feed, id = null, askId = null, answers = listOf("One"))!!
        val byFeedId = out.filterIsInstance<AgentFeedItem.Question>().associateBy { it.id }
        assertEquals("One", byFeedId[1L]?.answer)
        assertNull(byFeedId[2L]?.answer)
        // Answers ran out — the card still retires, answerless.
        assertNull(byFeedId[3L]?.answer)
        assertTrue(byFeedId.values.all { it.resolved })
    }

    @Test
    fun `an id-less dismissal retires every card answerless`() {
        val feed = listOf<AgentFeedItem>(question(1), plan(2))
        val out = resolveQuestions(feed, id = null, askId = null, dismissed = true)!!
        val cards = out.filterIsInstance<AgentFeedItem.Question>()
        assertTrue(cards.all { it.resolved })
        assertTrue(cards.all { it.answer == null })
        // Nothing left unresolved — a repeat matches nothing.
        assertNull(resolveQuestions(out, id = null, askId = null))
    }

    // EXP-97: consecutive runs of >=2 tool calls collapse into one render row.

    @Test
    fun `collapses runs of two or more consecutive tools, leaves the rest single`() {
        val feed = listOf(
            AgentFeedItem.Narration(1, "working"),
            tool(2),
            tool(3),
            tool(4),
            AgentFeedItem.UserMessage(5, "hi"),
            tool(6),
        )
        assertEquals(
            listOf<AgentFeedRow>(
                AgentFeedRow.Single(feed[0]),
                AgentFeedRow.ToolRun(listOf(tool(2), tool(3), tool(4))),
                AgentFeedRow.Single(feed[4]),
                AgentFeedRow.Single(feed[5]),
            ),
            groupFeedRows(feed),
        )
    }

    @Test
    fun `a lone tool between other kinds stays a single row`() {
        val feed = listOf(tool(1), AgentFeedItem.Narration(2, "x"), tool(3))
        assertEquals(feed.map { AgentFeedRow.Single(it) }, groupFeedRows(feed))
    }

    @Test
    fun `two runs split by a narration stay separate runs`() {
        val feed = listOf(tool(1), tool(2), AgentFeedItem.Narration(3, "x"), tool(4), tool(5))
        assertEquals(
            listOf<AgentFeedRow>(
                AgentFeedRow.ToolRun(listOf(tool(1), tool(2))),
                AgentFeedRow.Single(feed[2]),
                AgentFeedRow.ToolRun(listOf(tool(4), tool(5))),
            ),
            groupFeedRows(feed),
        )
    }

    @Test
    fun `an all-tool feed is one run and an empty feed has no rows`() {
        val feed = listOf(tool(1), tool(2), tool(3))
        assertEquals(listOf<AgentFeedRow>(AgentFeedRow.ToolRun(feed)), groupFeedRows(feed))
        assertEquals(emptyList<AgentFeedRow>(), groupFeedRows(emptyList()))
    }

    @Test
    fun `run id stays the first tool's id as the trailing run grows`() {
        val feed = listOf<AgentFeedItem>(AgentFeedItem.Narration(1, "x"), tool(2), tool(3))
        assertEquals(2L, groupFeedRows(feed)[1].id)
        assertEquals(2L, groupFeedRows(feed + tool(4))[1].id)
    }

    @Test
    fun `questions adjacent to tools are never absorbed into a run`() {
        val feed = listOf(tool(1), tool(2), question(3), question(4))
        assertEquals(
            listOf<AgentFeedRow>(
                AgentFeedRow.ToolRun(listOf(tool(1), tool(2))),
                AgentFeedRow.Single(feed[2]),
                AgentFeedRow.Single(feed[3]),
            ),
            groupFeedRows(feed),
        )
    }

    // EXP-249: askId steppers.

    @Test
    fun `every step of one ask collapses into a single stepper row`() {
        val feed = listOf<AgentFeedItem>(
            AgentFeedItem.Narration(1, "asking"),
            step("ask1", index = 1, feedId = 2),
            step("ask1", index = 2, feedId = 3),
            submitStep("ask1", feedId = 4),
        )
        val rows = groupFeedRows(feed)
        assertEquals(2, rows.size)
        val stepper = rows[1] as AgentFeedRow.QuestionStepper
        assertEquals("ask1", stepper.askId)
        assertEquals(listOf(1, 2, null), stepper.steps.map { it.index })
        // Anchored where the ask's first card landed.
        assertEquals(2L, stepper.id)
    }

    @Test
    fun `stepper steps sort by index with the submit step last`() {
        val feed = listOf<AgentFeedItem>(
            submitStep("ask1", feedId = 1),
            step("ask1", index = 2, feedId = 2),
            step("ask1", index = 1, feedId = 3),
        )
        val stepper = groupFeedRows(feed).single() as AgentFeedRow.QuestionStepper
        assertEquals(listOf(3L, 2L, 1L), stepper.steps.map { it.id })
        // The row key is the lowest feed id, not the re-sorted head.
        assertEquals(1L, stepper.id)
    }

    @Test
    fun `two asks stay two stepper rows`() {
        val feed = listOf<AgentFeedItem>(
            step("ask1", index = 1, feedId = 1),
            step("ask2", index = 1, feedId = 2),
        )
        assertEquals(listOf("ask1", "ask2"), groupFeedRows(feed).map { (it as AgentFeedRow.QuestionStepper).askId })
    }

    @Test
    fun `the stepper advances as answer locks land`() {
        // The screen passes ALL lock keys (Sending and Acked) — a step
        // advances the moment its answer is sent, and a dropped Sending lock
        // (5s no-ack timeout) re-surfaces it.
        val steps = listOf(
            step("ask1", index = 1, feedId = 1, wireId = "u#0"),
            step("ask1", index = 2, feedId = 2, wireId = "u#1"),
            submitStep("ask1", feedId = 3, wireId = "u#submit"),
        )
        assertEquals("u#0", currentStepperStep(steps, emptySet())?.wireId)
        assertEquals("u#1", currentStepperStep(steps, setOf("u#0"))?.wireId)
        assertEquals("u#submit", currentStepperStep(steps, setOf("u#0", "u#1"))?.wireId)
        assertNull(currentStepperStep(steps, setOf("u#0", "u#1", "u#submit")))
        // The dropped-lock rollback: u#1's lock expired, so it is current again.
        assertEquals("u#1", currentStepperStep(steps, setOf("u#0", "u#submit"))?.wireId)
    }

    // EXP-820: ONE completion rule for an ask (byte-identical ×4) — until it
    // holds, an answered step can still be re-answered and the stepper waits.

    @Test
    fun `an ask completes when its submit step resolves`() {
        val steps = listOf(
            step("ask1", index = 1, feedId = 1, wireId = "u#0").copy(resolved = true, answer = "Red"),
            step("ask1", index = 2, feedId = 2, wireId = "u#1").copy(resolved = true, answer = "Blue"),
            submitStep("ask1", feedId = 3, wireId = "u#submit"),
        )
        // Every numbered step resolved, the submit still open: still OPEN —
        // the engine re-records an earlier step until the submit lands.
        assertFalse(askComplete(steps))
        assertTrue(askComplete(steps.dropLast(1) + steps.last().copy(resolved = true)))
        // Resolved numbered steps with the submit step not yet seen: open too.
        assertFalse(askComplete(steps.dropLast(1)))
    }

    @Test
    fun `a one-question ask completes on its lone step`() {
        val lone = step("ask1", index = 1, feedId = 1, wireId = "u#0").copy(total = 1)
        assertFalse(askComplete(listOf(lone)))
        assertTrue(askComplete(listOf(lone.copy(resolved = true, answer = "Red"))))
        assertFalse(askComplete(emptyList()))
    }

    @Test
    fun `a dismissed step completes the whole ask`() {
        val steps = listOf(
            step("ask1", index = 1, feedId = 1, wireId = "u#0").copy(resolved = true, answer = "Red"),
            step("ask1", index = 2, feedId = 2, wireId = "u#1").copy(resolved = true, dismissed = true),
            submitStep("ask1", feedId = 3, wireId = "u#submit"),
        )
        assertTrue(askComplete(steps))
    }

    @Test
    fun `a dismissal naming a step and its ask retires every step`() {
        // `session/cancel` names the step that was showing AND its ask: the
        // whole ask is over, so the stepper must not surface the next step
        // (and keep the composer hidden behind an ask nobody can answer).
        val feed = listOf<AgentFeedItem>(
            step("ask1", index = 1, feedId = 1, wireId = "u#0").copy(resolved = true, answer = "Red"),
            step("ask1", index = 2, feedId = 2, wireId = "u#1"),
            step("ask1", index = 3, feedId = 3, wireId = "u#2"),
        )
        val out = resolveQuestions(feed, id = "u#1", askId = "ask1", dismissed = true)!!
        val cards = out.filterIsInstance<AgentFeedItem.Question>()
        assertTrue(cards.all { it.resolved && it.dismissed })
        assertEquals("Red", cards[0].answer)
        assertEquals(emptySet<Long>(), activeQuestionIds(out))
        assertTrue(askComplete(cards))
    }

    @Test
    fun `a re-answered step resolves again with its new answer`() {
        // EXP-820 back and forth: the engine re-records an earlier step and
        // publishes a fresh `question_resolved` for it — the already-resolved
        // card takes the new answer, and the lock the re-answer set is
        // released like a first answer's.
        val resolvedOnce = ActivityFeedState(
            feed = listOf(step("ask1", index = 1, feedId = 1, wireId = "u#0").copy(resolved = true, answer = "Red")),
        )
        val sent = resolvedOnce.lockAnswer("u#0", listOf("Blue"))
        assertEquals(AnswerState.Sending, sent.answerLocks["u#0"])
        val acked = sent.applying(event("""{"kind":"answer_ack","id":"u#0"}"""))
        assertEquals(AnswerState.Acked, acked.answerLocks["u#0"])
        val again = acked.applying(
            event("""{"kind":"question_resolved","id":"u#0","askId":"ask1","answers":["Blue"]}"""),
        )
        val card = again.feed.single() as AgentFeedItem.Question
        assertTrue(card.resolved)
        assertEquals("Blue", card.answer)
        assertTrue(again.answerLocks.isEmpty())
    }

    @Test
    fun `a resolved step is skipped even without an ack`() {
        val steps = listOf(
            step("ask1", index = 1, feedId = 1, wireId = "u#0").copy(resolved = true, answer = "Red"),
            step("ask1", index = 2, feedId = 2, wireId = "u#1"),
        )
        assertEquals("u#1", currentStepperStep(steps, emptySet())?.wireId)
    }

    // EXP-249: subagent groups.

    @Test
    fun `a subagent absorbs the tool calls that follow it`() {
        val feed = listOf<AgentFeedItem>(
            AgentFeedItem.Narration(1, "delegating"),
            subagent(2, "s1", completed = false),
            tool(3).copy(subagentId = "s1"),
            tool(4).copy(subagentId = "s1"),
            tool(5),
        )
        val rows = groupFeedRows(feed)
        assertEquals(3, rows.size)
        val run = rows[1] as AgentFeedRow.SubagentRun
        assertEquals(listOf(3L, 4L), run.items.map { it.id })
        assertEquals(2L, run.id)
        // A main-thread tool after the group is its own row.
        assertEquals(AgentFeedRow.Single(feed[4]), rows[2])
    }

    @Test
    fun `another subagent's tools are never absorbed`() {
        val feed = listOf<AgentFeedItem>(
            subagent(1, "s1", completed = false),
            tool(2).copy(subagentId = "s2"),
        )
        val rows = groupFeedRows(feed)
        assertTrue((rows[0] as AgentFeedRow.SubagentRun).items.isEmpty())
        assertEquals(2, rows.size)
        // The orphan (its marker fell off the top) still forms its own group.
        val orphan = rows[1] as AgentFeedRow.SubagentRun
        assertEquals("s2", orphan.subagentId)
        assertEquals(SUBAGENT_FALLBACK_TYPE, orphan.agentType)
        assertEquals(listOf(2L), orphan.items.map { it.id })
    }

    @Test
    fun `an interleaved fan-out groups every subagent's tools by id`() {
        // EXP-350: two subagents' tools interleave — positional grouping used
        // to strand every tool outside its group.
        val feed = listOf<AgentFeedItem>(
            subagent(1, "s1", completed = false),
            subagent(2, "s2", completed = false),
            tool(3).copy(subagentId = "s1"),
            AgentFeedItem.Narration(4, "checking in"),
            tool(5).copy(subagentId = "s2"),
            tool(6).copy(subagentId = "s1"),
        )
        val rows = groupFeedRows(feed)
        assertEquals(3, rows.size)
        val first = rows[0] as AgentFeedRow.SubagentRun
        assertEquals("s1", first.subagentId)
        assertEquals(listOf(3L, 6L), first.items.map { it.id })
        val second = rows[1] as AgentFeedRow.SubagentRun
        assertEquals("s2", second.subagentId)
        assertEquals(listOf(5L), second.items.map { it.id })
        assertEquals(AgentFeedRow.Single(feed[3]), rows[2])
    }

    @Test
    fun `a tagged tool never joins a main-thread tool run`() {
        val feed = listOf<AgentFeedItem>(
            tool(1),
            tool(2),
            tool(3).copy(subagentId = "s1"),
        )
        val rows = groupFeedRows(feed)
        assertEquals(2, rows.size)
        assertEquals(listOf(1L, 2L), (rows[0] as AgentFeedRow.ToolRun).items.map { it.id })
        assertEquals("s1", (rows[1] as AgentFeedRow.SubagentRun).subagentId)
    }

    @Test
    fun `a fallback-typed completed marker never degrades the label`() {
        // An old desktop's completed edge carries the "agent" fallback as a
        // SECOND marker (this client's completeSubagent normally folds it, but
        // a replayed log can hold both) — the real type must win.
        val feed = listOf<AgentFeedItem>(
            subagent(1, "s1", completed = false).copy(detail = "map the repo"),
            AgentFeedItem.Subagent(2, "s1", SUBAGENT_FALLBACK_TYPE, completed = true),
        )
        val run = groupFeedRows(feed)[0] as AgentFeedRow.SubagentRun
        assertEquals("explore", run.agentType)
        assertTrue(run.completed)
        assertEquals("map the repo", run.detail)
    }

    @Test
    fun `a completed-only marker keeps its type and renders a static empty group`() {
        val feed = listOf<AgentFeedItem>(subagent(1, "s1", completed = true))
        val run = groupFeedRows(feed)[0] as AgentFeedRow.SubagentRun
        assertEquals("explore", run.agentType)
        assertTrue(run.completed)
        assertTrue(run.items.isEmpty())
    }

    @Test
    fun `a reported tool-call count wins over the visible tool rows`() {
        // EXP-748: a replay evicts a subagent's tool events first, so the
        // count the publisher stamped on the completed edge is what keeps the
        // "N tool calls" caption honest.
        val evicted = listOf<AgentFeedItem>(
            subagent(1, "s1", completed = true).copy(toolCalls = 12),
            tool(2).copy(subagentId = "s1"),
        )
        assertEquals(12, (groupFeedRows(evicted)[0] as AgentFeedRow.SubagentRun).toolCount)

        // Never BELOW what the viewer can see: a stale or low report loses.
        val visible = listOf<AgentFeedItem>(
            subagent(1, "s1", completed = true).copy(toolCalls = 1),
            tool(2).copy(subagentId = "s1"),
            tool(3).copy(subagentId = "s1"),
        )
        assertEquals(2, (groupFeedRows(visible)[0] as AgentFeedRow.SubagentRun).toolCount)

        // An older desktop reports nothing — the rows ARE the count.
        val unreported = listOf<AgentFeedItem>(subagent(1, "s1", completed = true))
        assertEquals(0, (groupFeedRows(unreported)[0] as AgentFeedRow.SubagentRun).toolCount)
    }

    @Test
    fun `the completed edge folds its tool-call count onto the running row`() {
        val state = ActivityFeedState()
            .applying(
                event("""{"kind":"subagent","id":"s1","agentType":"explore","status":"started"}"""),
            )
            .applying(event("""{"kind":"tool","name":"Grep","subagentId":"s1"}"""))
            .applying(
                event(
                    """
                    {"kind":"subagent","id":"s1","agentType":"explore",
                     "status":"completed","detail":"9 files","toolCalls":9}
                    """,
                ),
            )
        // The completion folded into the running row, count and all.
        assertEquals(2, state.feed.size)
        val run = groupFeedRows(state.feed)[0] as AgentFeedRow.SubagentRun
        assertTrue(run.completed)
        assertEquals("9 files", run.detail)
        assertEquals(9, run.toolCount)
    }

    @Test
    fun `completion flips the running row in place instead of adding one`() {
        val feed = listOf<AgentFeedItem>(
            subagent(1, "s1", completed = false),
            tool(2).copy(subagentId = "s1"),
        )
        val out = completeSubagent(feed, "s1", "3 files")!!
        assertEquals(2, out.size)
        val header = out[0] as AgentFeedItem.Subagent
        assertTrue(header.completed)
        assertEquals("3 files", header.detail)
        assertNull(completeSubagent(out, "s1", null))
        assertNull(completeSubagent(out, "ghost", null))
    }

    // EXP-249: the decode path. Unknown kinds are skipped, never fatal, and
    // NOTHING here ever clears the feed — only the relay's activity_reset does
    // (the ViewModel drops the whole state on that frame).

    @Test
    fun `an unknown kind and a malformed event are skipped, not fatal`() {
        val start = ActivityFeedState().applying(narration("hello"))
        assertEquals(1, start.feed.size)
        val after = start
            .applying(event("""{"kind":"telemetry","payload":{"a":1}}"""))
            .applying(event("""{"kind":"tool"}"""))
            .applying(event("""{"kind":"narration","text":{"nested":true}}"""))
            .applying(event("""{"kind":"question","text":"hi","options":"not-an-array"}"""))
            .applying(event("""{}"""))
        assertEquals(start.feed, after.feed)
    }

    @Test
    fun `tool, permission and subagent events decode into rows`() {
        val state = ActivityFeedState()
            .applying(event("""{"kind":"subagent","id":"s1","agentType":"explore","status":"started"}"""))
            .applying(event("""{"kind":"tool","name":"Read","detail":"a.ts","subagentId":"s1"}"""))
            .applying(event("""{"kind":"subagent","id":"s1","agentType":"explore","status":"completed","detail":"done"}"""))
            .applying(event("""{"kind":"permission","tool":"Bash","detail":"rm -rf"}"""))
        val rows = groupFeedRows(state.feed)
        assertEquals(2, rows.size)
        val run = rows[0] as AgentFeedRow.SubagentRun
        assertTrue(run.completed)
        assertEquals("done", run.detail)
        assertEquals(1, run.items.size)
        assertEquals(
            AgentFeedItem.Permission(2, "Bash", "rm -rf"),
            (rows[1] as AgentFeedRow.Single).item,
        )
    }

    @Test
    fun `a question decodes its stepper fields, descriptions and multiSelect`() {
        val state = ActivityFeedState().applying(
            event(
                """{"kind":"question","id":"u#0","askId":"u","index":1,"total":2,
                   "header":"Colors","text":"Which colors?","multiSelect":true,
                   "options":[{"label":"Red","key":"1","description":"warm"},
                              {"label":"Blue","key":"2"}]}""",
            ),
        )
        val card = state.feed.single() as AgentFeedItem.Question
        assertEquals("u#0", card.wireId)
        assertEquals("u", card.askId)
        assertEquals(1, card.index)
        assertEquals(2, card.total)
        assertEquals("Colors", card.header)
        assertTrue(card.multiSelect)
        assertEquals("warm", card.options[0].description)
        assertNull(card.options[1].description)
    }

    @Test
    fun `an ack locks a card and a resolution releases it`() {
        val asked = ActivityFeedState()
            .applying(event("""{"kind":"question","id":"q1","text":"Which?","options":[{"label":"Red","key":"1"}]}"""))
            .lockAnswer("q1")
        assertEquals(AnswerState.Sending, asked.answerLocks["q1"])
        val acked = asked.applying(event("""{"kind":"answer_ack","id":"q1"}"""))
        assertEquals(AnswerState.Acked, acked.answerLocks["q1"])
        // An ack is proof of injection — the lock survives the timeout sweep.
        assertEquals(acked, acked.failUnacknowledged("q1"))
        val resolved = acked.applying(event("""{"kind":"question_resolved","id":"q1","answers":["Red"]}"""))
        assertTrue(resolved.answerLocks.isEmpty())
        assertEquals("Red", (resolved.feed.single() as AgentFeedItem.Question).answer)
    }

    @Test
    fun `a locked card remembers its picked labels until it rolls back`() {
        // EXP-588: the stepper shows WHAT was picked before the desktop's
        // resolution fills the real answer in; an expired lock forgets it.
        val sent = ActivityFeedState().lockAnswer("q1", listOf("Blue", "Green"))
        assertEquals("Blue, Green", sent.localAnswerSummary("q1"))
        val acked = sent.applying(event("""{"kind":"answer_ack","id":"q1"}"""))
        assertEquals("Blue, Green", acked.localAnswerSummary("q1"))
        assertNull(sent.failUnacknowledged("q1").localAnswerSummary("q1"))
        // A lock taken with no labels has no summary, not "".
        assertNull(ActivityFeedState().lockAnswer("q2").localAnswerSummary("q2"))
        assertNull(ActivityFeedState().localAnswerSummary("q1"))
    }

    @Test
    fun `an unacknowledged answer flips to Failed so it can be retried`() {
        // EXP-334: Failed (not removed) — the card re-surfaces WITH a retry
        // hint, no longer holds the stepper, and a re-tap re-locks it.
        val state = ActivityFeedState().lockAnswer("q1")
        val failed = state.failUnacknowledged("q1")
        assertEquals(AnswerState.Failed, failed.answerLocks["q1"])
        assertFalse(failed.answerLocks["q1"].locksCard())
        assertEquals(AnswerState.Sending, failed.lockAnswer("q1").answerLocks["q1"])
        // A LATE ack after the expiry still locks the card for good.
        val late = failed.applying(event("""{"kind":"answer_ack","id":"q1"}"""))
        assertEquals(AnswerState.Acked, late.answerLocks["q1"])
    }

    // EXP-483: prose from the withheld ask/plan entry flushes AFTER its
    // already-published card and splices back above it via beforeQuestionId.

    @Test
    fun `anchored narration splices above the first card of its ask`() {
        val state = ActivityFeedState()
            .applying(event("""{"kind":"question","id":"tu_1#0","askId":"tu_1","index":1,"total":2,"text":"Q1","options":[{"label":"Red","key":"1"}]}"""))
            .applying(event("""{"kind":"question","id":"tu_1#1","askId":"tu_1","index":2,"total":2,"text":"Q2","options":[{"label":"Big","key":"1"}]}"""))
            .applying(event("""{"kind":"narration","text":"the summary","beforeQuestionId":"tu_1"}"""))
        assertEquals(3, state.feed.size)
        assertEquals("the summary", (state.feed[0] as AgentFeedItem.Narration).text)
        assertEquals("tu_1#0", (state.feed[1] as AgentFeedItem.Question).wireId)
    }

    @Test
    fun `anchored narration matches a plan card by wire id, resolved or not`() {
        val state = ActivityFeedState()
            .applying(event("""{"kind":"question","id":"tu_plan","planMode":true,"text":"## Plan","options":[{"label":"Approve","key":"1"}]}"""))
            .applying(event("""{"kind":"question_resolved","id":"tu_plan","answers":["Approve"]}"""))
            .applying(event("""{"kind":"narration","text":"plan prose","beforeQuestionId":"tu_plan"}"""))
        assertEquals("plan prose", (state.feed[0] as AgentFeedItem.Narration).text)
        assertTrue((state.feed[1] as AgentFeedItem.Question).resolved)
    }

    @Test
    fun `successive anchored narrations keep their order`() {
        val state = ActivityFeedState()
            .applying(event("""{"kind":"question","id":"tu_1#0","askId":"tu_1","text":"Q1","options":[{"label":"Red","key":"1"}]}"""))
            .applying(event("""{"kind":"narration","text":"first","beforeQuestionId":"tu_1"}"""))
            .applying(event("""{"kind":"narration","text":"second","beforeQuestionId":"tu_1"}"""))
        assertEquals(
            listOf("first", "second"),
            state.feed.take(2).map { (it as AgentFeedItem.Narration).text },
        )
        assertTrue(state.feed[2] is AgentFeedItem.Question)
    }

    @Test
    fun `anchored narration with no matching card appends`() {
        val state = ActivityFeedState()
            .applying(narration("working"))
            .applying(event("""{"kind":"narration","text":"late","beforeQuestionId":"tu_gone"}"""))
        assertEquals("late", (state.feed[1] as AgentFeedItem.Narration).text)
    }

    @Test
    fun `the local echo of a steered message swallows its transcript twin`() {
        val echoed = ActivityFeedState().appendUserMessage("ship it")
        val once = echoed.applyActivityEvent(
            event("""{"kind":"user_message","text":"ship it"}"""),
            { it.trim() == "ship it" },
        )
        assertEquals(1, once.feed.size)
        val twice = once.applying(event("""{"kind":"user_message","text":"ship it"}"""))
        assertEquals(2, twice.feed.size)
    }

    @Test
    fun `a diff replaces the previous one and never enters the feed`() {
        val state = ActivityFeedState()
            .applying(event("""{"kind":"diff","diff":"one"}"""))
            .applying(event("""{"kind":"diff","diff":"two"}"""))
        assertEquals("two", state.latestDiff)
        assertTrue(state.feed.isEmpty())
    }

    // EXP-783: a run far past the OLD 2000-event cap keeps every event — the
    // screen windows, the reducer does not truncate.
    @Test
    fun `the feed keeps the whole run`() {
        var state = ActivityFeedState()
        repeat(5_000) { i -> state = state.applying(narration("line $i")) }
        assertEquals(5_000, state.feed.size)
        assertEquals("line 0", (state.feed.first() as AgentFeedItem.Narration).text)
        assertEquals("line 4999", (state.feed.last() as AgentFeedItem.Narration).text)
    }

    // …bounded by BYTES instead: the oldest go, the newest always survives.
    @Test
    fun `the byte budget evicts the oldest and always keeps one`() {
        val chunk = "x".repeat(64 * 1024)
        val overflow = (0L until 400L).map { AgentFeedItem.Narration(it, "$it$chunk") }
        val state = ActivityFeedState(
            feed = overflow,
            feedBytes = overflow.sumOf { feedItemBytes(it) },
        ).trimmed()
        assertTrue(state.feed.size < overflow.size)
        assertTrue(state.feed.isNotEmpty())
        assertTrue(state.feedBytes <= FEED_BYTE_CAP)
        assertEquals(overflow.last(), state.feed.last())
    }

    // EXP-783: a window that cuts a tool run re-keys the boundary row onto the
    // first item the reader can actually see, and `from = 0` is the whole
    // projection.
    @Test
    fun `a window restricts the projection without changing it`() {
        val feed = listOf<AgentFeedItem>(narrationItem(0, "hello")) +
            (1L..6L).map { tool(it) }
        assertEquals(groupFeedRows(feed), groupFeedRows(feed, 0))
        val windowed = groupFeedRows(feed, 4)
        assertEquals(1, windowed.size)
        assertEquals(4L, windowed.single().id)
    }

    // EXP-783: the wire sequence rides onto the row it produced, which is what
    // a replay splice and an older-page request are addressed by.
    @Test
    fun `the wire sequence lands on the row`() {
        val state = ActivityFeedState()
            .applying(narration("one"), seq = 7L)
            .applying(narration("two"), seq = 8L)
        assertEquals(listOf(7L, 8L), state.feed.map { it.seq })
    }

    @Test
    fun `a fresh state is what the relay's activity_reset restores`() {
        val busy = ActivityFeedState()
            .applying(narration("working"))
            .applying(event("""{"kind":"diff","diff":"one"}"""))
            .lockAnswer("q1")
        assertNotNull(busy.latestDiff)
        val reset = ActivityFeedState()
        assertTrue(reset.feed.isEmpty())
        assertNull(reset.latestDiff)
        assertTrue(reset.answerLocks.isEmpty())
        assertEquals(0L, reset.nextEventId)
    }

    @Test
    fun `collectSubagents lists every run in first-appearance order`() {
        // EXP-356: one conversation tab per subagent, summarized exactly like
        // the group rows (iOS/web parity).
        val feed = listOf(
            AgentFeedItem.Narration(1, "Delegating."),
            AgentFeedItem.Subagent(2, "a", "Explore", completed = false, detail = "map"),
            AgentFeedItem.Tool(3, "Grep", null, subagentId = "a"),
            tool(4),
            AgentFeedItem.Subagent(5, "b", "review", completed = false),
            AgentFeedItem.Tool(6, "Read", null, subagentId = "a"),
            AgentFeedItem.Subagent(7, "a", "Explore", completed = true, detail = "map"),
        )
        val agents = collectSubagents(feed)
        assertEquals(listOf("a", "b"), agents.map { it.subagentId })
        assertEquals("Explore", agents[0].agentType)
        assertTrue(agents[0].completed)
        assertEquals("map", agents[0].detail)
        assertEquals(2, agents[0].items.size)
        assertEquals("review", agents[1].agentType)
        assertFalse(agents[1].completed)
        assertTrue(agents[1].items.isEmpty())
        assertTrue(collectSubagents(listOf(tool(1))).isEmpty())
    }

    @Test
    fun `visibleSubagentTabs drops completed runs except the focused one`() {
        // EXP-387: the strip shows running subagents only — a completed run's
        // tab is dropped, unless it is the focused one (never yank the user
        // out mid-read); all-done with Main focused leaves the strip empty.
        val feed = listOf(
            subagent(1, "a", completed = false),
            subagent(2, "b", completed = false),
            subagent(3, "a", completed = true),
        )
        val agents = collectSubagents(feed)
        assertEquals(listOf("b"), visibleSubagentTabs(agents, null).map { it.subagentId })
        assertEquals(listOf("a", "b"), visibleSubagentTabs(agents, "a").map { it.subagentId })
        assertEquals(listOf("b"), visibleSubagentTabs(agents, "b").map { it.subagentId })

        val done = collectSubagents(listOf(subagent(1, "a", completed = true)))
        assertTrue(visibleSubagentTabs(done, null).isEmpty())
    }

    // ── Compaction (EXP-724) ────────────────────────────────────────────────

    @Test
    fun `a compaction is state until it ends, then leaves a marker row`() {
        val started = ActivityFeedState()
            .applying(narration("working"))
            .applying(event("""{"kind":"compaction","phase":"started","trigger":"manual"}"""))
        assertEquals(CompactionState("manual"), started.compacting)
        // The strip is state beside the diff — never a feed row.
        assertEquals(1, started.feed.size)
        assertTrue(started.feed.none { it is AgentFeedItem.Compaction })
        assertTrue(groupFeedRows(started.feed).none {
            it is AgentFeedRow.Single && it.item is AgentFeedItem.Compaction
        })

        val ended = started.applying(event("""{"kind":"compaction","phase":"ended"}"""))
        assertNull(ended.compacting)
        assertEquals(2, ended.feed.size)
        assertTrue(ended.feed.last() is AgentFeedItem.Compaction)
        // The marker is its own row in the projection.
        val rows = groupFeedRows(ended.feed)
        assertTrue((rows.last() as AgentFeedRow.Single).item is AgentFeedItem.Compaction)
    }

    @Test
    fun `a bare ended still marks and an unknown phase changes nothing`() {
        // Codex auto-compaction publishes no start at all.
        val bare = ActivityFeedState()
            .applying(event("""{"kind":"compaction","phase":"ended"}"""))
        assertNull(bare.compacting)
        assertEquals(1, bare.feed.size)
        assertTrue(bare.feed.single() is AgentFeedItem.Compaction)

        val odd = bare
            .applying(event("""{"kind":"compaction","phase":"paused"}"""))
            .applying(event("""{"kind":"compaction"}"""))
        assertEquals(bare.feed, odd.feed)
        assertNull(odd.compacting)
    }

    @Test
    fun `a start with no trigger is still a compaction and clears explicitly`() {
        val started = ActivityFeedState()
            .applying(event("""{"kind":"compaction","phase":"started"}"""))
        assertEquals(CompactionState(null), started.compacting)
        // The connection's 180s backstop drops the strip WITHOUT a marker —
        // nothing was observed to finish.
        val cleared = started.clearCompaction()
        assertNull(cleared.compacting)
        assertTrue(cleared.feed.isEmpty())
        assertEquals(180_000L, COMPACTION_TIMEOUT_MS)
    }

    @Test
    fun `the compaction labels are the ones every client shows`() {
        // Byte-identical to web, iOS and the desktop — the ellipsis is U+2026.
        assertEquals("Compacting context…", COMPACTING_LABEL)
        assertEquals("Context compacted", COMPACTED_LABEL)
    }

    // ── EXP-746: the live configuration + usage slots ───────────────────────

    @Test
    fun `config_state and usage are state beside the feed, never rows`() {
        val state = ActivityFeedState()
            .applying(narration("working"))
            .applying(configState())
            .applying(usage(124_000, 200_000, 1.235))

        // One narration, and nothing the new kinds appended.
        assertEquals(1, state.feed.size)
        assertTrue(state.feed.single() is AgentFeedItem.Narration)
        assertTrue(groupFeedRows(state.feed).size == 1)

        val config = state.config!!
        assertEquals(listOf("model", "effort"), config.options.map { it.id })
        assertEquals("Model", config.options.first().label)
        assertEquals("opus", config.options.first().value)
        assertEquals(listOf("opus", "sonnet"), config.options.first().values.map { it.id })
        assertEquals("plan", config.currentMode)
        assertEquals(listOf("plan", "auto"), config.modes.map { it.id })
        assertEquals(listOf("review", "usage"), config.commands.map { it.name })
        assertEquals("<path>", config.commands.first().hint)

        assertEquals(SessionUsageState(124_000, 200_000, 1.235), state.usage)
    }

    @Test
    fun `a newer config_state replaces the snapshot`() {
        val state = ActivityFeedState()
            .applying(configState())
            .applying(
                event(
                    """{"kind":"config_state","options":[""" +
                        """{"id":"model","label":"Model","value":"sonnet"}],"currentMode":"auto"}""",
                ),
            )
        // A snapshot, never a merge: the old options and modes are gone.
        assertEquals(listOf("model"), state.config?.options?.map { it.id })
        assertEquals("sonnet", state.config?.options?.first()?.value)
        assertEquals("auto", state.config?.currentMode)
        assertTrue(state.config?.modes.orEmpty().isEmpty())
    }

    @Test
    fun `a malformed config_state leaves the previous snapshot standing`() {
        val good = ActivityFeedState().applying(configState())
        val after = good
            .applying(event("""{"kind":"config_state"}"""))
            .applying(event("""{"kind":"config_state","options":"not-an-array"}"""))
        assertEquals(good.config, after.config)
        // A wholly unreadable usage frame keeps the numbers too.
        val used = good.applying(usage(10, 100, null))
        assertEquals(
            used.usage,
            used.applying(event("""{"kind":"usage","contextUsed":"lots"}""")).usage,
        )
    }

    // EXP-785/786: `tool_update` folds into the tool row by call id.
    @Test
    fun `a tool row carries its callId and toolKind`() {
        val state = ActivityFeedState()
            .applying(toolWithId("tc-1", "edit"))
            .applying(event("""{"kind":"tool","name":"X","id":"  ","toolKind":"teleport"}"""))
        val first = state.feed[0] as AgentFeedItem.Tool
        assertEquals("tc-1", first.callId)
        assertEquals("edit", first.toolKind)
        val second = state.feed[1] as AgentFeedItem.Tool
        assertNull(second.callId)
        assertNull(second.toolKind)
        assertEquals("switch_mode", parseToolKind("switch_mode"))
        assertNull(parseToolKind("teleport"))
        assertTrue(DomainContract.toolKindValues.contains("edit"))
    }

    @Test
    fun `a tool_update settles and diffs its row and never adds one`() {
        val diff = "--- a/src/a.ts\n+++ b/src/a.ts\n@@ -1 +1 @@\n-a\n+b\n"
        val base = ActivityFeedState()
            .applying(toolWithId("tc-1", "edit"))
            .applying(event("""{"kind":"narration","text":"between"}"""))
            .applying(toolWithId("tc-2", "execute"))
        val settled = base.applying(
            event("""{"kind":"tool_update","id":"tc-1","status":"completed","diff":"$diff"}"""),
        )
        assertEquals(3, settled.feed.size)
        val row = settled.feed[0] as AgentFeedItem.Tool
        assertTrue(row.settled)
        assertFalse(row.failed)
        assertEquals("--- a/src/a.ts\n+++ b/src/a.ts\n@@ -1 +1 @@\n-a\n+b\n", row.diff)
        assertFalse((settled.feed[2] as AgentFeedItem.Tool).settled)
        // The diff weighs against the budget.
        assertEquals(base.feedBytes + row.diff!!.length, settled.feedBytes)
        assertEquals(settled.feed.sumOf { feedItemBytes(it) }, settled.feedBytes)

        // A failed settle wins over the completed one; the diff stays.
        val failed = settled.applying(event("""{"kind":"tool_update","id":"tc-1","status":"failed"}"""))
        val failedRow = failed.feed[0] as AgentFeedItem.Tool
        assertTrue(failedRow.settled)
        assertTrue(failedRow.failed)
        assertEquals(row.diff, failedRow.diff)
        // A status-less update carrying only a diff never settles.
        val diffed = failed.applying(event("""{"kind":"tool_update","id":"tc-2","diff":"+x\n"}"""))
        val diffedRow = diffed.feed[2] as AgentFeedItem.Tool
        assertFalse(diffedRow.settled)
        assertEquals("+x\n", diffedRow.diff)
    }

    @Test
    fun `a tool_update for an unknown id is dropped and the newest row wins`() {
        val base = ActivityFeedState()
            .applying(toolWithId("tc-1", "read"))
            .applying(event("""{"kind":"tool","name":"Grep"}"""))
            .applying(toolWithId("tc-1", "read"))
        val dropped = base
            .applying(event("""{"kind":"tool_update","id":"tc-nope","status":"failed","diff":"+never\n"}"""))
            .applying(event("""{"kind":"tool_update","id":"","status":"completed"}"""))
        assertEquals(base, dropped)
        val next = base.applying(event("""{"kind":"tool_update","id":"tc-1","status":"completed"}"""))
        assertFalse((next.feed[0] as AgentFeedItem.Tool).settled)
        assertFalse((next.feed[1] as AgentFeedItem.Tool).settled)
        assertTrue((next.feed[2] as AgentFeedItem.Tool).settled)
    }

    // EXP-784: the rate-limit slot.
    @Test
    fun `rate_limit is a slot and an empty or ok status clears it`() {
        val limited = ActivityFeedState().applying(
            event(
                """{"kind":"rate_limit","status":" allowed_warning ","resetsAt":1700000000000,""" +
                    """"message":" 80% used "}""",
            ),
        )
        assertTrue(limited.feed.isEmpty())
        assertEquals(
            SessionRateLimitState("allowed_warning", 1_700_000_000_000L, "80% used"),
            limited.rateLimit,
        )
        val rejected = limited.applying(event("""{"kind":"rate_limit","status":"rejected","resetsAt":-1}"""))
        assertEquals(SessionRateLimitState("rejected"), rejected.rateLimit)
        assertNull(rejected.applying(event("""{"kind":"rate_limit","status":"ok"}""")).rateLimit)
        assertNull(rejected.applying(event("""{"kind":"rate_limit","status":""}""")).rateLimit)
        // Unreadable clears too: a stale banner beside a live run is worse.
        assertNull(rejected.applying(event("""{"kind":"rate_limit"}""")).rateLimit)
        assertTrue(rateLimitClears(" OK "))
        assertFalse(rateLimitClears("allowed"))
    }

    // EXP-818/831: the banner's gate — a wall (a rejection or a notice,
    // never a bare warning) whose reset is not more than a minute behind.
    @Test
    fun `the rate-limit banner is a wall inside its window`() {
        val now = 1_700_000_000_000L
        assertTrue(rateLimitIsWall(SessionRateLimitState("rejected")))
        assertTrue(rateLimitIsWall(SessionRateLimitState("allowed_warning", null, "You've hit your limit")))
        assertFalse(rateLimitIsWall(SessionRateLimitState("allowed_warning")))
        assertFalse(rateLimitIsWall(SessionRateLimitState("allowed_warning", null, "  ")))

        val past = SessionRateLimitState("rejected", now - 61_000L)
        val recent = SessionRateLimitState("rejected", now - 30_000L)
        assertTrue(rateLimitExpired(past, now))
        assertFalse(rateLimitExpired(recent, now))
        assertFalse(rateLimitExpired(SessionRateLimitState("rejected", now + 3_600_000L), now))
        assertFalse(rateLimitExpired(SessionRateLimitState("rejected"), now))

        assertFalse(rateLimitBannerShows(past, now))
        assertTrue(rateLimitBannerShows(recent, now))
        assertTrue(rateLimitBannerShows(SessionRateLimitState("rejected"), now))
        assertFalse(rateLimitBannerShows(SessionRateLimitState("allowed_warning"), now))
    }

    @Test
    fun `a zero context size clears the usage slot`() {
        // The engine reporting a zero window means "unknown", not "0%".
        val state = ActivityFeedState()
            .applying(usage(124_000, 200_000, null))
            .applying(usage(0, 0, null))
        assertNull(state.usage)
    }

    /** EXP-772: the mode is the ONLY steering chip left. Advertised options
     *  (the engine now publishes none) never reach the composer again. */
    @Test
    fun `only the mode reaches the composer`() {
        val chip = modeChip(
            SessionConfigState(
                options = listOf(
                    ConfigOption(
                        id = "model",
                        label = "Model",
                        value = "opus",
                        values = listOf(ConfigValue("opus", "Opus")),
                    ),
                    ConfigOption(id = "effort", label = "Effort", value = ""),
                ),
                currentMode = "plan",
                modes = listOf(
                    ConfigMode("plan", "Plan"),
                    ConfigMode("auto", "Auto"),
                    ConfigMode("ask", "Ask"),
                ),
            ),
        )!!
        assertEquals("Plan", chip.valueLabel)
        assertEquals(listOf("plan", "auto", "ask"), chip.values.map { it.id })
        // Three modes are a picker, not a switch.
        assertNull(chip.planToggle)
        // A run that advertises NO modes (codex) draws nothing at all — an
        // inert badge would be a control that cannot be operated.
        assertNull(modeChip(SessionConfigState()))
        assertNull(modeChip(null))
    }

    /** EXP-772: `plan` plus exactly one other mode is a yes/no question, so it
     *  collapses into the compact Plan switch whose off position is the other
     *  mode — claude's `plan` / `bypassPermissions` pair. */
    @Test
    fun `plan plus one other mode collapses into the switch`() {
        val modes = listOf(ConfigMode(PLAN_MODE_ID, "Plan"), ConfigMode("bypassPermissions", "Build"))
        val off = modeChip(
            SessionConfigState(currentMode = "bypassPermissions", modes = modes),
        )!!.planToggle!!
        assertEquals(false, off.on)
        assertEquals(PLAN_MODE_ID, off.planId)
        assertEquals("bypassPermissions", off.otherId)

        val on = modeChip(SessionConfigState(currentMode = PLAN_MODE_ID, modes = modes))!!
        assertEquals(true, on.planToggle!!.on)

        // A PAIR without a plan mode stays an ordinary picker.
        val pair = modeChip(
            SessionConfigState(
                currentMode = "a",
                modes = listOf(ConfigMode("a", "A"), ConfigMode("b", "B")),
            ),
        )!!
        assertNull(pair.planToggle)
    }

    @Test
    fun `the config default label is the one every client shows`() {
        // Byte-identical to web, iOS and the desktop.
        assertEquals("CLI default", CONFIG_DEFAULT_VALUE_LABEL)
        assertEquals("Mode", CONFIG_MODE_LABEL)
        assertEquals("Plan", PLAN_TOGGLE_LABEL)
        assertEquals("plan", PLAN_MODE_ID)
        // A mode in force the publisher never advertised still reads as
        // itself, never as the CLI default.
        val unknown = modeChip(
            SessionConfigState(currentMode = "sneaky", modes = listOf(ConfigMode("plan", "Plan"))),
        )!!
        assertEquals("sneaky", unknown.valueLabel)
    }

    // ── Narration merging + subagent scoping (EXP-772/EXP-773) ───────────────

    /** EXP-772: the engine flushes one assistant message in several narration
     *  events. Consecutive flushes of the SAME message id grow one bubble;
     *  anything in between opens a new one. */
    @Test
    fun `consecutive flushes of one message merge into one bubble`() {
        val state = ActivityFeedState()
            .applying(narration("Reading ", messageId = "m1"))
            .applying(narration("the file.", messageId = "m1"))
        val row = state.feed.single() as AgentFeedItem.Narration
        assertEquals("Reading the file.", row.text)
        // The merge consumed NO feed id — the row it grew already had one.
        assertEquals(1L, state.nextEventId)

        // A different message opens its own bubble, and so does prose that
        // resumed after a tool call.
        val split = state
            .applying(narration("Next thought.", messageId = "m2"))
            .applying(toolEvent("Edit"))
            .applying(narration("After.", messageId = "m2"))
        assertEquals(4, split.feed.size)

        // An id-less event (an older publisher) always opens its own bubble.
        val legacy = ActivityFeedState()
            .applying(narration("a"))
            .applying(narration("b"))
        assertEquals(2, legacy.feed.size)

        // Same message id from a different scope is a different bubble.
        val scoped = ActivityFeedState()
            .applying(narration("main", messageId = "m1"))
            .applying(narration("sub", messageId = "m1", subagentId = "s1"))
        assertEquals(2, scoped.feed.size)
    }

    /** EXP-773: a subagent's prose and the turns addressed to it leave the
     *  main feed and render inside that subagent's run, in publish order. */
    @Test
    fun `subagent prose and turns group under their run`() {
        val state = ActivityFeedState()
            .applying(narration("Delegating."))
            .applying(subagentStartedEvent("s1"))
            .applying(userMessageEvent("map the repo", subagentId = "s1"))
            .applying(narration("Looking.", subagentId = "s1"))
            .applying(toolEvent("Read", subagentId = "s1"))
            .applying(narration("Back on the main thread."))
        val rows = groupFeedRows(state.feed)
        // Main feed: the two unscoped narrations plus the group row.
        assertEquals(3, rows.size)
        val run = rows[1] as AgentFeedRow.SubagentRun
        assertEquals(listOf(2L, 3L, 4L), run.items.map { it.id })
        // The caption still counts TOOL calls, not conversation rows.
        assertEquals(1, run.toolCount)
    }

    // EXP-787: the transcript's gap ladder — ONE derivation, mirrored on all
    // four clients. Space sits ABOVE a row and is chosen from the row before
    // it; the first row gets none.
    @Test
    fun transcriptGapLadder() {
        val turn = AgentRowClass.Turn
        val prose = AgentRowClass.Prose
        val toolClass = AgentRowClass.Tool

        // The first row of the transcript hangs on nothing.
        assertEquals(TranscriptGap.None, transcriptGap(null, turn))
        assertEquals(TranscriptGap.None, transcriptGap(null, prose))
        assertEquals(TranscriptGap.None, transcriptGap(null, toolClass))

        // A human turn on EITHER side breathes widest — all 5 pairs.
        assertEquals(TranscriptGap.Turn, transcriptGap(turn, turn))
        assertEquals(TranscriptGap.Turn, transcriptGap(turn, prose))
        assertEquals(TranscriptGap.Turn, transcriptGap(turn, toolClass))
        assertEquals(TranscriptGap.Turn, transcriptGap(prose, turn))
        assertEquals(TranscriptGap.Turn, transcriptGap(toolClass, turn))

        // Two tool rows sit tightest; prose meeting one takes the middle step.
        assertEquals(TranscriptGap.Default, transcriptGap(toolClass, toolClass))
        assertEquals(TranscriptGap.Tool, transcriptGap(prose, toolClass))
        assertEquals(TranscriptGap.Tool, transcriptGap(toolClass, prose))

        // Two prose rows: a paragraph's worth.
        assertEquals(TranscriptGap.Block, transcriptGap(prose, prose))

        // The row classes themselves: every row shape lands where the ladder
        // expects it.
        assertEquals(turn, AgentFeedRow.Single(AgentFeedItem.UserMessage(0, "go")).rowClass)
        assertEquals(prose, AgentFeedRow.Single(narrationItem(1, "hi")).rowClass)
        assertEquals(prose, AgentFeedRow.Single(question(2)).rowClass)
        assertEquals(prose, AgentFeedRow.Single(AgentFeedItem.Compaction(3)).rowClass)
        assertEquals(
            prose,
            AgentFeedRow.QuestionStepper("ask", listOf(step("ask", 1, 4))).rowClass,
        )
        assertEquals(toolClass, AgentFeedRow.Single(tool(5)).rowClass)
        assertEquals(toolClass, AgentFeedRow.ToolRun(listOf(tool(6), tool(7))).rowClass)
        assertEquals(
            toolClass,
            AgentFeedRow.Single(AgentFeedItem.Permission(8, "Bash", "rm -rf")).rowClass,
        )
        assertEquals(toolClass, AgentFeedRow.Single(subagent(9, "s1", false)).rowClass)
        assertEquals(
            toolClass,
            AgentFeedRow.SubagentRun(
                id = 10,
                subagentId = "s1",
                agentType = "explore",
                completed = false,
                detail = null,
                items = emptyList(),
            ).rowClass,
        )
    }

    // ── EXP-846/847/848: the turn slot, subagent titles, tool previews ──────

    @Test
    fun `the turn slot is latest-wins state and defaults to ended`() {
        // Nothing has been said about a turn yet: idle, so nothing pulses.
        assertEquals(TURN_STATE_ENDED, ActivityFeedState().turnState)
        assertEquals(listOf("started", "ended"), DomainContract.turnStateValues)

        val started = ActivityFeedState()
            .applying(narration("working"))
            .applying(event("""{"kind":"turn","state":"started","at":1700000000000}"""))
        assertEquals(TURN_STATE_STARTED, started.turnState)
        // State beside the feed — never a row.
        assertEquals(1, started.feed.size)

        val ended = started.applying(event("""{"kind":"turn","state":"ended"}"""))
        assertEquals(TURN_STATE_ENDED, ended.turnState)
        assertEquals(1, ended.feed.size)

        // A state this build cannot name leaves the slot standing.
        val odd = started
            .applying(event("""{"kind":"turn","state":"thinking"}"""))
            .applying(event("""{"kind":"turn"}"""))
        assertEquals(TURN_STATE_STARTED, odd.turnState)

        // The session ending forces it back to idle.
        assertEquals(TURN_STATE_ENDED, started.clearTurn().turnState)
        assertSame(ended, ended.clearTurn())
    }

    @Test
    fun `the working rule needs a live run mid-turn with nothing parked on it`() {
        fun working(
            live: Boolean = true,
            sessionEnded: Boolean = false,
            turnState: String = TURN_STATE_STARTED,
            awaitingInput: Boolean = false,
            needsInput: Boolean = false,
            blocked: Boolean = false,
            compacting: Boolean = false,
        ) = agentWorking(
            live, sessionEnded, turnState, awaitingInput, needsInput, blocked, compacting,
        )

        assertTrue(working())
        // Every gate, one at a time.
        assertFalse(working(live = false))
        assertFalse(working(sessionEnded = true))
        assertFalse(working(turnState = TURN_STATE_ENDED))
        assertFalse(working(awaitingInput = true))
        assertFalse(working(needsInput = true))
        assertFalse(working(blocked = true))
        assertFalse(working(compacting = true))
    }

    @Test
    fun `a subagent carries the spawning call's description as its label`() {
        val state = ActivityFeedState()
            .applying(
                event(
                    """{"kind":"subagent","id":"s1","status":"started",
                       "agentType":"explore","title":"Find the sync regression"}""",
                ),
            )
            .applying(toolEvent("Read", subagentId = "s1"))
        val marker = state.feed.first() as AgentFeedItem.Subagent
        assertEquals("Find the sync regression", marker.title)

        val run = collectSubagents(state.feed).single()
        assertEquals("Find the sync regression", run.title)
        // The description is the label; the type stays available as a caption.
        assertEquals("Find the sync regression", run.label)
        assertEquals("explore", run.agentType)

        // A completed edge carrying none never erases it.
        val closed = state.applying(
            event("""{"kind":"subagent","id":"s1","status":"completed"}"""),
        )
        assertEquals("Find the sync regression", collectSubagents(closed.feed).single().label)

        // No title anywhere: the label IS the agent type.
        val untitled = ActivityFeedState().applying(subagentStartedEvent("s2"))
        assertEquals("explore", collectSubagents(untitled.feed).single().label)
    }

    @Test
    fun `a completed edge may be the first frame carrying the description`() {
        val state = ActivityFeedState()
            .applying(subagentStartedEvent("s1"))
            .applying(
                event(
                    """{"kind":"subagent","id":"s1","status":"completed","title":"Audit the shapes"}""",
                ),
            )
        val run = collectSubagents(state.feed).single()
        assertTrue(run.completed)
        assertEquals("Audit the shapes", run.label)
    }

    @Test
    fun `a tool_update folds an Exponential preview onto its call`() {
        val state = ActivityFeedState()
            .applying(toolWithId("call-1", "other"))
            .applying(
                event(
                    """{"kind":"tool_update","id":"call-1","status":"completed",
                       "preview":{"identifier":"EXP-849","title":"Drop pi",
                                  "url":"https://exp.test/i/1","count":3,"status":"in_review"}}""",
                ),
            )
        val row = state.feed.single() as AgentFeedItem.Tool
        assertTrue(row.settled)
        val preview = row.preview!!
        assertEquals("EXP-849", preview.identifier)
        assertEquals("Drop pi", preview.title)
        assertEquals("https://exp.test/i/1", preview.url)
        assertEquals(3, preview.count)
        assertEquals("in_review", preview.status)
        // It weighs against the feed budget like the other folded payloads.
        assertTrue(feedItemBytes(row) > feedItemBytes(row.copy(preview = null)))

        // A later update carrying none keeps it; an empty object is no preview.
        val kept = state.applying(
            event("""{"kind":"tool_update","id":"call-1","diff":"- a\n+ b"}"""),
        )
        assertEquals(preview, (kept.feed.single() as AgentFeedItem.Tool).preview)
        val none = ActivityFeedState()
            .applying(toolWithId("call-2", "other"))
            .applying(event("""{"kind":"tool_update","id":"call-2","preview":{}}"""))
        assertNull((none.feed.single() as AgentFeedItem.Tool).preview)
    }

    // ── fixtures ────────────────────────────────────────────────────────────

    private fun ActivityFeedState.applying(event: JsonObject, seq: Long? = null) =
        applyActivityEvent(event, { false }, seq)

    private fun event(raw: String): JsonObject =
        Json.parseToJsonElement(raw.trimIndent()) as JsonObject

    private fun narration(
        text: String,
        messageId: String? = null,
        subagentId: String? = null,
    ) = event(
        """{"kind":"narration","text":"$text"""" +
            (messageId?.let { ""","messageId":"$it"""" } ?: "") +
            (subagentId?.let { ""","subagentId":"$it"""" } ?: "") + "}",
    )

    private fun toolEvent(name: String, subagentId: String? = null) = event(
        """{"kind":"tool","name":"$name","detail":"src/a.ts"""" +
            (subagentId?.let { ""","subagentId":"$it"""" } ?: "") + "}",
    )

    private fun userMessageEvent(text: String, subagentId: String? = null) = event(
        """{"kind":"user_message","text":"$text"""" +
            (subagentId?.let { ""","subagentId":"$it"""" } ?: "") + "}",
    )

    private fun subagentStartedEvent(subagentId: String) =
        event("""{"kind":"subagent","id":"$subagentId","status":"started","agentType":"explore"}""")

    /** A full `config_state` snapshot — options, modes and agent commands. */
    private fun configState() = event(
        """
        {"kind":"config_state",
         "options":[
           {"id":"model","label":"Model","category":"model","value":"opus",
            "values":[{"id":"opus","label":"Opus"},{"id":"sonnet","label":"Sonnet"}]},
           {"id":"effort","label":"Effort","value":""}],
         "currentMode":"plan",
         "modes":[{"id":"plan","label":"Plan","description":"Ask first"},
                  {"id":"auto","label":"Auto"}],
         "commands":[{"name":"review","description":"Review the diff","hint":"<path>"},
                     {"name":"usage","description":"Show usage"}]}
        """,
    )

    private fun usage(used: Int, size: Int, cost: Double?) = event(
        """{"kind":"usage","contextUsed":$used,"contextSize":$size""" +
            (if (cost == null) "}" else ""","costUsd":$cost}"""),
    )

    private fun tool(id: Long) = AgentFeedItem.Tool(id, "Edit", "src/a.ts")

    private fun toolWithId(callId: String, kind: String) = event(
        """{"kind":"tool","name":"Edit","detail":"src/a.ts","id":"$callId","toolKind":"$kind"}""",
    )

    private fun narrationItem(id: Long, text: String) = AgentFeedItem.Narration(id, text)

    private fun subagent(id: Long, subagentId: String, completed: Boolean) =
        AgentFeedItem.Subagent(id, subagentId, "explore", completed)

    private fun question(id: Long) = AgentFeedItem.Question(
        id = id,
        text = "Which color?",
        options = listOf(QuestionOption("Red", "1"), QuestionOption("Blue", "2")),
        multiSelect = false,
        wireId = "q$id",
    )

    private fun plan(id: Long) = question(id).copy(planMode = true)

    private fun step(askId: String, index: Int, feedId: Long, wireId: String = "w$feedId") =
        question(feedId).copy(wireId = wireId, askId = askId, index = index, total = 2)

    private fun submitStep(askId: String, feedId: Long, wireId: String = "w$feedId") =
        question(feedId).copy(wireId = wireId, askId = askId)
}
