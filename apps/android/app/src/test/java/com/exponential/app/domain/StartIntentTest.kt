package com.exponential.app.domain

import java.util.UUID
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertSame
import org.junit.Test

// EXP-1259 start surfaces: one startId per press-intent, reused by that
// press's Retry. iOS mirrors these cases (`StartIntentTests.swift`).
class StartIntentTest {

    private var counter = 0
    private val mint = { "id-${++counter}" }
    private val target = StartIntent.resumeTarget("s1", "mac", null)

    @Test
    fun aPressMints() {
        assertEquals("id-1", StartIntent.press(target, mint).startId)
    }

    @Test
    fun aRetryReusesThePressesId() {
        val press = StartIntent.press(target, mint)
        val retry = StartIntent.retry(press, target, mint)
        assertSame(press, retry)
        assertEquals("id-1", StartIntent.retry(retry, target, mint).startId)
    }

    @Test
    fun aNewPressMintsANewId() {
        val first = StartIntent.press(target, mint)
        val second = StartIntent.press(target, mint)
        assertNotEquals(first.startId, second.startId)
        assertEquals("id-2", second.startId)
    }

    @Test
    fun aRetryWithNothingPendingMints() {
        assertEquals("id-1", StartIntent.retry(null, target, mint).startId)
    }

    @Test
    fun aRetryOfAnotherTargetMints() {
        val press = StartIntent.press(target, mint)
        listOf(
            StartIntent.resumeTarget("s2", "mac", null),
            StartIntent.resumeTarget("s1", "linux", null),
            StartIntent.resumeTarget("s1", "mac", "profile-2"),
        ).forEach { other ->
            assertNotEquals(press.startId, StartIntent.retry(press, other, mint).startId)
        }
    }

    @Test
    fun theMintedIdIsAUuid() {
        val id = StartIntent.mint()
        assertEquals(id, UUID.fromString(id).toString())
    }
}
