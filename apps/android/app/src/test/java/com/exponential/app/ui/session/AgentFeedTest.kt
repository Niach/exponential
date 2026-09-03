package com.exponential.app.ui.session

import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.AnswerState
import com.exponential.app.domain.COMPACTED_LABEL
import com.exponential.app.domain.COMPACTING_LABEL
import com.exponential.app.domain.COMPACTION_TIMEOUT_MS
import com.exponential.app.domain.CompactionState
import com.exponential.app.domain.FEED_CAP
import com.exponential.app.domain.QuestionOption
import com.exponential.app.domain.SUBAGENT_FALLBACK_TYPE
import com.exponential.app.domain.activeQuestionIds
import com.exponential.app.domain.appendUserMessage
import com.exponential.app.domain.applyActivityEvent
import com.exponential.app.domain.capFeed
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
import com.exponential.app.domain.upsertQuestion
import com.exponential.app.domain.visibleSubagentTabs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
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
    fun `an id-less card is never answerable`() {
        // EXP-672: no wire id means no `answer` frame can address the card, so
        // it renders read-only ("Update the desktop app to answer this here.")
        // instead of offering a dead control.
        assertEquals(emptySet<Long>(), activeQuestionIds(listOf(idlessQuestion(1), idlessQuestion(2))))
        assertEquals(emptySet<Long>(), activeQuestionIds(listOf(idlessQuestion(1), tool(2))))
        // A plan card is no exception, however fresh it looks.
        assertEquals(emptySet<Long>(), activeQuestionIds(listOf(idlessQuestion(1).copy(planMode = true))))
        // A wire-id card next to one is unaffected.
        assertEquals(setOf(2L), activeQuestionIds(listOf(idlessQuestion(1), question(2))))
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
    fun `a card with no wire id always appends`() {
        val feed = listOf<AgentFeedItem>(idlessQuestion(1))
        assertEquals(2, upsertQuestion(feed, idlessQuestion(2)).size)
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
        assertEquals(listOf(3L, 4L), run.tools.map { it.id })
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
        assertTrue((rows[0] as AgentFeedRow.SubagentRun).tools.isEmpty())
        assertEquals(2, rows.size)
        // The orphan (its marker fell off the top) still forms its own group.
        val orphan = rows[1] as AgentFeedRow.SubagentRun
        assertEquals("s2", orphan.subagentId)
        assertEquals(SUBAGENT_FALLBACK_TYPE, orphan.agentType)
        assertEquals(listOf(2L), orphan.tools.map { it.id })
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
        assertEquals(listOf(3L, 6L), first.tools.map { it.id })
        val second = rows[1] as AgentFeedRow.SubagentRun
        assertEquals("s2", second.subagentId)
        assertEquals(listOf(5L), second.tools.map { it.id })
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
        assertTrue(run.tools.isEmpty())
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
        assertEquals(1, run.tools.size)
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
        ) { it.trim() == "ship it" }
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

    @Test
    fun `the feed is capped at two thousand events, oldest first`() {
        val overflow = (1L..(FEED_CAP + 5L)).map { AgentFeedItem.Narration(it, "n$it") }
        val capped = capFeed(overflow)
        assertEquals(FEED_CAP, capped.size)
        assertEquals(6L, capped.first().id)
        assertEquals(overflow.last(), capped.last())
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
        assertEquals(2, agents[0].tools.size)
        assertEquals("review", agents[1].agentType)
        assertFalse(agents[1].completed)
        assertTrue(agents[1].tools.isEmpty())
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

    // ── fixtures ────────────────────────────────────────────────────────────

    private fun ActivityFeedState.applying(event: JsonObject) = applyActivityEvent(event)

    private fun event(raw: String): JsonObject =
        Json.parseToJsonElement(raw.trimIndent()) as JsonObject

    private fun narration(text: String) = event("""{"kind":"narration","text":"$text"}""")

    private fun tool(id: Long) = AgentFeedItem.Tool(id, "Edit", "src/a.ts")

    private fun subagent(id: Long, subagentId: String, completed: Boolean) =
        AgentFeedItem.Subagent(id, subagentId, "explore", completed)

    private fun question(id: Long) = AgentFeedItem.Question(
        id = id,
        text = "Which color?",
        options = listOf(QuestionOption("Red", "1"), QuestionOption("Blue", "2")),
        multiSelect = false,
        wireId = "q$id",
    )

    /** A card from a pre-EXP-249 desktop: no wire id, so it is read-only. */
    private fun idlessQuestion(id: Long) = question(id).copy(wireId = null)

    private fun plan(id: Long) = question(id).copy(planMode = true)

    private fun step(askId: String, index: Int, feedId: Long, wireId: String = "w$feedId") =
        question(feedId).copy(wireId = wireId, askId = askId, index = index, total = 2)

    private fun submitStep(askId: String, feedId: Long, wireId: String = "w$feedId") =
        question(feedId).copy(wireId = wireId, askId = askId)
}
