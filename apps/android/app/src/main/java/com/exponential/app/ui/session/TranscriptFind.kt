package com.exponential.app.ui.session

import com.exponential.app.domain.ChatExpress
import android.os.SystemClock
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.runtime.Composable
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.layout.LayoutCoordinates
import com.exponential.app.domain.DeepLinkDriver
import com.exponential.app.domain.WaitingCards
import kotlinx.coroutines.delay
import kotlin.math.abs
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import at.exponential.ui.primitives.Markdown
import at.exponential.ui.primitives.MarkdownHighlight
import at.exponential.ui.primitives.MarkdownHooks
import at.exponential.ui.primitives.MarkdownOptions
import at.exponential.ui.primitives.highlightCount
import at.exponential.ui.primitives.markdownHighlightCount
import com.exponential.app.data.chatstore.StoredChatHit
import com.exponential.app.data.steer.SteerSearchResult
import com.exponential.app.domain.AgentFeedItem
import com.exponential.app.domain.AgentFeedRow
import com.exponential.app.domain.Chats
import com.exponential.app.domain.ExpToolDisplay
import com.exponential.app.domain.FindInChat
import com.exponential.app.domain.MessageDelivery
import com.exponential.app.domain.SearchList
import com.exponential.app.domain.SlashCommands
import com.exponential.app.domain.TraceLineModel
import com.exponential.app.domain.Trace as TraceModels
import com.exponential.app.domain.TranscriptGrouping
import com.exponential.app.domain.groupFeedRows
import com.exponential.app.domain.isScriptCall
import com.exponential.app.domain.parseSteerMessage
import com.exponential.app.domain.splitWorkflowToolRows
import com.exponential.app.ui.chat.FindInChatState
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.onStart
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

// #EDE-130 (fixtures/find-in-chat.json): the transcript's half of find in
// chat on Android: counts the WHOLE loaded feed (not the painted window),
// merges the chat store's and the device's older hits, keeps the current
// stop while answers arrive, lands on it through THE deep-link scroll path
// ([rememberDeepLink]: page older until loaded, centre, ring) and reports
// the counter and status line to the bar ([FindInChatState]).

/** What find in chat asks of the session on show. */
internal class TranscriptFindHost(
    val sessionId: String,
    val rootId: String,
    /** The session's device: true = online, false = offline, null = unknown. */
    val deviceOnline: Boolean?,
    val deviceName: String,
    val searchStore: suspend (rootId: String, query: String) -> List<StoredChatHit>?,
    val searchDevice: suspend (query: String, beforeSeq: Long?) -> SteerSearchResult?,
    /** #EXP-2021 (find-in-chat.json THE STORE CATCHING UP): ticks every time
     *  the chat store takes in rows of this chat. */
    val storeTaken: Flow<Unit> = emptyFlow(),
)

internal val LocalTranscriptFindHost = staticCompositionLocalOf<TranscriptFindHost?> { null }

/** text-highlight.json: the dark theme's `match` / `matchCurrent` fills. */
internal object FindMarks {
    val Match = Color(0x4DFACC15)
    val MatchCurrent = Color(0x99F59E0B)
}

/** What the transcript's rows paint while a query is on. */
@Immutable
internal data class FindPaint(
    val query: String,
    /** The current stop's row and its local index (null = none painted). */
    val currentKey: String?,
    val currentLocal: Int,
    /** Tool rows whose output matches → that output's match count (the hint). */
    val outputHits: Map<String, Int>,
    /** Feed item id → its find key. */
    val keys: Map<Long, String>,
) {
    /** The genUI `highlight` of the row [key] ([highlight.current] = its local current). */
    fun highlight(key: String?): MarkdownHighlight? {
        if (key == null) return null
        return MarkdownHighlight(query, if (key == currentKey) currentLocal else -1, FindMarks.Match, FindMarks.MatchCurrent)
    }

    fun highlightOf(itemId: Long): MarkdownHighlight? = highlight(keys[itemId])

    fun outputHint(itemId: Long): String? = keys[itemId]?.let { outputHits[it] }?.let(FindInChat::outputHint)
}

internal val LocalFindPaint = compositionLocalOf<FindPaint?> { null }

