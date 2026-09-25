package com.exponential.app.domain

import com.exponential.app.data.db.TeamInviteEntity
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// EXP-630: what the Members list reads off the synced invites — a member is
// "invited, not joined" while an invite bound to it is unaccepted. EXP-1076:
// an imported roster row nobody was ever sent a link for reads "Not invited",
// never "Invite expired". Mirrors web's placeholder-status.test.ts case for
// case.
class PlaceholderStatusTest {

    private val nowMs = instant("2026-09-24T10:00:00Z")
    private val later = "2026-10-01T10:00:00Z"
    private val earlier = "2026-09-20T10:00:00Z"

    private fun instant(iso: String) = java.time.Instant.parse(iso).toEpochMilli()

    private fun invite(
        id: String,
        placeholderUserId: String?,
        acceptedAt: String? = null,
        expiresAt: String,
        sentAt: String? = "2026-09-17 10:00:00+00",
    ) = TeamInviteEntity(
        id = id,
        teamId = "team-1",
        role = "member",
        placeholderUserId = placeholderUserId,
        sentAt = sentAt,
        acceptedAt = acceptedAt,
        expiresAt = expiresAt,
        createdAt = "2026-09-17 10:00:00+00",
        updatedAt = "2026-09-17 10:00:00+00",
    )

    @Test
    fun badgesPendingAndExpiredButNothingOnceAcceptedOrUnbound() {
        val statuses = placeholderStatuses(
            listOf(
                invite("i1", "p-pending", expiresAt = later),
                invite("i2", "p-expired", expiresAt = earlier),
                invite("i3", "p-joined", acceptedAt = earlier, expiresAt = later),
                invite("i4", null, expiresAt = later),
            ),
            nowMs,
        )
        assertEquals(
            listOf("p-pending" to PlaceholderStatus.PENDING, "p-expired" to PlaceholderStatus.EXPIRED),
            statuses.entries.map { it.key to it.value },
        )
    }

    @Test
    fun aFreshLinkOutranksTheExpiredOneItSuperseded() {
        val rows = listOf(
            invite("i1", "p", expiresAt = earlier),
            invite("i2", "p", expiresAt = later),
        )
        assertEquals(PlaceholderStatus.PENDING, placeholderStatuses(rows, nowMs)["p"])
        assertEquals(PlaceholderStatus.PENDING, placeholderStatuses(rows.reversed(), nowMs)["p"])
    }

    // EXP-1076: the import stamps expires_at = created_at, so the row is
    // "expired" by date from the moment it exists — the label must not say so.
    // An empty string reads the same as null.
    @Test
    fun readsANeverSentRowAsUnsentWhateverItsExpirySays() {
        val statuses = placeholderStatuses(
            listOf(
                invite("i1", "p-import", expiresAt = earlier, sentAt = null),
                invite("i2", "p-future", expiresAt = later, sentAt = null),
                invite("i3", "p-empty", expiresAt = later, sentAt = ""),
            ),
            nowMs,
        )
        assertEquals(PlaceholderStatus.UNSENT, statuses["p-import"])
        assertEquals(PlaceholderStatus.UNSENT, statuses["p-future"])
        assertEquals(PlaceholderStatus.UNSENT, statuses["p-empty"])
    }

    @Test
    fun ranksPendingOverExpiredOverUnsentInEitherOrder() {
        val unsent = invite("i-u", "p", expiresAt = earlier, sentAt = null)
        val expired = invite("i-e", "p", expiresAt = earlier)
        val pending = invite("i-p", "p", expiresAt = later)
        assertEquals(PlaceholderStatus.EXPIRED, placeholderStatuses(listOf(unsent, expired), nowMs)["p"])
        assertEquals(PlaceholderStatus.EXPIRED, placeholderStatuses(listOf(expired, unsent), nowMs)["p"])
        assertEquals(PlaceholderStatus.PENDING, placeholderStatuses(listOf(unsent, pending), nowMs)["p"])
        assertEquals(PlaceholderStatus.PENDING, placeholderStatuses(listOf(pending, unsent), nowMs)["p"])
        assertEquals(PlaceholderStatus.PENDING, placeholderStatuses(listOf(pending, expired, unsent), nowMs)["p"])
    }

    @Test
    fun labelsAllThreeStates() {
        assertEquals("Not invited", PlaceholderStatus.UNSENT.label)
        assertEquals("Invited", PlaceholderStatus.PENDING.label)
        assertEquals("Invite expired", PlaceholderStatus.EXPIRED.label)
    }

    @Test
    fun readsElectricPostgresTextAndTreatsGarbageAsLapsed() {
        // The synced form the shape actually delivers (space separator,
        // hour-only offset) — not ISO, which is what a tRPC-era write looks
        // like.
        assertEquals(
            PlaceholderStatus.PENDING,
            placeholderStatuses(listOf(invite("i1", "p", expiresAt = "2026-10-01 10:00:00+00")), nowMs)["p"],
        )
        assertEquals(
            PlaceholderStatus.EXPIRED,
            placeholderStatuses(listOf(invite("i1", "p", expiresAt = "not a date")), nowMs)["p"],
        )
        assertNull(placeholderStatuses(emptyList(), nowMs)["p"])
    }
}
