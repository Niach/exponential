package com.exponential.app.domain

/**
 * #FEED-232 (nav-badges.json): ONE count badge ×4. Every nav destination says
 * HOW MANY things in it need the viewer, never a dot. The spec is the
 * fixture's `$comment`; TS reference `src/nav-badges.ts` (every case list
 * replays in `NavBadgesTest`).
 */
object NavBadges {
    const val MAX = 99

    enum class Tone { Attention, Muted }

    /** A destination's badge: [count] ≤ 0 = no badge. */
    data class Badge(val count: Int = 0, val tone: Tone = Tone.Muted)

    val NONE = Badge()

    /** What the badge prints: `""` (nothing renders) at zero or less, `99+`
     *  past the cap (`labelCases`). */
    fun countBadgeLabel(count: Int, max: Int = MAX): String = when {
        count <= 0 -> ""
        count > max -> "$max+"
        else -> count.toString()
    }

    /** The Chats destination (`chatsCases`): distinct depth-0 chats that need
     *  the viewer or hold an unread reply; an archived root counts only while
     *  it needs the viewer. Attention when one of them needs the viewer. */
    fun chatsBadge(groups: List<SessionTree.ChatsListGroup>, reads: List<SessionTree.SessionRead> = emptyList()): Badge {
        val archived = reads.filter { !it.archivedAt.isNullOrEmpty() }.mapTo(HashSet()) { it.sessionId }
        val counted = HashSet<String>()
        var attention = false
        for (group in groups) {
            for (row in group.rows) {
                if (row.depth != 0) continue
                if (!(row.needsYou || (row.unread && row.sessionId !in archived))) continue
                counted += row.sessionId
                if (row.needsYou) attention = true
            }
        }
        return badge(counted.size, attention)
    }

    /** An agent row, an app row or a chat-list row (`rowCases`): its unread
     *  agent messages, at least 1 while it waits on the viewer. */
    fun rowBadge(unreadCount: Int, needsYou: Boolean): Badge {
        val unread = maxOf(0, unreadCount)
        if (needsYou) return Badge(maxOf(1, unread), Tone.Attention)
        return if (unread > 0) Badge(unread, Tone.Muted) else NONE
    }

    /** The OS app icon (`appIconCases`): every team's needs-you rows summed. */
    fun appIconBadge(teamsNeedsYou: List<Int>): Int = teamsNeedsYou.sumOf { maxOf(0, it) }

    /** [count] chats, attention when one of them needs the viewer. */
    fun badge(count: Int, attention: Boolean): Badge =
        if (count <= 0) NONE else Badge(count, if (attention) Tone.Attention else Tone.Muted)
}