/** The find group key of a folded tool run (stable for the run's first call). */
internal fun findGroupKey(items: List<AgentFeedItem>): String = "g:${items.first().id}"

/** The synced message a SYNCED TAIL row draws (`synced:<id>`), else null. */
internal fun syncedMessageId(item: AgentFeedItem): String? {
    val id = when (item) {
        is AgentFeedItem.Narration -> item.messageId
        is AgentFeedItem.UserMessage -> item.messageId
        else -> null
    }
    return id?.takeIf { it.startsWith(SYNCED_PREFIX) }?.removePrefix(SYNCED_PREFIX)?.takeIf { it.isNotEmpty() }
}

private const val SYNCED_PREFIX = "synced:"

/** Every feed item's find key: `e:<session>:<seq>`; #EXP-1975: a SYNCED
 *  TAIL row = `m:<messageId>`; an item without either (or one repeating a
 *  key already given) gets its own local key. */
internal fun transcriptFindKeys(feed: List<AgentFeedItem>, sessionId: String): Map<Long, String> {
    val keys = HashMap<Long, String>(feed.size)
    val seen = HashSet<String>()
    for (item in feed) {
        val seq = item.seq
        val synced = syncedMessageId(item)
        val base = when {
            seq != null -> FindInChat.eventKey(sessionId, seq)
            synced != null -> FindInChat.messageKey(synced)
            else -> "i:$sessionId:${item.id}"
        }
        keys[item.id] = if (seen.add(base)) base else "$base#${item.id}"
    }
    return keys
}

/** EXP-1324: a plain tool call's Trace line model (what [TraceToolRow] paints). */
internal fun traceToolModel(item: AgentFeedItem.Tool, live: Boolean): TraceLineModel {
    val tests = if (item.toolKind == "execute" && item.settled && item.output != null) {
        TraceModels.failedTests(TraceModels.parseTestResults(item.output))
    } else {
        null
    }
    return tests ?: TraceModels.tool(
        toolKind = item.toolKind ?: "other",
        name = item.name,
        detail = item.detail,
        settled = item.settled || !live,
        failed = item.failed,
    )
}

/** A plain tool line's title (what [TraceToolRow] paints), or null for a
 *  call drawn another way (a notice, a script card, a comment, our tools). */
internal fun findToolTitle(item: AgentFeedItem.Tool, live: Boolean): String? {
    if (TranscriptGrouping.traceNotice(item.name) != null || item.isScriptCall || isCommentCallCard(item)) return null
    if (ExpToolDisplay.forName(item.name, item.settled) != null) return null
    return traceToolModel(item, live).title
}

/** One countable loaded row with the feed item landing reveals ([itemId]
 *  null = an own bubble, drawn outside the feed rows). */
internal data class TranscriptFindRow(val row: FindInChat.Row, val itemId: Long?, val outputCount: Int)

/** #EXP-1975 (find-in-chat.json): an own bubble's find key: its synced row
 *  (`m:<messageId>`), else its outbox entry (`o:<clientId>`). */
internal fun ownBubbleFindKey(bubble: MessageDelivery.Bubble): String =
    bubble.messageId?.takeIf { it.isNotEmpty() }?.let(FindInChat::messageKey)
        ?: "o:${bubble.clientId ?: bubble.key}"

/** #EXP-1975: the own bubbles' rows ([bubbles] in drawn order: the sent
 *  inputs over a placeholder, then the own tail), each counting its text as
 *  [OwnTailBubbles] paints it; a key already counted is skipped. */
internal fun ownBubbleFindRows(
    bubbles: List<MessageDelivery.Bubble>,
    query: String,
    seen: MutableSet<String> = HashSet(),
    countMarkdown: (String) -> Int = { findMarkdownCount(it, query) },
): List<TranscriptFindRow> {
    if (query.isEmpty()) return emptyList()
    val out = ArrayList<TranscriptFindRow>(bubbles.size)
    for (bubble in bubbles) {
        val key = ownBubbleFindKey(bubble)
        if (!seen.add(key)) continue
        out += TranscriptFindRow(FindInChat.Row(key, if (bubble.text.isBlank()) 0 else countMarkdown(bubble.text)), null, 0)
    }
    return out
}

