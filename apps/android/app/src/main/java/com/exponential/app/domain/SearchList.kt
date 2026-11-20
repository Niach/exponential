package com.exponential.app.domain

// SLOP-28 (EXP-1259 D4/D5): THE search list x4, the pure half — the `#`
// typeahead's trigger + prefill rule and the splice a pick performs, plus the
// list's geometry. Hand-copied from
// `packages/domain-contract/fixtures/search-list.json` and locked to it by
// SearchListTest (which replays `typeaheadPrefill`). The Compose half is
// `ui/components/SearchList.kt` (the input row + the issue search sheet) over
// the shared Picker rows.

/** A live `#` token: where its `#` sits and the text typed after it. */
data class TypeaheadPrefill(val start: Int, val query: String)

object SearchList {
    const val INPUT_HEIGHT = 40
    const val INPUT_ICON = "nav-search"
    const val ROW_HEIGHT = 32
    const val ROW_HEIGHT_PHONE = 44
    const val MAX_VISIBLE_ROWS = 8
    const val FADE_HEIGHT = 24
    const val POPOVER_WIDTH = 560

    const val LIST_TEST_ID = "search-list"
    const val INPUT_TEST_ID = "search-list-input"
    const val ROW_TEST_ID = "search-list-row"
    const val COUNT_TEST_ID = "search-list-count"

    /**
     * The `#` token ending at [caret], or null when there is none: the `#`
     * must open the text or follow whitespace or `(` (so `C#sharp` is not a
     * trigger), and only token characters (`[A-Za-z0-9-]`) may sit between
     * it and the caret (whitespace or punctuation ends it). A `#` at the
     * START of a line waits for a token character (`#E` opens with `E`), so
     * `# Title` and `## H` stay headings. The query is everything typed after
     * the `#`, which is what the sheet's input opens prefilled with.
     */
    fun typeaheadPrefill(text: String, caret: Int): TypeaheadPrefill? {
        val end = caret.coerceIn(0, text.length)
        var i = end - 1
        while (i >= 0) {
            val c = text[i]
            if (c == '#') {
                val before = text.getOrNull(i - 1)
                val lineStart = before == null || before == '\n' || before == '\r'
                if (!lineStart && !before!!.isWhitespace() && before != '(') return null
                if (lineStart && i + 1 == end) return null
                return TypeaheadPrefill(start = i, query = text.substring(i + 1, end))
            }
            if (!isTokenChar(c)) return null
            i--
        }
        return null
    }

    private fun isTokenChar(c: Char): Boolean =
        c in 'A'..'Z' || c in 'a'..'z' || c in '0'..'9' || c == '-'

    /**
     * A pick: exactly `[start, start + 1 + queryLength)` (the `#` and the
     * token as recorded when the sheet opened, never text after the caret)
     * becomes `#IDENT `, and the caret lands after the space. Null when
     * [start] no longer holds a `#` (the text changed under the sheet).
     */
    fun spliceTypeaheadPick(text: String, start: Int, queryLength: Int, identifier: String): Pair<String, Int>? {
        val end = recordedEnd(text, start, queryLength) ?: return null
        // A space the text already has is reused rather than doubled.
        val after = text.substring(end).removePrefix(" ")
        val inserted = "#$identifier "
        return Pair(text.substring(0, start) + inserted + after, start + inserted.length)
    }

    /**
     * Backspace in the sheet's EMPTY input closes the list and deletes the
     * `#` and the query typed so far (`1 + queryLength` from the recorded
     * start).
     */
    fun deleteTypeaheadToken(text: String, start: Int, queryLength: Int): Pair<String, Int>? {
        val end = recordedEnd(text, start, queryLength) ?: return null
        return Pair(text.substring(0, start) + text.substring(end), start)
    }

    private fun recordedEnd(text: String, start: Int, queryLength: Int): Int? {
        if (start !in text.indices || text[start] != '#') return null
        return (start + 1 + queryLength.coerceAtLeast(0)).coerceAtMost(text.length)
    }

    // ── #EDE-99 CHATS (search-list.json `chats`, its `$comment` tail) ──────
    // The global search's Chats section over the chat store's `searchAll`.
    // Pure: which sections show, the chat rows, what a pick opens and where
    // find in chat starts. Mirrors the TS reference `src/search-list.ts`,
    // desktop `domain::search_list` and iOS; SearchListChatsTest replays it.

    object Chats {
        const val DEBOUNCE_MS = 150L
        const val CHATS_MAX = 20
        const val ROW_HEIGHT = 52
        const val ROW_HEIGHT_PHONE = 64
        const val ICON_CHAT = "search-chat"
        const val ICON_AGENT = "search-agent"

