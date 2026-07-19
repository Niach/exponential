package com.exponential.app.data.api

import com.exponential.app.data.db.TeamEntity
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the helpdesk tRPC wire shapes (EXP-180) against the server contract
 * (apps/web/src/lib/trpc/helpdesk.ts): camelCase Drizzle property names,
 * ISO timestamps, and full thread rows carrying server-only columns the DTOs
 * don't model (ignoreUnknownKeys — same config as HttpClientModule's Json).
 */
class HelpdeskWireFormatTest {

    private val json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        encodeDefaults = true
    }

    @Test
    fun listThreadsRowDecodesWithLastMessageAndUnread() {
        val payload = """
            [{
              "id": "5f2b8a1e-0000-4000-8000-000000000001",
              "teamId": "5f2b8a1e-0000-4000-8000-000000000002",
              "title": "Cannot sign in",
              "status": "open",
              "linkedIssueId": null,
              "reporterEmail": "jane@example.com",
              "reporterName": "Jane",
              "lastReporterSeenAt": "2026-07-18T10:00:00.000Z",
              "createdAt": "2026-07-17T09:00:00.000Z",
              "updatedAt": "2026-07-18T11:00:00.000Z",
              "lastMessage": {
                "body": "Still broken on my end.",
                "direction": "inbound",
                "createdAt": "2026-07-18T11:00:00.000Z"
              },
              "unread": true
            }]
        """.trimIndent()

        val rows = json.decodeFromString(ListSerializer(SupportThreadRow.serializer()), payload)
        val row = rows.single()
        assertEquals("Cannot sign in", row.title)
        assertEquals("open", row.status)
        assertNull(row.linkedIssueId)
        assertEquals("Jane", row.reporterName)
        assertEquals("inbound", row.lastMessage?.direction)
        assertTrue(row.unread)
    }

    @Test
    fun getThreadDecodesTheFullThreadRowMessagesAndLinkedIssue() {
        // The bare thread row has no lastMessage/unread and carries the
        // server-only tokenRevokedAt column — both must decode cleanly.
        val payload = """
            {
              "thread": {
                "id": "5f2b8a1e-0000-4000-8000-000000000001",
                "teamId": "5f2b8a1e-0000-4000-8000-000000000002",
                "title": "Crash on export",
                "status": "resolved",
                "linkedIssueId": "5f2b8a1e-0000-4000-8000-000000000003",
                "reporterEmail": "sam@example.com",
                "reporterName": null,
                "tokenRevokedAt": "2026-07-18T12:00:00.000Z",
                "lastReporterSeenAt": null,
                "createdAt": "2026-07-17T09:00:00.000Z",
                "updatedAt": "2026-07-18T12:00:00.000Z"
              },
              "messages": [{
                "id": "5f2b8a1e-0000-4000-8000-000000000010",
                "threadId": "5f2b8a1e-0000-4000-8000-000000000001",
                "authorUserId": null,
                "direction": "inbound",
                "visibility": "public",
                "body": "It crashes every time.",
                "emailDeliveryId": null,
                "createdAt": "2026-07-17T09:00:00.000Z",
                "updatedAt": "2026-07-17T09:00:00.000Z"
              }, {
                "id": "5f2b8a1e-0000-4000-8000-000000000011",
                "threadId": "5f2b8a1e-0000-4000-8000-000000000001",
                "authorUserId": "usr_1",
                "direction": "outbound",
                "visibility": "internal",
                "body": "Repro'd — escalating.",
                "emailDeliveryId": null,
                "createdAt": "2026-07-17T10:00:00.000Z",
                "updatedAt": "2026-07-17T10:00:00.000Z"
              }],
              "linkedIssue": {
                "id": "5f2b8a1e-0000-4000-8000-000000000003",
                "identifier": "EXP-42",
                "title": "Export crash",
                "status": "in_progress",
                "boardId": "5f2b8a1e-0000-4000-8000-000000000004"
              }
            }
        """.trimIndent()

        val detail = json.decodeFromString(SupportThreadDetail.serializer(), payload)
        assertEquals("resolved", detail.thread.status)
        assertNull(detail.thread.reporterName)
        assertFalse(detail.thread.unread)
        assertNull(detail.thread.lastMessage)
        assertEquals(2, detail.messages.size)
        assertNull(detail.messages[0].authorUserId)
        assertEquals("internal", detail.messages[1].visibility)
        assertEquals("EXP-42", detail.linkedIssue?.identifier)
    }

    @Test
    fun teamHelpdeskFlagTolerantlyDecodesEveryWireForm() {
        // Electric shape rows deliver snake_case with Postgres "t"/"f" bools;
        // tRPC delivers camelCase native booleans. Both must land on the flag.
        val electricRow = """
            {"id":"t1","name":"Acme","slug":"acme","helpdesk_enabled":"t",
             "created_at":"2026-07-17 09:00:00+00","updated_at":"2026-07-17 09:00:00+00"}
        """.trimIndent()
        val trpcRow = """
            {"id":"t1","name":"Acme","slug":"acme","helpdeskEnabled":true,
             "createdAt":"2026-07-17T09:00:00.000Z","updatedAt":"2026-07-17T09:00:00.000Z"}
        """.trimIndent()
        val legacyRow = """
            {"id":"t1","name":"Acme","slug":"acme",
             "created_at":"2026-07-17 09:00:00+00","updated_at":"2026-07-17 09:00:00+00"}
        """.trimIndent()

        assertTrue(json.decodeFromString(TeamEntity.serializer(), electricRow).helpdeskEnabled)
        assertTrue(json.decodeFromString(TeamEntity.serializer(), trpcRow).helpdeskEnabled)
        // Pre-helpdesk servers omit the column — the flag defaults off.
        assertFalse(json.decodeFromString(TeamEntity.serializer(), legacyRow).helpdeskEnabled)
    }
}