/** Markdown matches as genUI's Markdown paints them (its normal form). */
internal fun findMarkdownCount(text: String, query: String): Int {
    if (text.isBlank()) return 0
    val blocks = Markdown.prepare(text, MarkdownHooks(options = MarkdownOptions(hardBreaks = true, chat = true)))
    return markdownHighlightCount(blocks, query)
}

/**
 * #EDE-130 (find-in-chat.json): the loaded transcript's countable rows for
 * [query], top to bottom, over the WHOLE feed: a narration's and a person's
 * bubble text (as the bubbles draw it), a plain tool line's title (+ its
 * output's stop), a folded run's members (`group`); never thoughts, cards,
 * markers or a subagent's rows. #EXP-1975: then every own bubble drawn
 * after them ([ownBubbles]: the placeholder's sent inputs, the own tail).
 */
internal fun transcriptFindRows(
    feed: List<AgentFeedItem>,
    sessionId: String,
    query: String,
    workflowIds: Set<String> = emptySet(),
    liveRowId: Long? = null,
    agent: String? = null,
    ownBubbles: List<MessageDelivery.Bubble> = emptyList(),
    countMarkdown: (String) -> Int = { findMarkdownCount(it, query) },
): List<TranscriptFindRow> {
    if (query.isEmpty()) return emptyList()
    val keys = transcriptFindKeys(feed, sessionId)
    val shown = dropHiddenFeedRows(splitWorkflowToolRows(groupFeedRows(feed, 0, workflowIds), workflowIds))
    val out = ArrayList<TranscriptFindRow>()
    fun tool(item: AgentFeedItem.Tool, group: String?) {
        val title = findToolTitle(item, live = item.id == liveRowId) ?: return
        val output = item.output
        val outputCount = if (output != null && item.id != liveRowId) highlightCount(output, query) else 0
        out += TranscriptFindRow(FindInChat.Row(keys.getValue(item.id), highlightCount(title, query), group, outputCount > 0), item.id, outputCount)
    }
    for (row in shown.rows) {
        when (row) {
            is AgentFeedRow.ToolRun -> {
                val group = findGroupKey(row.items)
                row.items.forEach { tool(it, group) }
            }
            is AgentFeedRow.Single -> when (val item = row.item) {
                is AgentFeedItem.Narration -> if (!TranscriptGrouping.isBlankText(item.text)) {
                    // #EXP-2067: find reads the PURE prose (a block's raw is never a hit).
                    out += TranscriptFindRow(FindInChat.Row(keys.getValue(item.id), countMarkdown(ChatExpress.plainProse(item.text))), item.id, 0)
                }
                is AgentFeedItem.UserMessage -> {
                    val skip = shown.checkIns.containsKey(row.id) ||
                        Chats.commandDivider(item.text) != null ||
                        SlashCommands.commandFor(item.text, agent) != null
                    val input = if (skip) null else TranscriptGrouping.feedInput(item.text)
                    // #EXP-2067: an action input = its body (a marker without one).
                    val action = ChatExpress.parseActionInput(item.text)
                    if (action != null) {
                        if (action.body.isNotBlank()) {
                            out += TranscriptFindRow(FindInChat.Row(keys.getValue(item.id), countMarkdown(action.body)), item.id, 0)
                        }
                    } else if (!skip && input !is TranscriptGrouping.FeedInput.Marker) {
                        val text = (input as? TranscriptGrouping.FeedInput.Relay)?.body ?: item.text
                        out += TranscriptFindRow(FindInChat.Row(keys.getValue(item.id), countMarkdown(findUserMessageProse(text))), item.id, 0)
                    }
                }
                is AgentFeedItem.Tool -> if (item.diff.isNullOrEmpty() && item.callId !in workflowIds) tool(item, null)
                else -> Unit
            }
            else -> Unit
        }
    }
    if (ownBubbles.isNotEmpty()) {
        out += ownBubbleFindRows(ownBubbles, query, out.mapTo(HashSet()) { it.row.key }, countMarkdown)
    }
    return out
}

/** The searched prose of a person's bubble ([UserMessageBody]): its prose
 *  without the image and file lines; a message naming its images
 *  (`[Image #N]` pills) paints no marks. */
