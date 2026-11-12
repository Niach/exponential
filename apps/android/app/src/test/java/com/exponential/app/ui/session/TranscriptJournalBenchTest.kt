package com.exponential.app.ui.session

import at.exponential.ui.primitives.Markdown
import at.exponential.ui.primitives.MarkdownHooks
import at.exponential.ui.primitives.MarkdownOptions
import com.exponential.app.domain.ActivityFeedState
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.AskDock
import com.exponential.app.domain.DomainContract
import com.exponential.app.domain.FEED_WINDOW
import com.exponential.app.domain.activeQuestionIds
import com.exponential.app.domain.applyActivityEvent
import com.exponential.app.domain.collectSubagents
import com.exponential.app.domain.groupFeedRows
import com.exponential.app.domain.liveToolRowId
import com.exponential.app.domain.openWaits
import com.exponential.app.domain.splitWorkflowToolRows
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test

/**
 * Large-chat perf (Danny: "takes a few seconds to load each chat"): a REAL
 * device journal replayed the way a `join` sends it, through this client's
 * decode → fold → row projection → markdown, before (the whole journal) and
 * after (the contract's REPLAY TAIL). A measurement, not a gate: it skips
 * when the journals are not on disk. Copy them first:
 * `cp ~/Library/Application\ Support/at.exponential/journal/<id>.jsonl /tmp/exp-android-perf/`.
 */
class TranscriptJournalBenchTest {
    private val dir = File(System.getProperty("exp.perf.journals") ?: "/tmp/exp-android-perf")

    @Test
    fun realJournalsBeforeAndAfter() {
        val files = dir.listFiles { f -> f.name.endsWith(".jsonl") }.orEmpty().sortedBy { it.name }
        assumeTrue("no journals in $dir", files.isNotEmpty())
        // Warm the JIT on the first file once, then measure every file.
        measure(journalReplay(files.first()), "warmup", window = FEED_WINDOW, print = false)
        for (file in files) {
            val replay = journalReplay(file)
            val tail = replayTail(replay, DomainContract.steerFeedReplayTail)
            measure(replay, "${file.name} FULL (old window 1500)", window = 1500)
            measure(replay, "${file.name} FULL (window $FEED_WINDOW)", window = FEED_WINDOW)
            measure(tail, "${file.name} TAIL ${DomainContract.steerFeedReplayTail} (window $FEED_WINDOW)", window = FEED_WINDOW)
        }
    }
}

/** One replayed event: the journal line (the event), its seq, and whether it is a latest-wins slot. */
internal data class ReplayEvent(val seq: Long, val line: String, val kind: String?, val id: String?, val slot: Boolean)

private val SLOT_KINDS = setOf(
    "config_state", "usage", "context_layout", "rate_limit", "turn", "queue", "background_tasks", "task_list", "diff",
)

/** The journal as a join replays it (`read_journal_seq`): the log in order, then the latest-wins slots. */
internal fun journalReplay(file: File): List<ReplayEvent> = replayOf(file.readLines())

internal fun replayOf(lines: List<String>): List<ReplayEvent> {
    val log = mutableListOf<ReplayEvent>()
    val slots = linkedMapOf<String, ReplayEvent>()
    lines.forEachIndexed { index, line ->
        if (line.isBlank()) return@forEachIndexed
        val obj = runCatching { Json.parseToJsonElement(line).jsonObject }.getOrNull() ?: return@forEachIndexed
        val kind = (obj["kind"] as? JsonPrimitive)?.contentOrNull
        val id = (obj["id"] as? JsonPrimitive)?.contentOrNull
        val event = ReplayEvent(index.toLong(), line, kind, id, kind in SLOT_KINDS || kind == "workflow")
        when {
            kind in SLOT_KINDS -> { slots.remove(kind!!); slots[kind] = event }
            kind == "workflow" -> { slots.remove("workflow:$id"); slots["workflow:$id"] = event }
            else -> log += event
        }
    }
    return log + slots.values
}

/** `steer::history::replay_tail` (fixtures/steer-enet.json REPLAY TAIL). */
internal fun replayTail(events: List<ReplayEvent>, tail: Int): List<ReplayEvent> {
    val log = events.indices.filter { !events[it].slot }
    if (log.size <= tail) return events
    var cut = log.size - tail
    val floor = maxOf(0, cut - tail)
    val wanted = log.drop(cut).map { events[it] }.filter { it.kind == "tool_update" }.mapNotNull { it.id }.toSet()
    for (at in (floor until cut).reversed()) {
        val e = events[log[at]]
        if (e.kind == "tool" && e.id in wanted) cut = at
    }
    val first = log[cut]
    return events.filterIndexed { ix, e -> ix >= first || e.slot }
}

internal data class BenchResult(
    val events: Int,
    val wireBytes: Long,
    val decodeMs: Double,
    val foldMs: Double,
    val items: Int,
    val allRows: Int,
    val windowRows: Int,
    val rowsMs: Double,
    val perChangeMs: Double,
    val markdownRows: Int,
    val markdownMs: Double,
)

private fun ms(start: Long) = (System.nanoTime() - start) / 1e6

