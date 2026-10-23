package com.exponential.app.domain

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/**
 * Thread 58cb32df (owner 2026-10-23, #EXP-1373): CHATS, the phone app's
 * WhatsApp-like list, the agent bubbles, the context menus ×4, the
 * transcript's divider lines and the inline surface's reply. The Android
 * mirror of `packages/domain-contract/src/chats.ts` (iOS ExpCore
 * `Chats.swift`, desktop `domain::chats`), byte-locked by
 * `fixtures/chats.json`, which `ChatsTest` replays case group by case group;
 * the fixture's `$comment` holds the rules and [Copy] is its `copy`.
 */
object Chats {

    object Copy {
        const val TITLE = "Chats"

        /** `copy.tabs` by [Shell.TABS]. */
        val TABS: Map<String, String> = linkedMapOf(
            "board" to "Board",
            "devices" to "Devices",
            "chats" to "Chats",
            "team" to "Team",
        )

        /** `copy.filters` by [FILTERS]. */
        val FILTERS: Map<ChatFilter, String> = linkedMapOf(
            ChatFilter.All to "All",
            ChatFilter.NeedsYou to "Needs you",
            ChatFilter.Running to "Running",
            ChatFilter.Unread to "Unread",
        )

        const val PREVIEW_WORKING = "working…"
        const val PREVIEW_QUESTION = "Question: {text}"
        const val PREVIEW_PLAN = "Plan: {text}"
        const val PREVIEW_YOU = "You: {text}"
        const val PREVIEW_PICTURE = "Picture"
        const val PREVIEW_SURFACE = "Preview"
        const val PREVIEW_EMPTY = "No messages yet"

        const val TIME_YESTERDAY = "Yesterday"
        val WEEKDAYS_SHORT: List<String> = listOf("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun")
        val MONTHS_SHORT: List<String> =
            listOf("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")

        const val MENU_STOP = "Stop"
        const val MENU_STOP_AND_DELETE = "Stop and delete"
        const val MENU_DELETE = "Delete"
        const val MENU_COMPACT = "Compact conversation"
        const val MENU_NEW_CONVERSATION = "New conversation"
        const val MENU_SETTINGS = "Agent settings"

        const val CONFIRM_DELETE_TITLE = "Delete this chat?"
        const val CONFIRM_DELETE_BODY =
            "Its messages and sessions are deleted for everyone. Pull requests stay on GitHub."

        const val TOAST_STOP_FAILED = "Couldn't stop the chat"
        const val TOAST_DELETE_FAILED = "Couldn't delete the chat"
        const val TOAST_STOP_NOT_DELIVERED = "The device didn't get the stop"

        const val DIVIDER_NEW = "New conversation"
        const val DIVIDER_COMPACT = "Conversation compacted"

        const val SUPERSEDED = "Replaced by a newer plan"

        const val BOARD_CHIP_ALL = "All issues"
        const val BOARD_CHIP_MINE = "My issues"
        const val BOARD_CHIP_DRAFTS = "Drafts"

        const val TEAM_META = "{members} members · {boards} boards"
        const val TEAM_SWITCH = "Teams"

        const val INBOX = "Inbox"

        /** A divider kind's words (`copy.dividers`). */
        fun divider(kind: String): String = if (kind == DIVIDER_KIND_COMPACT) DIVIDER_COMPACT else DIVIDER_NEW

        /** `copy.team.meta` filled. */
        fun teamMeta(members: Int, boards: Int): String =
            TEAM_META.replace("{members}", members.toString()).replace("{boards}", boards.toString())
    }

    /** `shell`: the phone tab order and their icon CONCEPTS (the Team tab's
     *  icon is the active team's avatar), the sizes. */
    object Shell {
        val TABS: List<String> = listOf("board", "devices", "chats", "team")
        val TAB_ICONS: Map<String, String> = linkedMapOf(
            "board" to "nav-issues",
            "devices" to "nav-devices",
            "chats" to "nav-threads",
        )
        val BOARD_CHIPS: List<String> = listOf("all", "mine", "drafts")
        val CHATS_HEADER: List<String> = listOf("nav-search", "nav-inbox")
        const val LEAD_DISC = 52
        const val BUBBLE = 66
        const val BUBBLE_RING = 3
    }

    object TestIds {
        const val CHAT_ROW = "chat-row"
        const val CHAT_FILTER = "chat-filter"
        const val AGENT_BUBBLE = "agent-bubble"
        const val INBOX_BUTTON = "chats-inbox"
        const val TAB_TEAM = "tab-team"
        const val BOARD_CHIP = "board-chip"
        const val SURFACE_CARD = "surface-card"
        const val DIVIDER = "transcript-divider"
    }

