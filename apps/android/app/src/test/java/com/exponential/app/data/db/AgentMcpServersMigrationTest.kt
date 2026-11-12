package com.exponential.app.data.db

import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * #EXP-1793 (agent-settings.json `mcpRow`): v122 adds `agents.mcp_server_ids`
 * and refetches ONLY the agents shape; the row decodes it tolerant of absence.
 */
class AgentMcpServersMigrationTest {
    private fun recording(statements: MutableList<String>): SupportSQLiteDatabase =
        Proxy.newProxyInstance(
            SupportSQLiteDatabase::class.java.classLoader,
            arrayOf(SupportSQLiteDatabase::class.java),
        ) { _, method, args ->
            if (method.name == "execSQL") {
                statements += args!![0] as String
                null
            } else {
                error("unexpected ${method.name}")
            }
        } as SupportSQLiteDatabase

    @Test
    fun `v122 adds mcp_server_ids and refetches only agents`() {
        assertEquals(121, MIGRATION_121_122.startVersion)
        assertEquals(122, MIGRATION_121_122.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_121_122.migrate(recording(statements))
        assertEquals(
            listOf(
                "ALTER TABLE `agents` ADD COLUMN `mcp_server_ids` TEXT",
                "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                    "`needs_refetch` = 1 WHERE `shape` = 'agents'",
            ),
            statements,
        )
    }

    @Test
    fun `the row decodes mcp_server_ids, absent or null = none`() {
        val json = Json { ignoreUnknownKeys = true }
        fun decode(raw: String) = json.decodeFromString(AgentDefinitionEntity.serializer(), raw)
        assertEquals(listOf("m1", "m2"), decode("""{"id":"a1","mcp_server_ids":["m1","m2"]}""").mcpServerIdList)
        assertEquals(listOf("m1"), decode("""{"id":"a1","mcp_server_ids":"[\"m1\"]"}""").mcpServerIdList)
        assertEquals(emptyList<String>(), decode("""{"id":"a1","mcp_server_ids":null}""").mcpServerIdList)
        assertEquals(emptyList<String>(), decode("""{"id":"a1"}""").mcpServerIdList)
    }
}
