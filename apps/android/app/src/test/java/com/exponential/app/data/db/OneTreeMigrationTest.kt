package com.exponential.app.data.db

import com.exponential.app.domain.SessionTree
import androidx.sqlite.db.SupportSQLiteDatabase
import java.lang.reflect.Proxy
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #EXP-1730 phase A: v121 adds `messages.session_id` (+ index),
 * `sessions.digest`, `agents.defaults` and `apps.agent_id`, drops the local
 * `conversations` table and refetches the four shapes; the rows decode the
 * columns tolerant of absence and messages key by their own session.
 */
class OneTreeMigrationTest {
    private val json = Json { ignoreUnknownKeys = true }

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
    fun `v121 adds the one-tree columns, drops conversations and refetches four shapes`() {
        assertEquals(120, MIGRATION_120_121.startVersion)
        assertEquals(121, MIGRATION_120_121.endVersion)
        val statements = mutableListOf<String>()
        MIGRATION_120_121.migrate(recording(statements))
        assertEquals(
            listOf(
                "ALTER TABLE `messages` ADD COLUMN `session_id` TEXT",
                "CREATE INDEX IF NOT EXISTS `index_messages_session_id` ON `messages` (`session_id`)",
                "ALTER TABLE `sessions` ADD COLUMN `digest` TEXT",
                "ALTER TABLE `agents` ADD COLUMN `defaults` TEXT",
                "ALTER TABLE `apps` ADD COLUMN `agent_id` TEXT",
                "DROP TABLE IF EXISTS `conversations`",
                "DELETE FROM `electric_offsets` WHERE `shape` = 'conversations'",
                "UPDATE `electric_offsets` SET `handle` = '', `offset` = '-1', `is_live` = 0, " +
                    "`needs_refetch` = 1 WHERE `shape` IN ('messages', 'sessions', 'agents', 'apps')",
            ),
            statements,
        )
    }

    @Test
    fun `a message decodes its session and keys by it, a dm row is left out`() {
        val keyed = json.decodeFromString(MessageEntity.serializer(), """{"id":"m1","conversation_id":"c1","session_id":"s1","root_id":"r"}""")
        assertEquals("s1", keyed.sessionId)
        val dm = json.decodeFromString(MessageEntity.serializer(), """{"id":"m2","conversation_id":"c2"}""")
        assertNull(dm.sessionId)
        assertEquals(listOf("m1" to "s1"), SessionTree.messages(listOf(keyed, dm)).map { it.id to it.sessionId })
    }

    @Test
    fun `an agent reads its runner picks off defaults, absent or malformed = none`() {
        val row = json.decodeFromString(
            AgentDefinitionEntity.serializer(),
            """{"id":"ag","device_id":"legacy","cli":"codex",
               "defaults":{"deviceId":"dev-1","cli":"claude","account":"work","model":"opus","extra":1}}""",
        )
        assertEquals(AgentDefaults(deviceId = "dev-1", cli = "claude", account = "work", model = "opus"), row.runnerDefaults)
        val blank = json.decodeFromString(AgentDefinitionEntity.serializer(), """{"id":"ag","defaults":{"deviceId":"","cli":null}}""")
        assertEquals(AgentDefaults.NONE, blank.runnerDefaults)
        assertEquals(AgentDefaults.NONE, json.decodeFromString(AgentDefinitionEntity.serializer(), """{"id":"ag"}""").runnerDefaults)
        assertEquals(AgentDefaults.NONE, AgentDefaults.parse("[1]"))
    }

    @Test
    fun `an app decodes its agent, absent = standalone`() {
        assertEquals("ag", json.decodeFromString(AppEntity.serializer(), """{"id":"a","agent_id":"ag"}""").agentId)
        assertEquals("ag", json.decodeFromString(AppEntity.serializer(), """{"id":"a","agentId":"ag"}""").agentId)
        assertNull(json.decodeFromString(AppEntity.serializer(), """{"id":"a"}""").agentId)
    }
}