    /** `agentMenu.sends`: what Compact / New conversation send (one path
     *  with the typed lines). */
    object Sends {
        const val COMPACT = "/compact"
        const val NEW_CONVERSATION = "/new"
    }

    /** `surfaceCard`: the reply event, the caps and the ENTER animation
     *  (durations / eases name the motion tokens). */
    object SurfaceCard {
        const val REPLY_EVENT = "reply"
        const val MAX_BYTES = 65536
        const val MAX_COMPONENTS = 200

        object Frame {
            const val FROM_OPACITY = 0f
            const val FROM_TRANSLATE_Y = 12f
            const val FROM_SCALE = 0.98f
            const val DURATION = "slow"
            const val EASE = "decelerate"
        }

        object Children {
            const val FROM_OPACITY = 0f
            const val FROM_TRANSLATE_Y = 6f
            const val DURATION = "standard"
            const val EASE = "decelerate"
            const val STAGGER_MS = 45
            const val MAX_STAGGERED = 8
            const val START_AFTER_MS = 90
        }

        object Glow {
            const val FROM_OPACITY = 0.35f
            const val DURATION = "slow"
            const val EASE = "decelerate"
        }

        /** The stagger delay of the root's [index]th first-level child:
         *  children past [Children.MAX_STAGGERED] share the last delay. */
        fun childDelayMs(index: Int): Int =
            Children.START_AFTER_MS + Children.STAGGER_MS * minOf(index, Children.MAX_STAGGERED - 1)
    }

    const val DIVIDER_KIND_NEW = "new"
    const val DIVIDER_KIND_COMPACT = "compact"

    enum class ChatFilter(val wire: String) { All("all"), NeedsYou("needs_you"), Running("running"), Unread("unread") }

    val FILTERS: List<ChatFilter> = ChatFilter.entries

    /** A row's lead (`lead`): needs_you | working | open | merged | waiting. */
    enum class ChatLead(val wire: String) {
        NeedsYou("needs_you"),
        Working("working"),
        Open("open"),
        Merged("merged"),
        Waiting("waiting"),
    }

    /** working | question | plan | you | agent | picture | surface | new | compact | empty. */
    data class ChatPreview(val kind: String, val text: String)

    data class ChatRow(
        val sessionId: String,
        val teamId: String,
        val title: String,
        val deviceId: String?,
        val group: SessionTree.ThreadsGroupKey,
        val lead: ChatLead,
        val preview: ChatPreview,
        val activityAt: String,
        val time: String,
        /** Agent messages of the ROOT conversation newer than the root's read. */
        val unreadCount: Int,
        val needsYou: Boolean,
        val busy: Boolean,
        val word: SessionTree.ThreadRowWord?,
    )

    data class ChatCounts(val needsYou: Int, val running: Int, val unread: Int) {
        operator fun get(filter: ChatFilter): Int = when (filter) {
            ChatFilter.All -> 0
            ChatFilter.NeedsYou -> needsYou
            ChatFilter.Running -> running
            ChatFilter.Unread -> unread
        }
    }

    data class ChatList(val rows: List<ChatRow>, val counts: ChatCounts)

    private fun ms(iso: String?): Long? = if (iso.isNullOrEmpty()) null else WireTimestamps.parseEpochMs(iso)

    private fun JsonObject.kind(): String? = (this["kind"] as? JsonPrimitive)?.takeIf { it.isString }?.content

    private fun JsonObject.textOf(key: String): String = when (val value = this[key]) {
        null, JsonNull -> ""
        is JsonPrimitive -> value.content
        else -> value.toString()
    }

    private val PREVIEW_CARDS = setOf(
        DomainContract.messageCardKindQuestion,
        DomainContract.messageCardKindPlan,
        DomainContract.messageCardKindAsk,
        DomainContract.messageCardKindPicture,
        DomainContract.messageCardKindSurface,
    )

    /** A builtin command line a person sent (`/new`, `/compact …`): the
     *  transcript paints it as a divider, never a bubble (`dividerCases`). */
    fun commandDivider(body: String): String? {
        val text = body.trim()
        if (text == "/new") return DIVIDER_KIND_NEW
        if (text == "/compact" || text.startsWith("/compact ")) return DIVIDER_KIND_COMPACT
        return null
    }

