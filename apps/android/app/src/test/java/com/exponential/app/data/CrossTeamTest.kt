package com.exponential.app.data

import com.exponential.app.data.db.TeamEntity
import com.exponential.app.ui.components.teamBands
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-1186: the cross-team lists band per team only with several teams, and a
// page opened from another team's row works in the ROW's team.
class CrossTeamTest {

    private fun team(id: String, name: String) = TeamEntity(
        id = id,
        name = name,
        slug = id,
        createdAt = "2026-10-01T00:00:00Z",
        updatedAt = "2026-10-01T00:00:00Z",
    )

    @Test
    fun `one team is one band with no team, several band in team order`() {
        val rows = listOf("b1" to "b", "a1" to "a", "b2" to "b", "x1" to "gone")
        val single = teamBands(rows, listOf(team("a", "Alpha"))) { it.second }
        assertEquals(1, single.size)
        assertNull(single.single().team)
        assertEquals(rows, single.single().items)

        val bands = teamBands(rows, listOf(team("a", "Alpha"), team("b", "Beta"))) { it.second }
        assertEquals(listOf("a", "b", null), bands.map { it.team?.id })
        assertEquals(listOf("b1", "b2"), bands[1].items.map { it.first })
        assertEquals(listOf("x1"), bands[2].items.map { it.first })
    }

    @Test
    fun `a team action, then a pr, then the first issue names the route's subject`() {
        assertEquals(
            RouteSubject.Action("act-1"),
            routeSubject(mapOf("actionId" to "act-1")::get),
        )
        assertEquals(
            RouteSubject.Action("act-2"),
            routeSubject(mapOf("action" to "act-2", "issues" to "i-1")::get),
        )
        // A builtin has no team row: its pr pick decides.
        assertEquals(
            RouteSubject.Issue("i-pr"),
            routeSubject(mapOf("action" to "builtin:fix-conflicts", "pr" to "i-pr")::get),
        )
        assertEquals(RouteSubject.Issue("i-1"), routeSubject(mapOf("issues" to "i-1,i-2")::get))
        // The Chat FAB names nothing: the selected team stands.
        assertNull(routeSubject(emptyMap<String, String>()::get))
        assertNull(routeSubject(mapOf("action" to "builtin:create-action")::get))
    }
}
