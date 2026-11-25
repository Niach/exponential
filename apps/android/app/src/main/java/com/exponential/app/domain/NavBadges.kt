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

    /** An agent row, a placed session row, an app row or a chat-list row
     *  (`rowCases`; the Assistant's disc too): its unread
     *  agent messages, at least 1 while it waits on the viewer. */
    fun rowBadge(unreadCount: Int, needsYou: Boolean): Badge {
        val unread = maxOf(0, unreadCount)
        if (needsYou) return Badge(maxOf(1, unread), Tone.Attention)
        return if (unread > 0) Badge(unread, Tone.Muted) else NONE
    }

    /** A parent's badge over the rows it hides (`sumCases`): a folded folder,
     *  the phones' Teams tab. Attention while a counted row needs the viewer. */
    fun sumBadges(rows: List<Badge>): Badge {
        val lit = rows.filter { it.count > 0 }
        return badge(lit.sumOf { it.count }, attention = lit.any { it.tone == Tone.Attention })
    }

    /** A team row (`teamCases`): its share of Needs you, and while folded its
     *  hidden rows' muted counts (the attention ones are in the share). */
    fun teamBadge(needsYou: Int, open: Boolean, rows: List<Badge>): Badge {
        val hidden = if (open) emptyList() else rows.filter { it.tone == Tone.Muted }
        return sumBadges(hidden + Badge(needsYou, Tone.Attention))
    }

    /** [total] = the Needs you badge; [teams] = each team's share. */
    data class NeedsYouCounts(val total: Int = 0, val teams: Map<String, Int> = emptyMap())

    /** Needs you (`needsYouCases`): [rows] = THE list over every member team
     *  and the Assistant's home, each its session's team; a row outside
     *  [teams] (the Assistant's) and the waiting [installs] belong to no team. */
    fun needsYouCounts(rows: List<String?>, teams: Collection<String>, installs: Int): NeedsYouCounts {
        val shares = teams.associateWithTo(LinkedHashMap()) { 0 }
        for (team in rows) if (team != null && team in shares) shares[team] = shares.getValue(team) + 1
        return NeedsYouCounts(rows.size + maxOf(0, installs), shares)
    }

    /** The OS app icon (`appIconCases`): every account's Needs you total. */
    fun appIconBadge(accountsNeedsYou: List<Int>): Int = accountsNeedsYou.sumOf { maxOf(0, it) }

    /** A row's team mark on its lead, bottom right (all dp): the mark's side,
     *  the ground's band around it, how far its box hangs past the lead. */
    data class TeamMarkSize(val mark: Int, val ring: Int, val offset: Int)

    /** `teamMark.sizes`, largest lead first. */
    private val TEAM_MARK_SIZES = listOf(
        40 to TeamMarkSize(mark = 20, ring = 2, offset = 2),
        24 to TeamMarkSize(mark = 14, ring = 2, offset = 3),
        0 to TeamMarkSize(mark = 10, ring = 1, offset = 4),
    )

    /** `teamMark.leadCases`. */
    fun teamMarkOf(lead: Float): TeamMarkSize =
        (TEAM_MARK_SIZES.firstOrNull { it.first <= lead } ?: TEAM_MARK_SIZES.last()).second

    /** [count] things, attention when one of them needs the viewer. */
    fun badge(count: Int, attention: Boolean): Badge =
        if (count <= 0) NONE else Badge(count, if (attention) Tone.Attention else Tone.Muted)
}
