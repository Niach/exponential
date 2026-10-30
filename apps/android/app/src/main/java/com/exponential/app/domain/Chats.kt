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
            "chats" to "Chats",
            "apps" to "Apps",
            "team" to "Team",
        )

        /** `copy.headers` by page (the Board / Team titles are the board's /
         *  team's names). */
        val HEADERS: Map<String, String> = linkedMapOf(
            "chats" to "Chats",
            "apps" to "Apps",
            "devices" to "Devices",
            "agent" to "Agent",
        )

        const val BOARD_MENU_NEW_BOARD = "New board"

        /** `copy.filters` by [FILTERS]. */
        val FILTERS: Map<ChatFilter, String> = linkedMapOf(
            ChatFilter.All to "All",
            ChatFilter.NeedsYou to "Needs you",
            ChatFilter.Running to "Working",
            ChatFilter.Unread to "Unread",
        )

        const val PREVIEW_WORKING = "working…"
        const val PREVIEW_QUESTION = "Question: {text}"
        const val PREVIEW_PLAN = "Plan: {text}"
        const val PREVIEW_YOU = "You: {text}"
        const val PREVIEW_PICTURE = "Picture"
        const val PREVIEW_SURFACE = "Preview"
        const val PREVIEW_EMPTY = "No messages yet"
        /** #EXP-1399: the bus's `askText` leads (`copy.preview.askQuestionLead` / `askPlanLead`). */
        const val PREVIEW_ASK_QUESTION_LEAD = "Asks: "
        const val PREVIEW_ASK_PLAN_LEAD = "Proposes a plan:"
        /** #EXP-1399 review: the server's NEUTRAL mirror (a reader may not see
         *  the origin) and the private preview lines. */
        const val PREVIEW_ASK_PRIVATE_QUESTION_LEAD = "Asks a question in a private chat."
        const val PREVIEW_ASK_PRIVATE_PLAN_LEAD = "Proposes a plan in a private chat."
        const val PREVIEW_QUESTION_PRIVATE = "Question in a private chat"
        const val PREVIEW_PLAN_PRIVATE = "Plan in a private chat"
        /** #EXP-1408: the bus's NEUTRAL lines for a private chat. */
        const val PREVIEW_PRIVATE_REPLY_LEAD = "A private chat replied."
        const val PREVIEW_PRIVATE_REPORT_LEAD = "A private chat finished."
        const val PREVIEW_PRIVATE_PR_LEAD = "A private chat opened a pull request."

        const val TIME_YESTERDAY = "Yesterday"
        val WEEKDAYS_SHORT: List<String> = listOf("Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun")
        val MONTHS_SHORT: List<String> =
            listOf("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")

        /** S-3: the composer stop-turn's toasts (`copy.stopTurn`). */
        const val STOP_TURN_FAILED = "Couldn't stop the turn"
        const val STOP_TURN_NOT_DELIVERED = "The device didn't get the stop"

        const val MENU_DELETE = "Delete"
        const val MENU_COMPACT = "Compact conversation"
        const val MENU_NEW_CONVERSATION = "New conversation"
        const val MENU_SETTINGS = "Agent settings"

        const val CONFIRM_DELETE_TITLE = "Delete this chat?"
        const val CONFIRM_DELETE_BODY =
            "Its messages and sessions are deleted for everyone. Pull requests stay on GitHub."

        const val TOAST_DELETE_FAILED = "Couldn't delete the chat"
        const val TOAST_SELECTION_FAILED = "Couldn't update every chat"
        const val TOAST_SELECTION_PARTIAL = "{n} of {m} chats couldn't be updated"

        /** #EXP-1388 `copy.selection`. */
        const val SELECTION_SELECT = "Select"
        const val SELECTION_COUNT = "{n}"
        const val SELECTION_MARK_READ = "Mark read"
        const val SELECTION_MARK_UNREAD = "Mark unread"
        const val SELECTION_ARCHIVE = "Archive"
        const val SELECTION_UNARCHIVE = "Unarchive"
        const val SELECTION_DELETE = "Delete"
        const val SELECTION_CONFIRM_DELETE_MANY = "Delete {n} chats?"

        /** #EXP-1388 `copy.archived`. */
        const val ARCHIVED_ENTRY = "Archived"
        const val ARCHIVED_TITLE = "Archived"
        const val ARCHIVED_EMPTY = "No archived chats"

        /** S-3 `copy.sections`: the Chats list's headers, only while an
         *  agent row exists. */
        const val SECTION_AGENTS = "Agents"
        const val SECTION_CHATS = "Chats"

        /** `copy.selection.count` filled. */
        fun selectionCount(n: Int): String = SELECTION_COUNT.replace("{n}", n.toString())

        const val DIVIDER_NEW = "New conversation"
        const val DIVIDER_COMPACT = "Conversation compacted"

        const val SUPERSEDED = "Replaced by a newer plan"

        const val BOARD_CHIP_ALL = "All issues"
        const val BOARD_CHIP_MINE = "My issues"
        const val BOARD_CHIP_DRAFTS = "Drafts"

        const val TEAM_META = "{members} members · {boards} boards"
        const val TEAM_SWITCH = "Teams"

        const val INBOX = "Inbox"

        /** #EXP-1380: the transcript's window button and its status line. */
        const val LOAD_EARLIER = "Load earlier"
        const val WORKING = "Working…"

        /** A divider kind's words (`copy.dividers`). */
        fun divider(kind: String): String = if (kind == DIVIDER_KIND_COMPACT) DIVIDER_COMPACT else DIVIDER_NEW

        /** `copy.team.meta` filled. */
        fun teamMeta(members: Int, boards: Int): String =
            TEAM_META.replace("{members}", members.toString()).replace("{boards}", boards.toString())
    }

    /** `shell`: the phone tab order and their icon CONCEPTS (the Team tab's
     *  icon is the active team's avatar), the bar, the board tab and menu,
     *  the page header, the sizes. */
    object Shell {
        val TABS: List<String> = listOf("board", "chats", "apps", "team")
        val TAB_ICONS: Map<String, String> = linkedMapOf(
            "board" to "nav-issues",
            "chats" to "nav-threads",
            "apps" to "nav-apps",
        )

        /** `shell.bar`: the tab capsule (dp). */
        object Bar {
            const val HEIGHT = 52
            const val PADDING = 4
            const val GAP = 12
            const val TAB_MIN_WIDTH = 44
            const val TAB_PADDING_X = 8

            /** `shell.bar.drag`: the selection lens. */
            object Drag {
                const val LENS_SCALE = 1.12f
                const val CANCEL_DISTANCE = 56
            }

            /**
             * The tabs' widths in a capsule [available] wide, given each tab's
             * [preferred] width (its label + 2 × [TAB_PADDING_X], at least
             * [TAB_MIN_WIDTH]): while they fit, every tab gets its own width
             * plus an even share of the rest; when they do not, the widest
             * give way first (one cap for all the tabs above it), so a label
             * truncates only when the labels alone do not fit.
             */
            fun tabWidths(preferred: List<Float>, available: Float): List<Float> {
                if (preferred.isEmpty()) return emptyList()
                val total = preferred.sum()
                if (total <= available) {
                    val extra = (available - total) / preferred.size
                    return preferred.map { it + extra }
                }
                var remaining = available
                var cap = Float.MAX_VALUE
                val ascending = preferred.sorted()
                for ((index, width) in ascending.withIndex()) {
                    val left = ascending.size - index
                    if (width * left <= remaining) {
                        remaining -= width
                    } else {
                        cap = remaining / left
                        break
                    }
                }
                return preferred.map { minOf(it, cap) }
            }
        }

        /** `shell.boardTab`: the caret on the board view. */
        object BoardTab {
            const val CARET = "nav-team-switcher"
            const val CARET_SIZE = 10
        }

        /** `shell.boardMenu`: the strip morph (durations / eases name the
         *  motion tokens). */
        object BoardMenu {
            const val ROW_HEIGHT = 48
            const val MAX_VISIBLE_ROWS = 8
            const val RADIUS = 28
            const val OPEN = "slow"
            const val OPEN_EASE = "decelerate"
            const val CLOSE = "standard"
            const val CLOSE_EASE = "accelerate"
        }

        /** `shell.header`: the page header's pages, its trailing circles in
         *  order, the circle size (dp). */
        object Header {
            val PAGES: List<String> = listOf("board", "chats", "apps", "team", "devices", "agent")
            val BUTTONS: List<String> = listOf("nav-search", "nav-inbox")
            const val CIRCLE = 44
        }

        val BOARD_CHIPS: List<String> = listOf("all", "mine", "drafts")
        const val LEAD_DISC = 52

        /** #EXP-1388 the touch SELECT BADGE on a selected row's lead disc:
         *  THE checkbox square (primary, [SELECT_BADGE_RADIUS] corner), its
         *  ring (the row's ground; outer corner = radius + ring) and the
         *  ui-check glyph inside (dp). */
        const val SELECT_BADGE = 18
        const val SELECT_BADGE_RING = 2
        const val SELECT_BADGE_GLYPH = 14
        const val SELECT_BADGE_RADIUS = 4

        /** The Archived entry's plain ui-archive glyph (dp). */
        const val ARCHIVED_GLYPH = 24
        const val BUBBLE = 66
        const val BUBBLE_RING = 3
        /** S-3 `shell.dimOpacity`: a done chat row (whole row). */
        const val DIM_OPACITY = 0.5f
        /** S-3 `shell.chatLead`: a chat row's lead glyph CONCEPT (never the
         *  brand mark). */
        const val CHAT_LEAD = "action-chat"
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
        const val TAB_BOARD = "tab-board"
        const val BOARD_MENU = "board-menu"
        const val BOARD_MENU_ROW = "board-menu-row"
        const val PAGE_HEADER = "page-header"
        const val SEARCH_BUTTON = "header-search"
        const val LOAD_EARLIER = "transcript-load-earlier"
        const val JUMP_TO_BOTTOM = "jump-to-bottom"
        const val CHAT_ARCHIVED = "chat-archived"
        const val SELECTION_BAR = "bulk-action-bar"
    }

    /** #EXP-1380 `transcript`: a chat paints its newest [WINDOW] rows, `Load
     *  earlier` widens by [WINDOW_STEP]; the tail is followed within
     *  [FOLLOW_SLACK] px of the bottom. */
    object Transcript {
        const val WINDOW = 100
        const val WINDOW_STEP = 100
        const val FOLLOW_SLACK = 32
    }

    /** chats.json WINDOW (`windowCases`, review of #1218; TS
     *  `transcriptWindowStart`): the index of the window's first painted row.
     *  The newest [Transcript.WINDOW] rows, the start reaching up to the
     *  oldest WAITING plan / question card ([waitingIndexes]: never cut off);
     *  [pinnedStart] = the start the FIRST paint returned (null = not painted
     *  yet / reopened): arrivals append below it, a shrink only grows the
     *  window; each Load earlier press ([loadedSteps]) moves the start up by
     *  [Transcript.WINDOW_STEP]. `start > 0` shows Load earlier. */
    fun transcriptWindowStart(
        rowCount: Int,
        waitingIndexes: List<Int>,
        pinnedStart: Int?,
        loadedSteps: Int,
    ): Int {
        val tail = maxOf(0, rowCount - Transcript.WINDOW)
        var base = if (pinnedStart == null) tail else minOf(maxOf(0, pinnedStart), tail)
        for (index in waitingIndexes) if (index in 0 until rowCount && index < base) base = index
        return maxOf(0, base - maxOf(0, loadedSteps) * Transcript.WINDOW_STEP)
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
        /** S-3: the chat is DONE (every PR of the root merged and nothing
         *  works or needs you): it stays, dimmed ([Shell.DIM_OPACITY]). */
        val dim: Boolean = false,
    )

    data class ChatCounts(val needsYou: Int, val running: Int, val unread: Int) {
        operator fun get(filter: ChatFilter): Int = when (filter) {
            ChatFilter.All -> 0
            ChatFilter.NeedsYou -> needsYou
            ChatFilter.Running -> running
            ChatFilter.Unread -> unread
        }
    }

    /** [rows] / [counts] of the requested view; [archived] = the team's
     *  archived chats either way (#EXP-1388, the Archived entry). */
    data class ChatList(val rows: List<ChatRow>, val counts: ChatCounts, val archived: Int = 0)

    /** #EXP-1388: the list's view: the chats, or the Archived page. */
    enum class ChatListView(val wire: String) { List("list"), Archived("archived") }

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

    /** #EXP-1383 (`agentHint.banner`): the agent chat's "Instructions
     *  changed" Dock banner row; its action sends [AGENT_HINT_SENDS]. */
    object AgentHint {
        const val ID = "instructions"
        const val ICON = "ui-info"
        const val LABEL = "Instructions changed"
        const val TEXT = "This conversation still follows the previous instructions"
        const val TONE = "muted"
        const val ACTION = "Start new conversation"
        const val SENDS = Sends.NEW_CONVERSATION
        /** The Dock row id the banner draws as. */
        const val ROW_ID = "banner:$ID"

        val banner: Dock.Source.Banner
            get() = Dock.Source.Banner(id = ID, icon = ICON, label = LABEL, text = TEXT, tone = Dock.Tint.Muted, action = ACTION)
    }

    /**
     * #EXP-1383 (`agentHint.cases`): the agent chat's root fetched its brief
     * ([sessionStamp] = `sessions.agent_instructions_at`) before the agent's
     * instructions last changed ([agentStamp] =
     * `agents.instructions_updated_at`). Either stamp missing = no hint;
     * equal = current.
     */
    fun agentInstructionsChanged(sessionStamp: String?, agentStamp: String?): Boolean {
        val session = ms(sessionStamp) ?: return false
        val agent = ms(agentStamp) ?: return false
        return session < agent
    }

    // ── the list's per-conversation DIGEST (`digestCases`, TS `digestMessages`) ──
    // The Chats list never reads a transcript: the server keeps, on each
    // conversation row (`conversations.digest`), exactly the messages the
    // list's rules read; [digestMessages] turns it back into tree messages so
    // `partitionThreads`, `chatList` and `agentBubbles` run UNCHANGED over it.

    /** `conversations.digest` (JSON text) → its object; null / bad = none. */
    fun parseDigest(text: String?): JsonObject? {
        if (text.isNullOrBlank()) return null
        return runCatching { kotlinx.serialization.json.Json.parseToJsonElement(text) as? JsonObject }.getOrNull()
    }

    /** A digest back as the tree messages the list reads: its first /
     *  preview / waiting messages (once each), one bodiless agent message
     *  per kept agent time (the one a kept message already stands for
     *  skipped, matched as INSTANTS), and the newest agent time as an event
     *  card. Output times are the digest's strings verbatim. */
    fun digestMessages(sessionId: String, digest: JsonObject?): List<SessionTree.Message> {
        if (digest == null) return emptyList()
        val out = mutableListOf<SessionTree.Message>()
        val seen = mutableSetOf<String>()
        // Kept agent messages by instant (a multiset: two may share a time).
        val counted = mutableMapOf<Long, Int>()
        for (key in listOf("first", "preview", "waiting")) {
            val m = digest[key] as? JsonObject ?: continue
            val id = m.textOf("id")
            if (!seen.add(id)) continue
            val authorKind = m.textOf("authorKind")
            val card = m["card"] as? JsonObject
            val createdAt = m.textOf("createdAt")
            out += SessionTree.Message(
                id = id,
                sessionId = sessionId,
                authorKind = authorKind,
                authorId = null,
                body = m.textOf("body"),
                card = card,
                answeredAt = m.optText("answeredAt"),
                consumedAt = null,
                createdAt = createdAt,
                authorSessionId = m.optText("authorSessionId"),
                // EXP-1459: a slot the DAO could not open (SealedContent.conversation).
                locked = m.optText("locked"),
            )
            if (authorKind == "agent" && card?.kind() != "event") {
                val at = ms(createdAt) ?: 0L
                counted[at] = (counted[at] ?: 0) + 1
            }
        }
        val times = (digest["agentTimes"] as? kotlinx.serialization.json.JsonArray).orEmpty()
        times.forEachIndexed { index, element ->
            val at = (element as? JsonPrimitive)?.takeIf { it.isString }?.content ?: return@forEachIndexed
            val instant = ms(at) ?: 0L
            val kept = counted[instant] ?: 0
            if (kept > 0) {
                counted[instant] = kept - 1
                return@forEachIndexed
            }
            out += SessionTree.Message(
                id = "digest:$sessionId:$index",
                sessionId = sessionId,
                authorKind = "agent",
                authorId = null,
                body = "",
                card = null,
                answeredAt = null,
                consumedAt = null,
                createdAt = at,
                authorSessionId = null,
            )
        }
        val agentAt = digest.optText("agentAt")
        if (!agentAt.isNullOrEmpty()) {
            out += SessionTree.Message(
                id = "digest:$sessionId:agent",
                sessionId = sessionId,
                authorKind = "agent",
                authorId = null,
                body = "",
                card = JsonObject(mapOf("kind" to JsonPrimitive("event"))),
                answeredAt = null,
                consumedAt = null,
                createdAt = agentAt,
                authorSessionId = null,
            )
        }
        return out
    }

    private fun JsonObject.optText(key: String): String? =
        (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

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

    private val BLANK_LINE = Regex("""\n\s*\n""")

    /** A plan / question card's preview (kind + filled copy). #EXP-1399: an
     *  `ask` mirror reads its body, the bus's `askText` (`Asks: <question>`
     *  on its first line with text | `Proposes a plan:` then the plan's first
     *  block), which the list digest keeps too. A PRIVATE origin (the server's
     *  neutral body, or a session outside [visible]) shows the private copy,
     *  never its text. */
    private fun askPreview(message: SessionTree.Message, visible: Set<String>?): ChatPreview {
        val card = message.card!!
        fun question(text: String) = ChatPreview("question", Copy.PREVIEW_QUESTION.replace("{text}", text))
        fun plan(text: String) = ChatPreview("plan", Copy.PREVIEW_PLAN.replace("{text}", text))
        if (card.kind() == DomainContract.messageCardKindPlan) return plan(plainLine(card.textOf("plan")))
        if (card.kind() != DomainContract.messageCardKindAsk) return question(plainLine(card.textOf("question")))
        val body = message.body.trim()
        val isPlan = body.startsWith(Copy.PREVIEW_ASK_PLAN_LEAD) || body.startsWith(Copy.PREVIEW_ASK_PRIVATE_PLAN_LEAD)
        val origin = (card["sessionId"] as? JsonPrimitive)?.takeIf { it.isString }?.content ?: message.authorSessionId ?: ""
        val hidden = body.startsWith(Copy.PREVIEW_ASK_PRIVATE_PLAN_LEAD) ||
            body.startsWith(Copy.PREVIEW_ASK_PRIVATE_QUESTION_LEAD) ||
            (visible != null && origin !in visible)
        if (hidden) {
            return if (isPlan) ChatPreview("plan", Copy.PREVIEW_PLAN_PRIVATE) else ChatPreview("question", Copy.PREVIEW_QUESTION_PRIVATE)
        }
        if (body.startsWith(Copy.PREVIEW_ASK_PLAN_LEAD)) {
            return plan(plainLine(body.substring(Copy.PREVIEW_ASK_PLAN_LEAD.length).trim().split(BLANK_LINE).first()))
        }
        val first = body.split("\n").firstOrNull { plainLine(it) != "" } ?: ""
        return question(plainLine(first.removePrefix(Copy.PREVIEW_ASK_QUESTION_LEAD)))
    }

    /** EXP-1459: a sealed message that does not open here (sealed-content.json `placeholder`). */
    private fun lockedPreview(reason: String) = ChatPreview(PREVIEW_KIND_LOCKED, SealedPlaceholder.copy(reason))

    /** EXP-1459: the preview kind of a locked message, painted italic muted like `working`. */
    const val PREVIEW_KIND_LOCKED = "locked"

    /** The root conversation's preview line (`previewCases`). */
    /** [visible] = the viewer's synced session ids (null = no check, #EXP-1399 review). */
    fun chatPreview(
        rootId: String,
        messages: List<SessionTree.Message>,
        needsYou: Boolean,
        busy: Boolean,
        visible: Set<String>? = null,
    ): ChatPreview {
        val own = messages
            .filter {
                it.sessionId == rootId &&
                    (it.authorKind == DomainContract.messageAuthorKindUser || it.authorKind == DomainContract.messageAuthorKindAgent)
            }
            .filter { m -> m.locked != null || (m.card?.let { it.kind() in PREVIEW_CARDS } ?: m.body.isNotBlank()) }
            .sortedWith(compareByDescending<SessionTree.Message> { ms(it.createdAt) ?: 0L }.thenByDescending { it.id })
        if (needsYou) {
            val waiting = own.firstOrNull { m ->
                m.answeredAt == null && m.card?.kind().let {
                    it == DomainContract.messageCardKindQuestion ||
                        it == DomainContract.messageCardKindPlan ||
                        it == DomainContract.messageCardKindAsk
                }
            }
            if (waiting != null) return waiting.locked?.let(::lockedPreview) ?: askPreview(waiting, visible)
        }
        if (busy) return ChatPreview("working", Copy.PREVIEW_WORKING)
        val newest = own.firstOrNull() ?: return ChatPreview("empty", Copy.PREVIEW_EMPTY)
        newest.locked?.let { return lockedPreview(it) }
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
            DomainContract.messageCardKindQuestion, DomainContract.messageCardKindAsk, DomainContract.messageCardKindPlan ->
                askPreview(newest, visible)
            // #EXP-1408: the bus's neutral relay of a private chat: its lead only.
            else ->
                if (newest.body.startsWith(Copy.PREVIEW_PRIVATE_REPLY_LEAD)) ChatPreview("agent", Copy.PREVIEW_PRIVATE_REPLY_LEAD)
                else ChatPreview("agent", plainLine(newest.body))
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
        /** #EXP-1388: the chats, or the Archived page. */
        view: ChatListView = ChatListView.List,
        now: String,
        utcOffsetMinutes: Int,
        /** #EXP-1399 review: the viewer's synced session ids ([chatPreview]). */
        visible: Set<String>? = null,
    ): ChatList {
        val archivedIds = reads.filter { !it.archivedAt.isNullOrEmpty() }.map { it.sessionId }.toSet()
        fun isArchived(row: ChatRow) = !row.needsYou && row.sessionId in archivedIds
        val wantArchived = view == ChatListView.Archived
        val rows = groups
            .flatMap { group -> group.rows.filter { it.depth == 0 }.map { it to group.key } }
            .map { (row, group) ->
                val mark = SessionTree.threadRowMarkState(row.needsYou, row.busy)
                val states = prStates[row.sessionId].orEmpty()
                val lead = when (mark) {
                    SessionTree.SessionState.NeedsYou -> ChatLead.NeedsYou
                    SessionTree.SessionState.Working -> ChatLead.Working
                    else -> {
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
                    preview = chatPreview(row.sessionId, messages, row.needsYou, row.busy, visible),
                    activityAt = row.activityAt,
                    time = chatTimeLabel(row.activityAt, now, utcOffsetMinutes),
                    unreadCount = unreadCount(row.sessionId, messages, reads),
                    needsYou = row.needsYou,
                    busy = row.busy,
                    word = words[row.sessionId],
                    dim = mark == SessionTree.SessionState.Idle &&
                        states.isNotEmpty() && states.all { it == DomainContract.prStateMerged },
                )
            }
            .sortedWith(compareByDescending<ChatRow> { ms(it.activityAt) ?: 0L }.thenBy { it.sessionId })
        val all = rows.filter { isArchived(it) == wantArchived }
        return ChatList(
            rows = all.filter { keep(it, filter) },
            counts = ChatCounts(
                needsYou = all.count { keep(it, ChatFilter.NeedsYou) },
                running = all.count { keep(it, ChatFilter.Running) },
                unread = all.count { keep(it, ChatFilter.Unread) },
            ),
            archived = rows.count(::isArchived),
        )
    }

    /** #EXP-1388 (`dotCases`): the Chats nav dot = an unread depth-0 chat
     *  outside the archive (a chat that needs you counts anyway). */
    fun chatsUnread(groups: List<SessionTree.ThreadsListGroup>, reads: List<SessionTree.SessionRead> = emptyList()): Boolean {
        val archived = reads.filter { !it.archivedAt.isNullOrEmpty() }.map { it.sessionId }.toSet()
        return groups.any { group ->
            group.rows.any { it.depth == 0 && it.unread && (it.needsYou || it.sessionId !in archived) }
        }
    }

    enum class ChatSelectionAction(val wire: String) {
        MarkRead("markRead"),
        MarkUnread("markUnread"),
        Archive("archive"),
        Unarchive("unarchive"),
        Delete("delete"),
    }

    data class ChatSelectionRow(
        val sessionId: String,
        val rootStatus: String,
        val unreadCount: Int,
        val canSteer: Boolean,
        val canDelete: Boolean,
    )

    data class ChatSelectionEntry(val action: ChatSelectionAction, val sessionIds: List<String>)

    /** #EXP-1388 (`selectionCases`): the selection bar's actions in bar
     *  order, each with the chats it acts on (selection order): Mark read
     *  when any is unread, else Mark unread (all); Archive in the list,
     *  Unarchive on the Archived page (all); Delete only when every one may
     *  be deleted (all). S-3: no Stop. */
    fun chatSelectionActions(rows: List<ChatSelectionRow>, view: ChatListView = ChatListView.List): List<ChatSelectionEntry> {
        if (rows.isEmpty()) return emptyList()
        val ids = rows.map { it.sessionId }
        val out = mutableListOf(
            ChatSelectionEntry(
                if (rows.any { it.unreadCount > 0 }) ChatSelectionAction.MarkRead else ChatSelectionAction.MarkUnread,
                ids,
            ),
            ChatSelectionEntry(
                if (view == ChatListView.Archived) ChatSelectionAction.Unarchive else ChatSelectionAction.Archive,
                ids,
            ),
        )
        if (rows.all { it.canSteer && it.canDelete }) out += ChatSelectionEntry(ChatSelectionAction.Delete, ids)
        return out
    }

    /** #EXP-1388 (`rangeCases`): a pointer's range select: the ids between
     *  [anchor] and [target] in list order, both included; no anchor = the
     *  target alone. */
    fun chatSelectionRange(order: List<String>, anchor: String?, target: String): List<String> {
        val to = order.indexOf(target)
        if (to < 0) return emptyList()
        val from = if (anchor == null) -1 else order.indexOf(anchor)
        if (from < 0) return listOf(target)
        return order.subList(minOf(from, to), maxOf(from, to) + 1).toList()
    }

    /** #EXP-1388: the multi-delete confirm's title. */
    fun chatDeleteTitle(count: Int): String =
        if (count == 1) Copy.CONFIRM_DELETE_TITLE else Copy.SELECTION_CONFIRM_DELETE_MANY.replace("{n}", count.toString())

    /** #EXP-1388: the selection bar's failure toast: `selectionPartial` when
     *  SOME of the [total] chats failed, else `selectionFailed`. */
    fun chatSelectionFailedTitle(failed: Int, total: Int): String =
        if (failed in 1 until total) {
            Copy.TOAST_SELECTION_PARTIAL.replace("{n}", failed.toString()).replace("{m}", total.toString())
        } else {
            Copy.TOAST_SELECTION_FAILED
        }

    fun chatSelectionLabel(action: ChatSelectionAction): String = when (action) {
        ChatSelectionAction.MarkRead -> Copy.SELECTION_MARK_READ
        ChatSelectionAction.MarkUnread -> Copy.SELECTION_MARK_UNREAD
        ChatSelectionAction.Archive -> Copy.SELECTION_ARCHIVE
        ChatSelectionAction.Unarchive -> Copy.SELECTION_UNARCHIVE
        ChatSelectionAction.Delete -> Copy.SELECTION_DELETE
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

    /** S-3: an agent row pinned on top of the Chats list (`agentRowCases`). */
    data class AgentChatRow(
        val agentId: String,
        /** The viewer's conversation with the agent; null = none yet. */
        val sessionId: String?,
        val name: String,
        val icon: String,
        val color: String,
        /** The viewer's Assistant: the brand mark instead of icon/color. */
        val assistant: Boolean,
        val preview: ChatPreview,
        /** [chatTimeLabel] of [activityAt]; empty without a conversation. */
        val time: String,
        val activityAt: String?,
        val unreadCount: Int,
        val needsYou: Boolean,
        val busy: Boolean,
    )

    /** S-3: the Chats list's pinned AGENTS section (`agentRowCases`): one row
     *  per [SessionTree.listAgents] row (that order), filtered by the chip
     *  like a chat; none on the Archived page. */
    fun agentChatRows(
        agents: List<SessionTree.AgentRow>,
        sessions: List<SessionTree.Session>,
        messages: List<SessionTree.Message>,
        reads: List<SessionTree.SessionRead> = emptyList(),
        me: String?,
        filter: ChatFilter,
        view: ChatListView = ChatListView.List,
        now: String,
        utcOffsetMinutes: Int,
    ): List<AgentChatRow> {
        if (view == ChatListView.Archived) return emptyList()
        return agents.map { agent ->
            val conversation = sessions.firstOrNull { it.parentId == null && it.agentId == agent.agentId && it.userId == me }
            val id = conversation?.id
            var newest: String? = null
            if (id != null) {
                for (m in messages) {
                    if (m.sessionId != id) continue
                    if (m.authorKind != DomainContract.messageAuthorKindUser && m.authorKind != DomainContract.messageAuthorKindAgent) continue
                    if ((ms(m.createdAt) ?: 0L) > (ms(newest) ?: -1L)) newest = m.createdAt
                }
            }
            val activityAt = newest ?: conversation?.createdAt
            AgentChatRow(
                agentId = agent.agentId,
                sessionId = id,
                name = agent.name,
                icon = agent.icon,
                color = agent.color,
                assistant = agent.assistant,
                preview = if (id != null) chatPreview(id, messages, agent.needsYou, agent.busy) else ChatPreview("empty", Copy.PREVIEW_EMPTY),
                time = activityAt?.let { chatTimeLabel(it, now, utcOffsetMinutes) } ?: "",
                activityAt = activityAt,
                unreadCount = if (id != null) unreadCount(id, messages, reads) else 0,
                needsYou = agent.needsYou,
                busy = agent.busy,
            )
        }.filter {
            when (filter) {
                ChatFilter.All -> true
                ChatFilter.NeedsYou -> it.needsYou
                ChatFilter.Running -> it.busy
                ChatFilter.Unread -> it.unreadCount > 0
            }
        }
    }

    enum class ThreadMenuEntry(val wire: String) { Delete("delete") }

    /** A chat row's context menu ×4 (`menuCases`): S-3 (owner 2026-10-28) no
     *  Stop/End anywhere: Delete for a viewer who can steer the chat and may
     *  delete it (the chat's starter or a team owner, the server's
     *  `sessions.delete` rule, which ends what is live first); else nothing. */
    @Suppress("UNUSED_PARAMETER")
    fun threadMenuEntries(rootStatus: String, canSteer: Boolean, canDelete: Boolean): List<ThreadMenuEntry> =
        if (canSteer && canDelete) listOf(ThreadMenuEntry.Delete) else emptyList()

    enum class AgentMenuEntry(val wire: String) {
        Compact("compact"),
        NewConversation("newConversation"),
        Settings("settings"),
    }

    /** The agent chat's header menu (and its bubble's long-press) ×4
     *  (`agentMenuCases`). S-3: no Stop (the composer's stop-turn is the one). */
    @Suppress("UNUSED_PARAMETER")
    fun agentMenuEntries(busy: Boolean, isOwner: Boolean): List<AgentMenuEntry> = buildList {
        add(AgentMenuEntry.Compact)
        add(AgentMenuEntry.NewConversation)
        if (isOwner) add(AgentMenuEntry.Settings)
    }

    /** What a menu entry sends (`agentMenu.sends`); null = not a send. */
    fun agentMenuSend(entry: AgentMenuEntry): String? = when (entry) {
        AgentMenuEntry.Compact -> Sends.COMPACT
        AgentMenuEntry.NewConversation -> Sends.NEW_CONVERSATION
        else -> null
    }

    fun threadMenuLabel(entry: ThreadMenuEntry): String = when (entry) {
        ThreadMenuEntry.Delete -> Copy.MENU_DELETE
    }

    fun agentMenuLabel(entry: AgentMenuEntry): String = when (entry) {
        AgentMenuEntry.Compact -> Copy.MENU_COMPACT
        AgentMenuEntry.NewConversation -> Copy.MENU_NEW_CONVERSATION
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