internal fun findUserMessageProse(text: String): String {
    val parsed = parseSteerMessage(text)
    val images = parsed.attachmentIds.isNotEmpty()
    if (images && parsed.markers.isNotEmpty()) return ""
    return if (images || parsed.files.isNotEmpty()) parsed.text else text
}

private data class OlderAnswers(
    val query: String,
    val store: List<FindInChat.StoreHit> = emptyList(),
    val device: List<FindInChat.DeviceHit> = emptyList(),
    val storePhase: FindInChat.Phase = FindInChat.Phase.None,
    val devicePhase: FindInChat.Phase = FindInChat.Phase.None,
)

/** The transcript's find: what the rows paint and the landing to drive. */
internal class TranscriptFind(
    val paint: FindPaint?,
    /** The pending landing ([rememberDeepLink]'s request; its id = the stop's key). */
    val request: CardScrollRequest?,
    /** The landing rings its row (an output or an older stop). */
    val ring: Boolean,
    /** Folded groups find opened (they stay open after the bar closes). */
    val openGroups: Set<String>,
    /** Feed item id → its find key. */
    val keys: Map<Long, String>,
)

/** Whether a transcript row draws the feed item keyed [key]. */
internal fun findRowHolds(row: AgentFeedRow, key: String, keys: Map<Long, String>): Boolean = when (row) {
    is AgentFeedRow.Single -> keys[row.item.id] == key
    is AgentFeedRow.ToolRun -> row.items.any { keys[it.id] == key }
    is AgentFeedRow.Edits -> row.items.any { keys[it.id] == key }
    is AgentFeedRow.ExpRun -> row.items.any { keys[it.id] == key }
    else -> false
}

/** #EXP-2051: the transcript list's item key of the own tail (ALL the
 *  viewer's own bubbles after the rows are ONE item). */
internal const val OWN_TAIL_ITEM_KEY = "own-tail"

/** #EXP-2051: where a find stop lands in the transcript's LazyColumn:
 *  [index] (to bring it into the layout), [itemKey] (THE item, found by
 *  key), [inner] = the stop is one bubble inside that item (the own tail),
 *  placed by its own position there, never the item's centre. */
internal data class FindListTarget(val index: Int, val itemKey: Any, val inner: Boolean)

/**
 * #EXP-2051 (find-in-chat.json AFTER AN OLDER LANDING: the host scrolls by
 * the stop's KEY): the list item of the stop keyed [key]: a row stop = the
 * row drawing it ([lead] = the items above the rows: Load earlier, the head
 * cards), an own bubble = the own-tail item at [ownTailIndex] with the
 * bubble inside it. Null = not drawn here (yet).
 */
internal fun findListTarget(
    key: String,
    rows: List<AgentFeedRow>,
    keys: Map<Long, String>,
    lead: Int,
    ownKeys: Set<String>,
    ownTailIndex: Int?,
): FindListTarget? {
    if (key in ownKeys) return ownTailIndex?.takeIf { it >= 0 }?.let { FindListTarget(it, OWN_TAIL_ITEM_KEY, inner = true) }
    val at = rows.indexOfFirst { findRowHolds(it, key, keys) }
    return if (at < 0) null else FindListTarget(lead + at, rows[at].id, inner = false)
}

/** The folded groups find opened (a group row opens itself when it is in). */
internal val LocalFindOpenGroups = compositionLocalOf<Set<String>> { emptySet() }

/**
 * The transcript half of find in chat (`ActivityFeed`): [find] = the bar
 * (null = no find here), [canLoadEarlier] = the loaded stretch does not
 * reach the session's first event.
 */
