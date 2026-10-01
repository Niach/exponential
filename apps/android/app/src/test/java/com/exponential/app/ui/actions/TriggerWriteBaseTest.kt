package com.exponential.app.ui.actions

import com.exponential.app.data.api.ActionDto
import org.junit.Assert.assertSame
import org.junit.Test

/**
 * SLOP-2: a trigger write replaces the WHOLE array, and the busy flag clears
 * when tRPC returns — before Electric echoes the row. [triggerWriteBase]
 * keeps the last write's response as the base until the synced row caught
 * up, so a second write in that gap cannot revert the first.
 */
class TriggerWriteBaseTest {

    private fun action(updatedAt: String, id: String = "a1") =
        ActionDto(id = id, teamId = "t1", name = "Nightly", updatedAt = updatedAt)

    @Test
    fun `no write yet builds on the synced row`() {
        val synced = action("2026-10-01 12:00:00.123456+00")
        assertSame(synced, triggerWriteBase(synced, null))
    }

    @Test
    fun `a pending echo builds on the write's response`() {
        val synced = action("2026-10-01 12:00:00.123456+00")
        val written = action("2026-10-01T12:00:05.000Z")
        assertSame(written, triggerWriteBase(synced, written))
    }

    @Test
    fun `the echo of the write hands back to the synced row`() {
        // The same instant in both wire forms: Postgres text keeps the
        // microseconds the tRPC response truncates.
        val synced = action("2026-10-01 12:00:05.000789+00")
        val written = action("2026-10-01T12:00:05.000Z")
        assertSame(synced, triggerWriteBase(synced, written))
    }

    @Test
    fun `a newer foreign write wins over the response`() {
        val synced = action("2026-10-01 12:01:00+00")
        val written = action("2026-10-01T12:00:05.000Z")
        assertSame(synced, triggerWriteBase(synced, written))
    }

    @Test
    fun `another action's response is ignored`() {
        val synced = action("2026-10-01 12:00:00+00")
        val written = action("2026-10-01T12:00:05.000Z", id = "a2")
        assertSame(synced, triggerWriteBase(synced, written))
    }

    @Test
    fun `an unreadable stamp never strands the base`() {
        val synced = action("2026-10-01 12:00:00+00")
        assertSame(synced, triggerWriteBase(synced, action("")))
        val written = action("2026-10-01T12:00:05.000Z")
        val unstamped = action("")
        assertSame(written, triggerWriteBase(unstamped, written))
    }
}