    private val IMAGE = Regex("""!\[[^\]]*\]\([^)]*\)""")
    private val LINK = Regex("""\[([^\]]*)\]\([^)]*\)""")
    private val LINE_PREFIX = Regex("""^\s*(?:#{1,6}\s+|>\s?|[-*+]\s+|\d+[.)]\s+)""")
    private val MARKS = Regex("""\*\*|__|~~|`""")
    private val OPEN_EMPHASIS = Regex("""(^|\s)[*_](\S)""")
    private val CLOSE_EMPHASIS = Regex("""(\S)[*_](?=\s|$)""")
    private val WHITESPACE = Regex("""\s+""")

    /** Markdown → one plain line (the list preview's text, `plainCases`). */
    fun plainLine(markdown: String): String = markdown
        .replace(IMAGE, " ")
        .replace(LINK, "\$1")
        .split("\n")
        .joinToString(" ") { it.replace(LINE_PREFIX, "") }
        .replace(MARKS, "")
        .replace(OPEN_EMPHASIS, "\$1\$2")
        .replace(CLOSE_EMPHASIS, "\$1")
        .replace(WHITESPACE, " ")
        .trim()

    /** The root conversation's preview line (`previewCases`). */
    fun chatPreview(rootId: String, messages: List<SessionTree.Message>, needsYou: Boolean, busy: Boolean): ChatPreview {
        val own = messages
            .filter {
                it.sessionId == rootId &&
                    (it.authorKind == DomainContract.messageAuthorKindUser || it.authorKind == DomainContract.messageAuthorKindAgent)
            }
            .filter { m -> m.card?.let { it.kind() in PREVIEW_CARDS } ?: m.body.isNotBlank() }
            .sortedWith(compareByDescending<SessionTree.Message> { ms(it.createdAt) ?: 0L }.thenByDescending { it.id })
        if (needsYou) {
            val waiting = own.firstOrNull { m ->
                m.answeredAt == null && m.card?.kind().let {
                    it == DomainContract.messageCardKindQuestion ||
                        it == DomainContract.messageCardKindPlan ||
                        it == DomainContract.messageCardKindAsk
                }
            }
            if (waiting != null) {
                val card = waiting.card!!
                return if (card.kind() == DomainContract.messageCardKindPlan) {
                    ChatPreview("plan", Copy.PREVIEW_PLAN.replace("{text}", plainLine(card.textOf("plan"))))
                } else {
                    ChatPreview("question", Copy.PREVIEW_QUESTION.replace("{text}", plainLine(card.textOf("question"))))
                }
            }
        }
        if (busy) return ChatPreview("working", Copy.PREVIEW_WORKING)
        val newest = own.firstOrNull() ?: return ChatPreview("empty", Copy.PREVIEW_EMPTY)
        if (newest.authorKind == DomainContract.messageAuthorKindUser) {
            val divider = commandDivider(newest.body)
            if (divider != null) return ChatPreview(divider, Copy.divider(divider))
            return ChatPreview("you", Copy.PREVIEW_YOU.replace("{text}", plainLine(newest.body)))
        }
        val card = newest.card
        return when (card?.kind()) {
            DomainContract.messageCardKindPicture -> ChatPreview("picture", Copy.PREVIEW_PICTURE)
            DomainContract.messageCardKindSurface -> {
                val title = (card["title"] as? JsonPrimitive)?.takeIf { it.isString }?.content?.trim().orEmpty()
                ChatPreview("surface", title.ifEmpty { Copy.PREVIEW_SURFACE })
            }
            DomainContract.messageCardKindQuestion, DomainContract.messageCardKindAsk ->
                ChatPreview("question", Copy.PREVIEW_QUESTION.replace("{text}", plainLine(card.textOf("question"))))
            DomainContract.messageCardKindPlan ->
                ChatPreview("plan", Copy.PREVIEW_PLAN.replace("{text}", plainLine(card.textOf("plan"))))
            else -> ChatPreview("agent", plainLine(newest.body))
        }
    }

    private fun pad2(n: Int) = n.toString().padStart(2, '0')

