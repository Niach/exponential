package com.exponential.app.domain

/**
 * EXP-820: the chips over the Agent page's empty prompt box. A POOL rather than
 * three fixed chips — the chat is a conversation with an agent that holds the
 * whole product's MCP surface (issues, boards, labels, automations, sessions on
 * other devices, reviews), so the chips are there to SHOW that range, the way
 * the getting-started cards do: each mount draws a few at random.
 *
 * [POOL] is hand-mirrored BYTE-FOR-BYTE ×4 (web `lib/chat-suggestions.ts`
 * `CHAT_SUGGESTION_POOL`, desktop `chat_screen.rs`, iOS) — change all four
 * together. A suggestion ending in `#` lands the caret right behind that `#` so
 * the issue-ref autocomplete opens at once ([caretOffset]); the rest are
 * complete prompts.
 */
object ChatSuggestions {
    val POOL: List<String> = listOf(
        "Fix #",
        "Explain #",
        "Review #",
        "Split # into sub-issues",
        "Label every issue in the backlog",
        "Set a priority on every unprioritized issue",
        "Find duplicate issues and link them",
        "Do a code review of the open PRs and file the findings on a new board",
        "Create an automation that labels new issues",
        "Set up a weekly standup digest automation",
        "Draft release notes from the issues completed this month",
        "Summarize what changed across the boards this week",
        "Start a session for # on my other machine",
        "Move stale in-progress issues back to the backlog",
        "Comment a plan on #",
        "Which issues are blocked, and by what?",
    )

    /** How many chips a mount shows — enough to read as a range, few enough to
     *  stay a couple of rows on a phone. */
    const val COUNT = 4

    /**
     * [count] distinct suggestions drawn from [POOL], in pool order. [random]
     * takes a `[0,1)` draw (a fixed function in tests), exactly like the web's
     * `pickChatSuggestions`.
     */
    fun pick(count: Int = COUNT, random: () -> Double = { Math.random() }): List<String> {
        val indices = POOL.indices.toMutableList()
        val take = minOf(count, indices.size)
        // A partial Fisher–Yates: the first `take` slots end up a uniform draw.
        for (i in 0 until take) {
            val j = i + (random() * (indices.size - i)).toInt().coerceIn(0, indices.size - 1 - i)
            val swap = indices[i]
            indices[i] = indices[j]
            indices[j] = swap
        }
        return indices.take(take).sorted().map { POOL[it] }
    }

    /**
     * Where the caret lands once a suggestion is inserted: right behind its `#`
     * placeholder when it has one (so the `#` autocomplete opens on the spot),
     * the end of the text otherwise.
     */
    fun caretOffset(text: String): Int {
        val hash = text.indexOf('#')
        return if (hash >= 0) hash + 1 else text.length
    }
}
