package com.exponential.app.domain

import java.time.Instant
import org.junit.Assert.assertEquals
import org.junit.Test

/** #NEW-37: the captions match web `formatArchivedOn` / `formatPurgeCountdown` and desktop `all_boards.rs`. */
class PutAwayBoardCaptionTest {
    @Test
    fun archivedCaptionMatchesWeb() {
        assertEquals("Archived Jul 1, 2026", PutAwayBoardCaption.archived("2026-07-01T10:00:00.000Z"))
        assertEquals("Archived", PutAwayBoardCaption.archived(null))
    }

    @Test
    fun purgeCountdownMatchesWeb() {
        val now = Instant.parse("2026-07-01T10:00:00Z")
        assertEquals("Purges in ~42h", PutAwayBoardCaption.purge("2026-07-03T03:30:00.000Z", now))
        assertEquals("Purging soon", PutAwayBoardCaption.purge("2026-07-01T09:00:00.000Z", now))
        assertEquals("Purges soon", PutAwayBoardCaption.purge(null, now))
    }
}
