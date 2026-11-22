package com.exponential.app.domain

/**
 * #EDE-129 / #EDE-130 (`packages/domain-contract/fixtures/find-in-chat.json`,
 * its `$comment` = the spec ×4): find in chat. Pure: the menu entry, the
 * query, the stops in reading order, the counter, the steps, keeping the
 * current stop while answers arrive, what a landing does, the status line
 * and the keys. Mirrors the TS reference `src/find-in-chat.ts`, desktop
 * `domain::find_in_chat` and iOS; [FindInChatTest] replays every case.
 */
object FindInChat {
    const val QUERY_MAX_CHARS = 200
    const val HITS_MAX = 50
    const val DEBOUNCE_MS = 150L

    object Copy {
        const val MENU = "Search in chat"
        const val PLACEHOLDER = "Search in chat"
        const val COUNTER = "{n} of {m}"
        const val COUNTER_COMPACT = "{n}/{m}"
        const val NO_RESULTS = "No results"
        const val SEARCHING_OLDER = "Searching older messages…"
        const val SEARCHING_DEVICE = "Searching older messages on {device}…"
        const val DEVICE_OFFLINE = "{device} is offline: older tool output not searched"
        const val OUTPUT_HINT_ONE = "1 match in output"
        const val OUTPUT_HINT_MANY = "{n} matches in output"
        const val PREVIOUS = "Previous match"
        const val NEXT = "Next match"
        const val CLOSE = "Close search"
    }

    object TestIds {
        const val BAR = "find-in-chat"
        const val INPUT = "find-in-chat-input"
        const val COUNTER = "find-in-chat-counter"
        const val PREVIOUS = "find-in-chat-previous"
        const val NEXT = "find-in-chat-next"
        const val CLOSE = "find-in-chat-close"
        const val STATUS = "find-in-chat-status"
        const val OUTPUT_HINT = "find-in-chat-output-hint"
    }

    const val SHORTCUT_MAC = "⌘F"
    const val SHORTCUT_OTHER = "Ctrl F"

    /** The `…` entry's wire id (the menu's `search`). */
    const val SEARCH = "search"
    const val SEPARATOR = "separator"

    private val KINDS_WITH_TRANSCRIPT = setOf("session", "agent", "assistant")

    /** Whether a screen of [kind] has a transcript to find in (never an app). */
    fun searchable(kind: String?): Boolean = kind in KINDS_WITH_TRANSCRIPT

    /** The `…` menu with `search` first in its own group (an app: unchanged). */
    fun menu(kind: String?, entries: List<String>): List<String> = when {
        !searchable(kind) -> entries
        entries.isEmpty() -> listOf(SEARCH)
        else -> listOf(SEARCH, SEPARATOR) + entries
    }

    private fun isAsciiSpace(c: Char): Boolean = c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\u000C' || c == '\u000B'

    /** The query as the chat store takes it: ASCII whitespace runs = one
     *  space, trimmed, the first [QUERY_MAX_CHARS] code points. */
    fun query(raw: String): String {
        val out = StringBuilder()
        var space = false
        for (c in raw) {
            if (isAsciiSpace(c)) {
                space = true
                continue
            }
            if (space && out.isNotEmpty()) out.append(' ')
            space = false
            out.append(c)
        }
        val collapsed = out.toString()
        val points = collapsed.codePointCount(0, collapsed.length)
        if (points <= QUERY_MAX_CHARS) return collapsed
        return collapsed.substring(0, collapsed.offsetByCodePoints(0, QUERY_MAX_CHARS))
    }

    /** A loaded row, top to bottom; [count] = its painted highlight count. */
    data class Row(
        val key: String,
        val count: Int,
        val group: String? = null,
        /** A tool row whose output matches: one more stop, ringing the row. */
        val outputHit: Boolean = false,
    )

    /** An older hit (store, then device), newest first. */
    data class OlderHit(val key: String, val output: Boolean = false)

    /** #EXP-1998: a store hit as [older] reads it (an event's carries its
     *  [seq], a synced message's its [messageId]). */
    data class StoreHit(val sessionId: String, val seq: Long? = null, val messageId: String? = null, val kind: String)

    /** #EXP-1998: a device's hit (always the shown session's event). */
    data class DeviceHit(val seq: Long, val kind: String)

    /** #EXP-1998: the synced messages the host draws as bubbles (null = a
     *  live transcript, which holds them as its own rows): [loaded] = every
     *  loaded synced message of the session, [drawable] = those it would draw
     *  with no cap, [more] = older ones exist unloaded. */
    data class Synced(val loaded: List<String>, val drawable: List<String>, val more: Boolean)

    /** #EXP-1998 (`olderCases`): the store's and the device's hits OLDER than
     *  the loaded rows, store first, first key wins. */
    fun older(sessionId: String, lowestSeq: Long?, store: List<StoreHit>, device: List<DeviceHit>, synced: Synced?): List<OlderHit> {
        val loaded = synced?.loaded?.toHashSet().orEmpty()
        val drawable = synced?.drawable?.toHashSet().orEmpty()
        val seen = HashSet<String>()
        val out = ArrayList<OlderHit>()
        fun push(key: String, output: Boolean) {
            if (seen.add(key)) out += OlderHit(key, output)
        }
        fun event(seq: Long, kind: String) {
            if (lowestSeq == null || seq < lowestSeq) push(eventKey(sessionId, seq), kind == "tool_output")
        }
        for (hit in store) {
            if (hit.sessionId != sessionId) continue
            val seq = hit.seq
            val id = hit.messageId
            if (seq != null) {
                event(seq, hit.kind)
            } else if (id != null && synced != null) {
                if (if (id in loaded) id in drawable else synced.more) push(messageKey(id), false)
            }
        }
        for (hit in device) event(hit.seq, hit.kind)
        return out
    }