@Composable
internal fun rememberTranscriptFind(
    find: FindInChatState?,
    host: TranscriptFindHost?,
    feed: List<AgentFeedItem>,
    workflowIds: Set<String>,
    liveRowId: Long?,
    agent: String?,
    canLoadEarlier: Boolean,
    /** #EXP-1975: the own bubbles drawn after the feed, in drawn order (the
     *  placeholder's sent inputs, then the own tail). */
    ownBubbles: List<MessageDelivery.Bubble> = emptyList(),
    /** #EXP-1998: the synced messages the screen draws as bubbles (null = a
     *  live transcript holds them as its rows). */
    synced: FindInChat.Synced? = null,
): TranscriptFind {
    val query = if (host != null) find?.activeQuery.orEmpty() else ""
    val sessionId = host?.sessionId.orEmpty()

    // ── The loaded rows, counted off the main thread ─────────────────────
    var counted by remember { mutableStateOf<Pair<String, List<TranscriptFindRow>>>("" to emptyList()) }
    LaunchedEffect(query, feed, workflowIds, liveRowId, agent, sessionId, ownBubbles) {
        if (query.isEmpty()) {
            counted = "" to emptyList()
            return@LaunchedEffect
        }
        val rows = withContext(Dispatchers.Default) { transcriptFindRows(feed, sessionId, query, workflowIds, liveRowId, agent, ownBubbles) }
        counted = query to rows
    }
    val rows = if (counted.first == query) counted.second else emptyList()
    val keys = remember(feed, sessionId) { if (sessionId.isEmpty()) emptyMap() else transcriptFindKeys(feed, sessionId) }

    // ── Older hits (store, then device) ───────────────────────────────────
    val lowest = feed.firstNotNullOfOrNull { it.seq }
    val latestLowest by rememberUpdatedState(lowest)
    val latestFromStart by rememberUpdatedState(!canLoadEarlier)
    var older by remember { mutableStateOf(OlderAnswers("")) }
    LaunchedEffect(query, host) {
        if (query.isEmpty() || host == null) {
            older = OlderAnswers("")
            return@LaunchedEffect
        }
        val fromStart = latestFromStart
        val askDevice = !fromStart && host.deviceOnline == true
        older = OlderAnswers(
            query,
            storePhase = FindInChat.Phase.Searching,
            devicePhase = when {
                fromStart -> FindInChat.Phase.None
                askDevice -> FindInChat.Phase.Searching
                host.deviceOnline == false -> FindInChat.Phase.Offline
                else -> FindInChat.Phase.None
            },
        )
        launch {
            // #EXP-2021 (THE STORE CATCHING UP): asked again every time the
            // store takes in rows of this chat (no new debounce, the field
            // untouched; the current stop is kept by identity below), so a
            // bar opened before the store caught up finds what it took in
            // later. A newer take cancels an ask still running.
            host.storeTaken.onStart { emit(Unit) }.collectLatest {
                val hits = runCatching { host.searchStore(host.rootId, query) }.getOrNull().orEmpty()
                    .filter { it.kind in STORE_KINDS }
                    .map { FindInChat.StoreHit(it.sessionId, it.seq, it.messageId, it.kind) }
                if (older.query == query) older = older.copy(store = hits, storePhase = FindInChat.Phase.Done)
            }
        }
        if (askDevice) {
            launch {
                val result = runCatching { host.searchDevice(query, latestLowest) }.getOrNull()
                val hits = result?.hits.orEmpty().map { FindInChat.DeviceHit(it.seq, it.kind) }
                if (older.query == query) older = older.copy(device = hits, devicePhase = FindInChat.Phase.Done)
            }
        }
    }
    // #EXP-1998 (`olderCases`): an older hit is an event BELOW the loaded
    // stretch, or a synced message the screen would draw but does not show.
    val olderHits = remember(older, query, sessionId, lowest, synced) {
        if (older.query != query) emptyList() else FindInChat.older(sessionId, lowest, older.store, older.device, synced)
    }
    val stops = remember(rows, olderHits) { FindInChat.stops(rows.map { it.row }, olderHits) }

    // ── The current stop (kept by identity while answers arrive) ──────────
    var current by remember { mutableStateOf<FindInChat.Stop?>(null) }
    var nonce by remember { mutableIntStateOf(0) }
    var request by remember { mutableStateOf<CardScrollRequest?>(null) }
    var ring by remember { mutableStateOf(false) }
    var openGroups by remember(sessionId) { mutableStateOf<Set<String>>(emptySet()) }
    LaunchedEffect(query) {
        current = null
        if (query.isEmpty()) request = null
    }
    var index = current?.let { c -> stops.indexOfFirst { it.same(c) } } ?: -1
    if (index < 0 && stops.isNotEmpty()) index = stops.lastIndex
    val shown = stops.getOrNull(index)
    val land: (FindInChat.Stop) -> Unit = { stop ->
        current = stop
        val landing = FindInChat.land(stop)
        landing.openGroup?.let { group -> if (group !in openGroups) openGroups = openGroups + group }
        ring = landing.ring
        nonce += 1
        request = CardScrollRequest(stop.key, nonce)
    }
    // A new query starts at the newest stop (once the loaded rows are
    // counted for it) and lands there. #EDE-99: a global search pick starts
    // at its hit's stop instead (`globalChatStart`), re-resolved as answers
    // arrive until found or the store's answer is in (then the newest).
    val rowsCounted = counted.first == query
    val target = find?.target
    val storeIn = older.query == query && older.storePhase != FindInChat.Phase.Searching
    LaunchedEffect(current == null, shown, rowsCounted, target, stops, storeIn) {
        if (current != null || !rowsCounted) return@LaunchedEffect
        if (target != null && find != null) {
            // No stops yet (the feed or its synced tail still on its way):
            // nothing to land on, the target waits.
            val at = SearchList.globalChatStart(stops, target) ?: return@LaunchedEffect
            val found = stops[at].key == target.key && stops[at].output == target.output
            if (!found && !storeIn) return@LaunchedEffect
            find.dropTarget()
            land(stops[at])
            return@LaunchedEffect
        }
        if (shown != null) land(shown)
    }
    // #EXP-1998: an older stop that turned out to be no row of this screen
    // (its page-in found nothing to draw) vanishes: the newest stop takes
    // over and is landed on.
    val vanished = current?.let { c -> !c.loaded && rowsCounted && stops.none { it.same(c) } } == true
    LaunchedEffect(vanished, shown) {
        if (vanished && shown != null) land(shown)
    }
    val step = find?.step
    var stepSeen by remember { mutableIntStateOf(step?.n ?: 0) }
    val latestStops by rememberUpdatedState(stops)
    val latestIndex by rememberUpdatedState(index)
    LaunchedEffect(step) {
        if (step == null || step.n == stepSeen) return@LaunchedEffect
        stepSeen = step.n
        val list = latestStops
        val next = FindInChat.step(latestIndex.takeIf { it >= 0 }, list.size, step.step) ?: return@LaunchedEffect
        land(list[next])
    }

    // ── The bar's counter and status line ─────────────────────────────────
    val done = older.query == query && counted.first == query &&
        older.storePhase != FindInChat.Phase.Searching && older.devicePhase != FindInChat.Phase.Searching
    val counter = if (query.isEmpty()) "" else FindInChat.counter(index.takeIf { it >= 0 }, stops.size, done, compact = true)
    val status = if (query.isNotEmpty() && older.query == query) {
        FindInChat.status(older.storePhase, older.devicePhase, host?.deviceName.orEmpty())
    } else {
        null
    }
    LaunchedEffect(find, counter, status) {
        if (find != null && find.open) find.report(counter, status)
    }

    val outputHits = remember(rows) { rows.filter { it.outputCount > 0 }.associate { it.row.key to it.outputCount } }
    val mark = shown?.takeIf { it.loaded && !it.output }
    val paint = remember(query, mark?.key, mark?.local, outputHits, keys) {
        if (query.isEmpty()) null else FindPaint(query, mark?.key, mark?.local ?: -1, outputHits, keys)
    }
    return TranscriptFind(paint, if (query.isEmpty()) null else request, ring, openGroups, keys)
}

