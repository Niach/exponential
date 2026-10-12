package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

// feedback-states.json `starting` (final review): one startId per draft
// start, bound to the device + the draft fingerprint. iOS mirrors these cases
// (`DraftStartTests.swift`).
class DraftStartTest {

    private val draft = DraftStart.Fingerprint("fix the login", listOf("i1"), null, listOf("a1"))
    private var counter = 0
    private val mint = { "id-${++counter}" }

    @Test
    fun theFirstStartMints() {
        val start = DraftStart.resolve(null, "mac", draft, mint)
        assertEquals("id-1", start.startId)
        assertEquals("mac", start.deviceId)
    }

    @Test
    fun theSameDraftOnTheSameDeviceReusesTheId() {
        val first = DraftStart.resolve(null, "mac", draft, mint)
        assertSame(first, DraftStart.resolve(first, "mac", draft, mint))
    }

    @Test
    fun anotherDeviceMintsAFreshId() {
        val first = DraftStart.resolve(null, "mac", draft, mint)
        assertEquals("id-2", DraftStart.resolve(first, "linux", draft, mint).startId)
    }

    @Test
    fun aChangedDraftMintsAFreshId() {
        val first = DraftStart.resolve(null, "mac", draft, mint)
        listOf(
            draft.copy(text = "fix it"),
            draft.copy(issueIds = listOf("i1", "i2")),
            draft.copy(issueIds = emptyList(), actionId = "act"),
            draft.copy(attachments = emptyList()),
        ).forEach { change ->
            assertNotEquals(first.startId, DraftStart.resolve(first, "mac", change, mint).startId)
        }
    }

    @Test
    fun aLandingClearsOnlyAnUnchangedDraft() {
        val start = DraftStart.resolve(null, "mac", draft, mint)
        assertTrue(start.landsInto(draft))
        assertFalse(start.landsInto(DraftStart.Fingerprint("a new prompt", emptyList(), null, emptyList())))
    }

    @Test
    fun theQuietWatchSpansTheDedupeWindow() {
        assertEquals(1_800_000L, StartedRunMatch.WATCH_MS)
        assertEquals(FeedbackStates.STARTING_DEDUPE_MS, FeedbackStates.STARTING_WATCH_MS)
    }
}
