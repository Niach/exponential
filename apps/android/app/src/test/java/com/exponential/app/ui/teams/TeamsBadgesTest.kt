package com.exponential.app.ui.teams

import com.exponential.app.domain.NavBadges
import com.exponential.app.domain.SidebarTree
import org.junit.Assert.assertEquals
import org.junit.Test

/** nav-badges.json on the Teams tab: teams, folders and the tab itself. */
class TeamsBadgesTest {
    private val unread = NavBadges.Badge(3, NavBadges.Tone.Muted)
    private val asks = NavBadges.Badge(1, NavBadges.Tone.Attention)
    private val leaf: (String) -> NavBadges.Badge = { mapOf("a" to unread, "s" to asks)[it] ?: NavBadges.NONE }
    private val needsYou = mapOf("t1" to 1, "t2" to 0)
    private val state = TeamsState(
        inside = mapOf("t1" to listOf("a", "s", "b"), "f" to listOf("a", "s")),
        trees = listOf("t1", "t2").associateWith { SidebarTree.PhoneTreeTeam(it, emptyList(), emptyList(), true, emptySet()) },
    )

    private fun badge(kind: String, id: String, open: Boolean) =
        state.groupBadge(SidebarTree.PhoneTreeRow(kind, id, "t1", 0, null, open, 0), needsYou, leaf)

    @Test
    fun `a team's share counts once, folded adds its hidden unread`() {
        assertEquals(NavBadges.Badge(1, NavBadges.Tone.Attention), badge(SidebarTree.ROW_TEAM, "t1", open = true))
        assertEquals(NavBadges.Badge(4, NavBadges.Tone.Attention), badge(SidebarTree.ROW_TEAM, "t1", open = false))
        assertEquals(NavBadges.NONE, badge(SidebarTree.ROW_TEAM, "t2", open = false))
    }

    @Test
    fun `a folder sums its rows while folded`() {
        assertEquals(NavBadges.Badge(4, NavBadges.Tone.Attention), badge(SidebarTree.ROW_FOLDER, "f", open = false))
        assertEquals(NavBadges.NONE, badge(SidebarTree.ROW_FOLDER, "f", open = true))
    }

    @Test
    fun `the tab sums its teams, each folded`() {
        assertEquals(NavBadges.Badge(4, NavBadges.Tone.Attention), state.tabBadge(needsYou, leaf))
    }

    @Test
    fun `an agent's badge is its conversation's`() {
        val badgeOf = leafBadges(mapOf("c1" to unread), mapOf("agent" to "c1"))
        assertEquals(unread, badgeOf("agent"))
        assertEquals(NavBadges.NONE, badgeOf("board"))
    }
}
