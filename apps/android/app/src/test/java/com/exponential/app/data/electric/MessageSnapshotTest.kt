package com.exponential.app.data.electric

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test

// #EXP-1641: the on-demand read of one card's message (a subset snapshot body).
class MessageSnapshotTest {
    private val json = Json { ignoreUnknownKeys = true; isLenient = true }

    @Test
    fun `a subset snapshot's insert decodes raw, card kept as synced`() {
        val body = """{"metadata":{"snapshot_mark":1},"data":[{"key":"\"public\".\"messages\"/\"m\"","headers":{"operation":"insert"},"value":{"id":"m","conversation_id":"c","root_id":"r","team_id":"t","author_kind":"agent","body":"","card":"{\"kind\":\"question\",\"sealed\":\"exp-sealed:x\"}","created_at":"2026-11-06","updated_at":"2026-11-06"}}]}"""
        val rows = MessageSnapshot.rows(json, body)
        assertEquals(listOf("m"), rows.map { it.id })
        assertEquals("r", rows.single().rootId)
        assertEquals(true, rows.single().card?.contains("exp-sealed:x"))
    }

    @Test
    fun `no row is an empty read, a mangled body a failed one`() {
        assertEquals(emptyList<Any>(), MessageSnapshot.rows(json, """{"metadata":{},"data":[]}"""))
        assertThrows(kotlinx.io.IOException::class.java) { MessageSnapshot.rows(json, "<html>") }
    }
}