    /** The trailing time label (`timeCases`): the viewer's local day by
     *  [utcOffsetMinutes]. */
    fun chatTimeLabel(at: String, now: String, utcOffsetMinutes: Int): String {
        val shift = utcOffsetMinutes * 60_000L
        val local = (ms(at) ?: 0L) + shift
        val today = (ms(now) ?: 0L) + shift
        val day = Math.floorDiv(local, 86_400_000L)
        val diff = Math.floorDiv(today, 86_400_000L) - day
        if (diff <= 0) {
            val minuteOfDay = Math.floorMod(local, 86_400_000L) / 60_000L
            return "${pad2((minuteOfDay / 60).toInt())}:${pad2((minuteOfDay % 60).toInt())}"
        }
        if (diff == 1L) return Copy.TIME_YESTERDAY
        // 1970-01-01 was a Thursday (Monday-first index 3).
        if (diff < 7) return Copy.WEEKDAYS_SHORT[Math.floorMod(day + 3, 7L).toInt()]
        val date = java.time.LocalDate.ofEpochDay(day)
        val todayYear = java.time.LocalDate.ofEpochDay(Math.floorDiv(today, 86_400_000L)).year
        val label = "${Copy.MONTHS_SHORT[date.monthValue - 1]} ${date.dayOfMonth}"
        return if (date.year == todayYear) label else "$label, ${date.year}"
    }

    /** The device's current offset from UTC in minutes (the clients' input). */
    fun deviceUtcOffsetMinutes(nowMs: Long = System.currentTimeMillis()): Int =
        java.util.TimeZone.getDefault().getOffset(nowMs) / 60_000

    private fun unreadCount(rootId: String, messages: List<SessionTree.Message>, reads: List<SessionTree.SessionRead>): Int {
        val read = ms(reads.firstOrNull { it.sessionId == rootId }?.readAt) ?: -1L
        return messages.count {
            it.sessionId == rootId &&
                it.authorKind == DomainContract.messageAuthorKindAgent &&
                it.card?.kind() != DomainContract.messageCardKindEvent &&
                (ms(it.createdAt) ?: 0L) > read
        }
    }

    private fun keep(row: ChatRow, filter: ChatFilter): Boolean = when (filter) {
        ChatFilter.All -> true
        ChatFilter.NeedsYou -> row.group == SessionTree.ThreadsGroupKey.NeedsYou
        ChatFilter.Running -> row.group == SessionTree.ThreadsGroupKey.Running
        ChatFilter.Unread -> row.unreadCount > 0
    }

    /**
     * The phone Chats list (`listCases`): the ROOT rows of
     * [SessionTree.partitionThreads] (active team) flattened and sorted by
     * activity desc (ties by id), each with its lead, preview, time and unread
     * count, then filtered by the chip.
     */
    fun chatList(
        groups: List<SessionTree.ThreadsListGroup>,
        messages: List<SessionTree.Message>,
        reads: List<SessionTree.SessionRead> = emptyList(),
        /** The ROOT's PR states by session id (absent = none). */
        prStates: Map<String, List<String>> = emptyMap(),
        /** threadRowWord by session id (absent = none). */
        words: Map<String, SessionTree.ThreadRowWord?> = emptyMap(),
        filter: ChatFilter,
        now: String,
        utcOffsetMinutes: Int,
    ): ChatList {
        val all = groups
            .flatMap { group -> group.rows.filter { it.depth == 0 }.map { it to group.key } }
            .map { (row, group) ->
                val mark = SessionTree.threadRowMarkState(row.needsYou, row.busy)
                val lead = when (mark) {
                    SessionTree.SessionState.NeedsYou -> ChatLead.NeedsYou
                    SessionTree.SessionState.Working -> ChatLead.Working
                    else -> {
                        val states = prStates[row.sessionId].orEmpty()
                        when {
                            states.any { it == DomainContract.prStateOpen } -> ChatLead.Open
                            states.any { it == DomainContract.prStateMerged } -> ChatLead.Merged
                            else -> ChatLead.Waiting
                        }
                    }
                }
                ChatRow(
                    sessionId = row.sessionId,
                    teamId = row.teamId,
                    title = row.title,
                    deviceId = row.deviceId,
                    group = group,
                    lead = lead,
                    preview = chatPreview(row.sessionId, messages, row.needsYou, row.busy),
                    activityAt = row.activityAt,
                    time = chatTimeLabel(row.activityAt, now, utcOffsetMinutes),
                    unreadCount = unreadCount(row.sessionId, messages, reads),
                    needsYou = row.needsYou,
                    busy = row.busy,
                    word = words[row.sessionId],
                )
            }
            .sortedWith(compareByDescending<ChatRow> { ms(it.activityAt) ?: 0L }.thenBy { it.sessionId })
        return ChatList(
            rows = all.filter { keep(it, filter) },
            counts = ChatCounts(
                needsYou = all.count { keep(it, ChatFilter.NeedsYou) },
                running = all.count { keep(it, ChatFilter.Running) },
                unread = all.count { keep(it, ChatFilter.Unread) },
            ),
        )
    }