/** The client path a replay takes, timed stage by stage. */
internal fun measure(events: List<ReplayEvent>, label: String, window: Int, print: Boolean = true): BenchResult {
    val frames = events.map { """{"t":"activity","event":${it.line},"seq":${it.seq}}""" }
    val wireBytes = frames.sumOf { it.length.toLong() }
    var t = System.nanoTime()
    val decoded = frames.map { frame ->
        val obj = Json.parseToJsonElement(frame).jsonObject
        (obj["event"] as JsonObject) to (obj["seq"] as JsonPrimitive).contentOrNull?.toLong()
    }
    val decodeMs = ms(t)
    t = System.nanoTime()
    val state = decoded.fold(ActivityFeedState()) { s, (event, seq) -> s.applyActivityEvent(event, { false }, seq) }
    val foldMs = ms(t)
    val feed = state.feed
    t = System.nanoTime()
    val allRows = projectRows(feed, 0).size
    val windowStart = maxOf(0, feed.size - window)
    val rows = projectRows(feed, windowStart)
    val rowsMs = ms(t)
    // What the screen recomputes over the WHOLE feed on every activity change.
    t = System.nanoTime()
    activeQuestionIds(feed)
    collectSubagents(feed)
    liveToolRowId(feed)
    openWaits(feed)
    AskDock.feedCalls(feed)
    projectRows(feed, windowStart)
    val perChangeMs = ms(t)
    val markdown = rows.mapNotNull { row ->
        when (val item = (row as? AgentFeedRow.Single)?.item) {
            is AgentFeedItem.Narration -> item.text
            is AgentFeedItem.UserMessage -> item.text
            else -> null
        }
    }
    t = System.nanoTime()
    val hooks = MarkdownHooks(options = MarkdownOptions(hardBreaks = true, chat = true))
    markdown.forEach { Markdown.prepare(it, hooks) }
    val markdownMs = ms(t)
    val result = BenchResult(
        events = events.size, wireBytes = wireBytes, decodeMs = decodeMs, foldMs = foldMs, items = feed.size,
        allRows = allRows, windowRows = rows.size, rowsMs = rowsMs, perChangeMs = perChangeMs,
        markdownRows = markdown.size, markdownMs = markdownMs,
    )
    if (print) println("BENCH $label: $result")
    return result
}

/** The screen's row projection (`ActivityFeed`'s `rows`). */
internal fun projectRows(feed: List<AgentFeedItem>, from: Int): List<AgentFeedRow> =
    dropHiddenFeedRows(splitWorkflowToolRows(groupFeedRows(feed, from, emptySet()), emptySet()))

/**
 * The large-chat guard (fixtures/steer-enet.json REPLAY TAIL): a synthetic
 * 20k-event journal opens as its tail, paints at most the contract window,
 * and decodes + folds + projects well inside a CI-safe budget.
 */
class LargeTranscriptGuardTest {
    private fun syntheticJournal(events: Int): List<String> {
        val lines = mutableListOf<String>()
        var call = 0
        while (lines.size < events) {
            val n = lines.size
            when (n % 10) {
                0 -> lines += """{"kind":"user_message","text":"Step $n: **do** the `thing` #EXP-$n","at":$n}"""
                1, 5 -> lines += """{"kind":"narration","text":"Working on $n.\n\n- a list item\n- `code` and [a link](https://exponential.at)","messageId":"m$n","at":$n}"""
                2, 6 -> lines += """{"kind":"tool","id":"c${++call}","name":"Bash","detail":"bun test $n","toolKind":"execute","at":$n}"""
                3, 7 -> lines += """{"kind":"tool_update","id":"c$call","status":"completed","output":"${"ok ".repeat(40)}","at":$n}"""
                4 -> lines += """{"kind":"turn","state":"started","startedAt":$n,"at":$n}"""
                8 -> lines += """{"kind":"usage","contextSize":200000,"contextUsed":$n,"at":$n}"""
                else -> lines += """{"kind":"narration","text":"Done $n.","messageId":"d$n","at":$n}"""
            }
        }
        return lines
    }

    @Test
    fun aLargeJournalOpensAsItsTailAndPaintsAtMostTheWindow() {
        val replay = replayOf(syntheticJournal(20_000))
        val tail = replayTail(replay, DomainContract.steerFeedReplayTail)
        // The join replays the tail (plus the pulled-back calls), never the log.
        assertTrue("replayed ${tail.size}", tail.size <= 2 * DomainContract.steerFeedReplayTail + 16)
        measure(tail, "warmup", window = FEED_WINDOW, print = false)
        val open = measure(tail, "synthetic 20k TAIL", window = FEED_WINDOW)
        assertTrue("painted ${open.windowRows} rows", open.windowRows <= FEED_WINDOW)
        assertTrue(open.markdownRows <= FEED_WINDOW)
        // Decode + fold + rows + markdown of the window: a few ms on a laptop;
        // the budget only catches a return to whole-journal work.
        val openMs = open.decodeMs + open.foldMs + open.rowsMs + open.markdownMs
        assertTrue("open took $openMs ms", openMs < 750)
        // Even a viewer holding the WHOLE run paints only the window.
        val full = measure(replay, "synthetic 20k FULL", window = FEED_WINDOW)
        assertTrue("painted ${full.windowRows} rows", full.windowRows <= FEED_WINDOW)
        assertTrue("a change re-derived in ${full.perChangeMs} ms", full.perChangeMs < 250)
    }

    @Test
    fun scrollingNearTheTopPagesEarlierOnlyAfterTheReaderScrolledUp() {
        assertTrue(nearTranscriptTop(firstVisibleIndex = 3, visibleCount = 8, scrolledUp = true))
        assertTrue(!nearTranscriptTop(firstVisibleIndex = 3, visibleCount = 8, scrolledUp = false))
        assertTrue(!nearTranscriptTop(firstVisibleIndex = 20, visibleCount = 8, scrolledUp = true))
        assertTrue(!nearTranscriptTop(firstVisibleIndex = 0, visibleCount = 0, scrolledUp = true))
    }
}
