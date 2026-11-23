package com.exponential.app.ui.chats

import com.exponential.app.data.db.AgentDefinitionEntity
import com.exponential.app.data.db.SessionEntity
import com.exponential.app.data.db.agentChatLookup
import com.exponential.app.domain.SessionTree
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #EXP-1990 (agent-settings.json `groupChat`): every lookup of "the viewer's
 * chat with an agent" goes through `agentChatOf`: the send target's Room
 * lookup, the per-agent conversation map, the rail rows and Needs you.
 */
class GroupChatLookupTest {
    private fun row(id: String, agentId: String?, userId: String, parentId: String? = null) =
        SessionEntity(id = id, parentId = parentId, rootId = parentId ?: id, userId = userId, agentId = agentId)

    private fun lookup(rows: List<SessionEntity>, groupChatId: String?, me: String?) = runBlocking {
        agentChatLookup(
            agentId = "ag1",
            userId = me,
            groupChatId = { groupChatId },
            byId = { id -> rows.firstOrNull { it.id == id } },
            own = { agent, user -> rows.firstOrNull { it.parentId == null && it.agentId == agent && it.userId == user } },
        )
    }

    @Test
    fun `the send target reads the group chat, else the viewer's own root`() {
        val mine = row("c-me", "ag1", "u-me")
        val group = row("g1", "ag1", "u-owner")
        // Switch off: the viewer's own conversation, as before.
        assertEquals("c-me", lookup(listOf(mine), null, "u-me")?.id)
        assertNull(lookup(listOf(mine), null, null))
        // Switch on: the group chat whoever started it, never the old one.
        assertEquals("g1", lookup(listOf(mine, group), "g1", "u-me")?.id)
        // Not synced yet: none (the first send opens a fresh tree), never the old one.
        assertNull(lookup(listOf(mine), "g1", "u-me"))
        // A child never counts as the chat.
        assertNull(lookup(listOf(row("g1", "ag1", "u-owner", parentId = "x")), "g1", "u-me"))
    }

    private fun session(id: String, agentId: String?, userId: String) = SessionTree.Session(
        id = id, teamId = "t1", parentId = null, rootId = id, userId = userId, status = "idle", busy = false,
        needsInput = false, blocked = null, shared = true, title = null, guide = null, startedAt = null, endedAt = null,
        createdAt = "2027-01-15T07:00:00.000Z", updatedAt = "2027-01-15T07:00:00.000Z", agentId = agentId,
    )

    @Test
    fun `the per-agent chat map follows the group chats`() {
        val sessions = listOf(
            session("c-me", "ag1", "u-me"),
            session("g2", "ag2", "u-owner"),
            session("c-other", "ag3", "u-other"),
        )
        // Switch off: only the viewer's own roots.
        assertEquals(mapOf("ag1" to "c-me"), SessionTree.conversations(sessions, "u-me"))
        // ag2's group chat is synced; ag1's names a root not synced: none.
        assertEquals(
            mapOf("ag2" to "g2"),
            SessionTree.conversations(sessions, "u-me", mapOf("ag1" to "g-missing", "ag2" to "g2")),
        )
    }

    @Test
    fun `the agents rows carry the group chat id and read its state`() {
        val decode = Json { ignoreUnknownKeys = true }
        val entity = decode.decodeFromString(
            AgentDefinitionEntity.serializer(),
            """{"id":"ag2","team_id":"t1","name":"Release","group_chat_id":"g2"}""",
        )
        val definition = SessionTree.agentDefinition(entity)
        assertEquals("g2", definition.groupChatId)
        val busy = session("g2", "ag2", "u-owner").copy(busy = true)
        val rows = SessionTree.listAgents(listOf(definition), listOf(busy), "t1", "u-me")
        assertEquals("g2", rows.single().groupChatId)
        assertEquals(true, rows.single().busy)
        assertEquals(listOf("g2"), SessionTree.groupChatIds(listOf(entity)))
    }

    @Test
    fun `needs you counts a group chat for every member`() {
        val card = Json.parseToJsonElement("""{"kind":"question","question":"Ship it?","options":["Yes"]}""").jsonObject
        val ask = SessionTree.Message(
            id = "q1", sessionId = "g2", authorKind = "agent", authorId = null, body = "", card = card,
            answeredAt = null, consumedAt = null, createdAt = "2027-01-15T07:00:00.000Z", authorSessionId = "g2",
        )
        fun fold(groupChatId: String?) = needsYouFold(
            sessions = listOf(session("g2", "ag2", "u-owner")),
            links = emptyList(),
            issues = emptyList(),
            messages = listOf(ask),
            definitions = listOf(SessionTree.AgentDefinition("ag2", "t1", "Release", null, null, 0.0, groupChatId = groupChatId)),
            pullRequests = emptyList(),
            team = "t1",
            me = "u-me",
            now = 1_800_000_000_000L,
        )
        // A teammate's private conversation (no group chat): never the viewer's.
        assertEquals(emptyList<String>(), fold(null).map { it.messageId })
        // The agent's group chat: every member's.
        assertEquals(listOf("q1"), fold("g2").map { it.messageId })
        assertEquals("Release", fold("g2").single().chat)
    }
}