    /** A chip's label: its copy, then its count when there is one. */
    fun chatFilterLabel(filter: ChatFilter, counts: ChatCounts): String {
        val label = Copy.FILTERS.getValue(filter)
        if (filter == ChatFilter.All) return label
        val n = counts[filter]
        return if (n > 0) "$label $n" else label
    }

    /** needs_you (amber) | unread (foreground) | none (a hairline). */
    enum class AgentRing(val wire: String) { NeedsYou("needs_you"), Unread("unread") }

    data class AgentBubble(val agent: SessionTree.AgentRow, val ring: AgentRing?)

    /** The Chats header's agent bubbles (`bubbleCases`), in
     *  [SessionTree.listAgents] order. */
    fun agentBubbles(
        agents: List<SessionTree.AgentRow>,
        sessions: List<SessionTree.Session>,
        messages: List<SessionTree.Message>,
        reads: List<SessionTree.SessionRead> = emptyList(),
        me: String?,
    ): List<AgentBubble> = agents.map { agent ->
        if (agent.needsYou) return@map AgentBubble(agent, AgentRing.NeedsYou)
        val conversation = sessions.firstOrNull { it.parentId == null && it.agentId == agent.agentId && it.userId == me }
        val unread = conversation != null && unreadCount(conversation.id, messages, reads) > 0
        AgentBubble(agent, if (unread) AgentRing.Unread else null)
    }

    enum class ThreadMenuEntry(val wire: String) { Stop("stop"), StopAndDelete("stopAndDelete"), Delete("delete") }

    /** A chat row's context menu ×4 (`menuCases`): live → Stop, Stop and
     *  delete; ended → Delete; nothing for a viewer who cannot steer it. */
    fun threadMenuEntries(rootStatus: String, canSteer: Boolean): List<ThreadMenuEntry> {
        if (!canSteer) return emptyList()
        return if (rootStatus == DomainContract.sessionStatusEnded) {
            listOf(ThreadMenuEntry.Delete)
        } else {
            listOf(ThreadMenuEntry.Stop, ThreadMenuEntry.StopAndDelete)
        }
    }

    enum class AgentMenuEntry(val wire: String) {
        Compact("compact"),
        NewConversation("newConversation"),
        Stop("stop"),
        Settings("settings"),
    }

    /** The agent chat's header menu (and its bubble's long-press) ×4
     *  (`agentMenuCases`). */
    fun agentMenuEntries(busy: Boolean, isOwner: Boolean): List<AgentMenuEntry> = buildList {
        add(AgentMenuEntry.Compact)
        add(AgentMenuEntry.NewConversation)
        if (busy) add(AgentMenuEntry.Stop)
        if (isOwner) add(AgentMenuEntry.Settings)
    }

    /** What a menu entry sends (`agentMenu.sends`); null = not a send. */
    fun agentMenuSend(entry: AgentMenuEntry): String? = when (entry) {
        AgentMenuEntry.Compact -> Sends.COMPACT
        AgentMenuEntry.NewConversation -> Sends.NEW_CONVERSATION
        else -> null
    }

    fun threadMenuLabel(entry: ThreadMenuEntry): String = when (entry) {
        ThreadMenuEntry.Stop -> Copy.MENU_STOP
        ThreadMenuEntry.StopAndDelete -> Copy.MENU_STOP_AND_DELETE
        ThreadMenuEntry.Delete -> Copy.MENU_DELETE
    }

    fun agentMenuLabel(entry: AgentMenuEntry): String = when (entry) {
        AgentMenuEntry.Compact -> Copy.MENU_COMPACT
        AgentMenuEntry.NewConversation -> Copy.MENU_NEW_CONVERSATION
        AgentMenuEntry.Stop -> Copy.MENU_STOP
        AgentMenuEntry.Settings -> Copy.MENU_SETTINGS
    }

    /** A press inside an inline preview (`replyCases`): an event named
     *  [SurfaceCard.REPLY_EVENT] with a non-empty `context.text` posts that
     *  text as the person's message; anything else does nothing. */
    fun surfaceReply(name: JsonElement?, context: JsonElement?): String? {
        val eventName = (name as? JsonPrimitive)?.takeIf { it.isString }?.content
        if (eventName != SurfaceCard.REPLY_EVENT) return null
        val text = ((context as? JsonObject)?.get("text") as? JsonPrimitive)?.takeIf { it.isString }?.content?.trim()
        return text?.takeIf { it.isNotEmpty() }
    }

    /** [surfaceReply] for a renderer's (name, context) pair. */
    fun surfaceReply(name: String?, context: JsonObject?): String? =
        surfaceReply(name?.let(::JsonPrimitive), context)
}
