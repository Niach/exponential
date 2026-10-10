package com.exponential.app.domain

import com.exponential.app.data.db.TeamMemberEntity
import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-1267 ×4 (web `membersInJoinOrder`): Settings › Members lists the roster
// in join order, across the mixed Postgres-text / ISO stamp forms the synced
// rows arrive in, ties by id.
class TeamMembersOrderTest {

    private fun member(id: String, createdAt: String) = TeamMemberEntity(
        id = id,
        teamId = "team-1",
        userId = "user-$id",
        role = "member",
        createdAt = createdAt,
        updatedAt = createdAt,
    )

    @Test
    fun `orders by the join stamp whatever its wire form`() {
        val rows = listOf(
            member("c", "2026-09-19T10:00:00.000Z"),
            member("a", "2026-09-17 10:00:00+00"),
            member("b", "2026-09-18T10:00:00Z"),
        )
        assertEquals(listOf("a", "b", "c"), membersInJoinOrder(rows).map { it.id })
    }

    @Test
    fun `breaks a tie by id and sorts an unreadable stamp last`() {
        val rows = listOf(
            member("z", "not a date"),
            member("b", "2026-09-17 10:00:00+00"),
            member("a", "2026-09-17T10:00:00Z"),
        )
        assertEquals(listOf("a", "b", "z"), membersInJoinOrder(rows).map { it.id })
    }
}