        object Copy {
            const val ISSUES = "Issues"
            const val CHATS = "Chats"
            const val YOU = "You: "
            const val NO_RESULTS = "No results"
            const val PLACEHOLDER = "Search issues and chats"
        }

        object TestIds {
            const val ISSUES = "search-list-issues"
            const val CHATS = "search-list-chats"
            const val CHAT_ROW = "search-list-chat-row"
        }
    }

    /** One chat store hit (chat-store.json SEARCH; `at` in ms). */
    data class ChatHit(
        val rootId: String,
        val sessionId: String,
        val deviceId: String? = null,
        val kind: String,
        val at: Long,
        val seq: Long? = null,
        val messageId: String? = null,
        val snippet: String,
        val matchStart: Int = 0,
        val matchLen: Int = 0,
    )

    /** One chat of `searchAll`'s answer: its root, its hit count, its newest hit. */
    data class ChatMatch(val rootId: String, val count: Int, val hit: ChatHit)

    enum class SectionKey(val wire: String) { Issues("issues"), Chats("chats") }

    data class Section(val key: SectionKey, val count: Int, val loading: Boolean)

    /** The store's answer so far ([rows] = the chat rows shown). */
    data class ChatsState(val rows: Int, val loading: Boolean)

    data class Sections(val titled: Boolean, val sections: List<Section>, val count: Int?, val empty: Boolean)

    /** Which sections show; [chats] null = no chat store on this client. */
    fun globalSections(query: String, issues: Int, chats: ChatsState?): Sections {
        val blank = FindInChat.query(query).isEmpty()
        if (blank || chats == null) {
            return Sections(false, listOf(Section(SectionKey.Issues, issues, false)), if (blank) null else issues, false)
        }
        val sections = ArrayList<Section>(2)
        if (issues > 0) sections += Section(SectionKey.Issues, issues, false)
        if (chats.rows > 0) sections += Section(SectionKey.Chats, chats.rows, chats.loading)
        val count = issues + chats.rows
        return Sections(true, sections, count, count == 0 && !chats.loading)
    }

    /** A root of the active team I can see: its list title and whether an agent's. */
    data class ChatRoot(val title: String, val agent: Boolean)

    data class ChatRow(
        val key: String,
        val rootId: String,
        val icon: String,
        val title: String,
        val subtitle: String,
        val highlight: String,
        val trailing: String,
        /** The hit the row opens (not in the fixture's rows). */
        val hit: ChatHit,
    )

    /** The Chats rows: the store's order, roots of the active team only, at most [limit]. */
    fun globalChatRows(
        query: String,
        now: String,
        utcOffsetMinutes: Int,
        chats: List<ChatMatch>,
        roots: Map<String, ChatRoot>,
        messageAuthors: Map<String, String>,
        limit: Int = Chats.CHATS_MAX,
    ): List<ChatRow> {
        val highlight = FindInChat.query(query)
        val rows = ArrayList<ChatRow>()
        for (chat in chats) {
            if (rows.size >= limit) break
            val root = roots[chat.rootId] ?: continue
            val hit = chat.hit
            val person = hit.kind == "user_message" ||
                (hit.kind == "message" && !hit.messageId.isNullOrEmpty() && messageAuthors[hit.messageId] == "user")
            rows += ChatRow(
                key = "chat:${chat.rootId}",
                rootId = chat.rootId,
                icon = if (root.agent) Chats.ICON_AGENT else Chats.ICON_CHAT,
                title = root.title,
                subtitle = (if (person) Chats.Copy.YOU else "") + hit.snippet,
                highlight = highlight,
                trailing = com.exponential.app.domain.Chats.chatTimeLabel(java.time.Instant.ofEpochMilli(hit.at).toString(), now, utcOffsetMinutes),
                hit = hit,
            )
        }
        return rows
    }

    /** Where find in chat starts after a pick: the stop's key and whether it is a tool output. */
    data class ChatTarget(val key: String, val output: Boolean)

    data class ChatPick(val sessionId: String, val query: String, val target: ChatTarget)

    /** A pick: the session to open and find in chat's query + start target. */
    fun globalChatPick(query: String, hit: ChatHit): ChatPick {
        val key = if (hit.kind == "message") FindInChat.messageKey(hit.messageId.orEmpty()) else FindInChat.eventKey(hit.sessionId, hit.seq ?: 0)
        return ChatPick(hit.sessionId, FindInChat.query(query), ChatTarget(key, hit.kind == "tool_output"))
    }

    /** Find in chat's start for a pick: the target's first stop, else the newest (null = no stops). */
    fun globalChatStart(stops: List<FindInChat.Stop>, target: ChatTarget): Int? {
        if (stops.isEmpty()) return null
        val at = stops.indexOfFirst { it.key == target.key && it.output == target.output }
        return if (at >= 0) at else stops.lastIndex
    }
}
