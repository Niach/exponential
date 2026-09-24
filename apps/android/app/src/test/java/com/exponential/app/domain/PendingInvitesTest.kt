package com.exponential.app.domain

import com.exponential.app.data.db.TeamInviteEntity
import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-630 (#810): the server no longer deletes a revoked placeholder invite —
// it keeps the row with `expires_at = now`. "Pending" must therefore be
// derived client-side as unaccepted AND unexpired, the predicate web's
// members-section.tsx applies to the same synced rows; the DAO's
// `accepted_at IS NULL` alone would list revoked and lapsed links.
class PendingInvitesTest {

    private val nowMs = java.time.Instant.parse("2026-09-24T10:00:00Z").toEpochMilli()

    private fun invite(
        id: String,
        expiresAt: String,
        acceptedAt: String? = null,
        placeholderUserId: String? = null,
    ) = TeamInviteEntity(
        id = id,
        teamId = "team-1",
        role = "member",
        placeholderUserId = placeholderUserId,
        acceptedAt = acceptedAt,
        expiresAt = expiresAt,
        createdAt = "2026-09-17 10:00:00+00",
        updatedAt = "2026-09-17 10:00:00+00",
    )

    @Test
    fun keepsOnlyUnacceptedUnexpiredLinks() {
        val rows = listOf(
            invite("open", expiresAt = "2026-10-01 10:00:00+00"),
            // A revoke stamps expires_at = now (server time, so in the past
            // by the time the row syncs) and leaves the row in place.
            invite("revoked", expiresAt = "2026-09-24 09:59:30+00", placeholderUserId = "p1"),
            invite("lapsed", expiresAt = "2026-09-20 10:00:00+00"),
            invite("joined", expiresAt = "2026-10-01 10:00:00+00", acceptedAt = "2026-09-21 10:00:00+00"),
        )
        assertEquals(listOf("open"), pendingInvites(rows, nowMs).map { it.id })
    }

    @Test
    fun expiryIsStrictSoAnInviteExpiringNowIsAlreadyGone() {
        val atNow = invite("edge", expiresAt = "2026-09-24T10:00:00Z")
        assertEquals(emptyList<String>(), pendingInvites(listOf(atNow), nowMs).map { it.id })
        assertEquals(listOf("edge"), pendingInvites(listOf(atNow), nowMs - 1).map { it.id })
    }

    @Test
    fun readsBothWireFormsAndTreatsGarbageAsLapsed() {
        val rows = listOf(
            invite("pg", expiresAt = "2026-10-01 10:00:00+00"),
            invite("iso", expiresAt = "2026-10-01T10:00:00.000Z"),
            invite("garbage", expiresAt = "not a date"),
        )
        assertEquals(listOf("pg", "iso"), pendingInvites(rows, nowMs).map { it.id })
    }

    @Test
    fun agreesWithThePlaceholderBadgeOnTheSameRows() {
        // The badge needs the lapsed row (it says "Invite expired"); the
        // pending list must not — same input, complementary reads.
        val rows = listOf(invite("revoked", expiresAt = "2026-09-24 09:59:30+00", placeholderUserId = "p1"))
        assertEquals(PlaceholderStatus.EXPIRED, placeholderStatuses(rows, nowMs)["p1"])
        assertEquals(emptyList<String>(), pendingInvites(rows, nowMs).map { it.id })
    }
}
