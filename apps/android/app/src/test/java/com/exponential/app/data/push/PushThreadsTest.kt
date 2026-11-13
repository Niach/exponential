package com.exponential.app.data.push

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

/** #EXP-1815: a posted push's thread (group + tag) and id. */
class PushThreadsTest {
    @Test
    fun `the server's thread wins`() {
        assertEquals("session:r1", PushThreads.thread(mapOf("thread" to "session:r1", "issueId" to "i1")))
    }

    @Test
    fun `an older server's push derives it like notificationThread`() {
        assertEquals("issue:i1", PushThreads.thread(mapOf("type" to "issue_comment", "issueId" to "i1")))
        assertEquals("session:r1", PushThreads.thread(mapOf("type" to "agent_message", "sessionId" to "c1", "rootId" to "r1")))
        assertEquals("session:c1", PushThreads.thread(mapOf("type" to "agent_message", "sessionId" to "c1")))
        assertEquals("device:d1", PushThreads.thread(mapOf("type" to "agent_signed_out", "deviceId" to "d1")))
        assertEquals("inbox", PushThreads.thread(mapOf("type" to "issue_comment")))
    }

    @Test
    fun `id = the notification id's hash, never the summary's`() {
        assertEquals("n1".hashCode(), PushThreads.notificationKey("n1", 7))
        assertEquals(7, PushThreads.notificationKey(null, 7))
        assertEquals(7, PushThreads.notificationKey("", 7))
        assertNotEquals(PushThreads.SUMMARY_ID, PushThreads.notificationKey(null, PushThreads.SUMMARY_ID))
    }
}