/** The store's hit kinds find in chat knows: the session's transcript events
 *  and, #EXP-1998, its synced messages (`message`, keyed `m:<id>`). */
private val STORE_KINDS = setOf("user_message", "narration", "tool", "tool_output", "message")

/** #EXP-1998 (find-in-chat.json): how many of [all] (oldest first) stay
 *  hidden: all but the newest [max], or none from [from] on when that
 *  reaches further back (TS `widenedIds`, desktop `widened_skip`). */
internal fun findWidenedSkip(all: List<String>, max: Int, from: String?): Int {
    val at = if (from == null) -1 else all.indexOf(from)
    return if (at >= 0 && all.size - at > max) at else maxOf(0, all.size - max)
}

/** The synced message of a find key (`m:<id>`), else null. */
internal fun findMessageId(key: String?): String? = key?.takeIf { it.startsWith("m:") }?.substring(2)?.takeIf { it.isNotEmpty() }

/**
 * #EXP-1998 (find-in-chat.json): the synced messages a not-live screen
 * draws as bubbles and how a landing pages an older one in: [shown] = the
 * drawn ones, [widen] = draw every drawable one from that message on,
 * [loadOlder] = load older synced messages.
 */
internal class TranscriptFindSynced(
    val synced: FindInChat.Synced?,
    val shown: Set<String>,
    val widen: (messageId: String) -> Unit,
    val loadOlder: () -> Unit,
) {
    /** A loaded page changed (the deep link's `loadedMark`). */
    val mark: Int get() = listOf(synced?.loaded?.size, synced?.drawable?.size, shown.size).hashCode()

    /** Whether paging can still bring the message [id] in. */
    fun canPage(id: String): Boolean {
        val s = synced ?: return false
        return if (id in s.drawable) id !in shown else id !in s.loaded && s.more
    }

    fun page(id: String) {
        val s = synced ?: return
        if (id in s.drawable) widen(id) else if (s.more) loadOlder()
    }
}