    /** One stop of the counter. */
    data class Stop(
        val key: String,
        val local: Int,
        val loaded: Boolean,
        val group: String?,
        val output: Boolean,
    ) {
        fun same(other: Stop): Boolean = key == other.key && local == other.local && output == other.output
    }

    /** Every stop in reading order (newest last): the older hits not loaded
     *  (oldest first, one each), then the loaded rows' matches and output stops. */
    fun stops(rows: List<Row>, older: List<OlderHit>): List<Stop> {
        val loaded = rows.mapTo(HashSet()) { it.key }
        val seen = HashSet<String>()
        val old = ArrayList<Stop>()
        for (hit in older) {
            if (hit.key in loaded || !seen.add(hit.key)) continue
            old += Stop(hit.key, 0, loaded = false, group = null, output = hit.output)
        }
        old.reverse()
        val out = old
        for (row in rows) {
            for (local in 0 until row.count) out += Stop(row.key, local, loaded = true, group = row.group, output = false)
            if (row.outputHit) out += Stop(row.key, 0, loaded = true, group = row.group, output = true)
        }
        return out
    }

    /** The counter text ("" = none shown); [current] is 0-based. */
    fun counter(current: Int?, total: Int, done: Boolean, compact: Boolean): String {
        if (total == 0 || current == null) return if (done && total == 0) Copy.NO_RESULTS else ""
        val template = if (compact) Copy.COUNTER_COMPACT else Copy.COUNTER
        return template.replace("{n}", (current + 1).toString()).replace("{m}", total.toString())
    }

    enum class Step(val wire: String) { Start("start"), Previous("previous"), Next("next") }

    /** The stop after a step (wraps); `start` = the newest. */
    fun step(current: Int?, total: Int, step: Step): Int? {
        if (total <= 0) return null
        if (step == Step.Start || current == null) return total - 1
        return if (step == Step.Previous) (current - 1 + total) % total else (current + 1) % total
    }

    /** The same stop in the new list (same key, local, output), else the newest. */
    fun keep(before: List<Stop>, current: Int?, after: List<Stop>): Int? {
        if (after.isEmpty()) return null
        val was = current?.let { before.getOrNull(it) }
        if (was != null) {
            val at = after.indexOfFirst { it.same(was) }
            if (at >= 0) return at
        }
        return after.size - 1
    }

    /** What landing on a stop does; [current] = the row's `highlight.current`. */
    data class Landing(val openGroup: String?, val pageIn: Boolean, val ring: Boolean, val current: Int?) {
        /** Always to the viewport's centre. */
        val scroll: String get() = "centre"
    }

    fun land(stop: Stop): Landing =
        if (!stop.loaded) {
            Landing(openGroup = null, pageIn = true, ring = true, current = null)
        } else {
            Landing(openGroup = stop.group, pageIn = false, ring = stop.output, current = if (stop.output) null else stop.local)
        }

    enum class Phase(val wire: String) { None("none"), Searching("searching"), Done("done"), Offline("offline") }

    /** The status line under the bar (null = none). */
    fun status(store: Phase, device: Phase, deviceName: String): String? = when {
        device == Phase.Offline -> Copy.DEVICE_OFFLINE.replace("{device}", deviceName)
        device == Phase.Searching -> Copy.SEARCHING_DEVICE.replace("{device}", deviceName)
        store == Phase.Searching -> Copy.SEARCHING_OLDER
        else -> null
    }

    enum class KeyAction(val wire: String) { Find("find"), Global("global"), Previous("previous"), Next("next"), Close("close") }

    /** A key press: [mod] = Cmd on Apple, else Ctrl (a hardware keyboard). */
    fun key(key: String, mod: Boolean, shift: Boolean, inChat: Boolean, inField: Boolean): KeyAction? {
        val k = if (key.length == 1) key.lowercase() else key
        if (mod && k == "k") return KeyAction.Global
        if (mod && k == "f") return if (inChat) KeyAction.Find else KeyAction.Global
        if (!inField || mod) return null
        if (k == "Enter") return if (shift) KeyAction.Next else KeyAction.Previous
        if (k == "Escape") return KeyAction.Close
        return null
    }

    /** The tool row's hint beside its title ([n] = its output's matches). */
    fun outputHint(n: Int): String = if (n == 1) Copy.OUTPUT_HINT_ONE else Copy.OUTPUT_HINT_MANY.replace("{n}", n.toString())

    /** The menu's shortcut hint (phones show none). */
    fun shortcut(apple: Boolean): String = if (apple) SHORTCUT_MAC else SHORTCUT_OTHER

    /** A hit's key: an event (seq = the tool row's for output) or a message. */
    fun eventKey(sessionId: String, seq: Long): String = "e:$sessionId:$seq"

    fun messageKey(messageId: String): String = "m:$messageId"

    /** The seq of an event key (`e:<session>:<seq>`), else null. */
    fun eventSeq(key: String): Long? =
        if (key.startsWith("e:")) key.substringAfterLast(':').substringBefore('#').toLongOrNull() else null
}