internal val LocalTranscriptFindSynced = compositionLocalOf<TranscriptFindSynced?> { null }

/** #EXP-1998: where the no-transcript placeholder's bubbles sit in their
 *  scrolling column, and the one a landing rings. */
internal class FindBubbleLanding(val ring: String?, val onPlaced: (key: String, coordinates: LayoutCoordinates) -> Unit)

internal val LocalFindBubbleLanding = compositionLocalOf<FindBubbleLanding?> { null }

/**
 * #EXP-1998 (find-in-chat.json LANDING under the no-transcript placeholder):
 * its bubbles are one scrolling column, so a stop's landing is THE deep-link
 * steps over it: page the message in ([page]) until its bubble is drawn
 * ([centreOf] = its centre in the column, null = not drawn), centre it, ring
 * it. Returns the ringing key.
 */
@Composable
internal fun rememberBubbleFindLanding(
    request: CardScrollRequest?,
    ring: Boolean,
    scroll: ScrollState,
    centreOf: (key: String) -> Int?,
    more: (key: String) -> Boolean,
    loadedMark: () -> Int,
    page: (key: String) -> Unit,
): String? {
    val latestCentreOf by rememberUpdatedState(centreOf)
    val latestMore by rememberUpdatedState(more)
    val latestMark by rememberUpdatedState(loadedMark)
    val latestPage by rememberUpdatedState(page)
    val latestRing by rememberUpdatedState(ring)
    var ringing by remember { mutableStateOf<Pair<String, Long>?>(null) }
    LaunchedEffect(request) {
        val req = request ?: return@LaunchedEffect
        val driver = DeepLinkDriver()
        driver.take(req.id, req.takenAtMs)
        val drags = launch {
            scroll.interactionSource.interactions.collect { if (it is DragInteraction.Start) driver.drop() }
        }
        try {
            while (driver.link != null) {
                withFrameNanos { }
                val centre = latestCentreOf(req.id)
                val target = centre?.let { (it - scroll.viewportSize / 2).coerceIn(0, scroll.maxValue) }
                val step = driver.step(
                    found = centre != null,
                    laidOut = centre != null,
                    centred = target != null && abs(target - scroll.value) <= 2,
                    more = latestMore(req.id),
                    loadedMark = latestMark(),
                    nowMs = SystemClock.uptimeMillis(),
                ) ?: break
                when (step) {
                    WaitingCards.DeepLinkStep.Ring -> if (latestRing) ringing = req.id to SystemClock.uptimeMillis()
                    WaitingCards.DeepLinkStep.Scroll -> target?.let { scroll.animateScrollTo(it) }
                    WaitingCards.DeepLinkStep.Page -> latestPage(req.id)
                    else -> Unit
                }
            }
        } finally {
            drags.cancel()
        }
    }
    val rung = ringing
    LaunchedEffect(rung) {
        if (rung == null) return@LaunchedEffect
        delay(WaitingCards.Constants.HIGHLIGHT_MS)
        ringing = null
    }
    return rung?.first
}
