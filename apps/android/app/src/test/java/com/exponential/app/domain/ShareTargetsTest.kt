package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-1635: sharing into Exponential, replayed off `share-targets.json`
// (TS `share-targets.test.ts`, same case names).
class ShareTargetsTest {
    private val fixture: JsonObject = Json.parseToJsonElement(contractFixtureJson("share-targets.json")).jsonObject

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content

    private fun cases(key: String): List<JsonObject> =
        fixture.getValue(key).jsonArray.map { it.jsonObject }.also { assertTrue(key, it.isNotEmpty()) }

    private fun agents(input: JsonObject): List<ShareTargets.Agent> =
        input.getValue("agents").jsonArray.map {
            val a = it.jsonObject
            ShareTargets.Agent(a.str("id")!!, a.str("name")!!, a.getValue("sortOrder").jsonPrimitive.int, a.str("role"), a.str("ownerId"))
        }

    private fun strings(element: JsonElement): List<String> = element.jsonArray.map { it.jsonPrimitive.content }

    @Test
    fun `copy, ids and limits are the fixture's`() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.str("title"), ShareTargets.TITLE)
        assertEquals(copy.str("newIssue"), ShareTargets.NEW_ISSUE_LABEL)
        assertEquals(copy.str("newChat"), ShareTargets.NEW_CHAT_LABEL)
        assertEquals(copy.str("newChatSubtitle"), ShareTargets.NEW_CHAT_SUBTITLE)
        assertEquals(copy.str("agents"), ShareTargets.AGENTS_LABEL)
        assertEquals(copy.str("imagesOne"), ShareTargets.IMAGES_ONE)
        assertEquals(copy.str("imagesMany"), ShareTargets.IMAGES_MANY)
        val ids = fixture.getValue("ids").jsonObject
        assertEquals(ids.str("newIssue"), ShareTargets.NEW_ISSUE)
        assertEquals(ids.str("newChat"), ShareTargets.NEW_CHAT)
        val icons = fixture.getValue("icons").jsonObject
        assertEquals(icons.str("newIssue"), ShareTargets.NEW_ISSUE_ICON)
        assertEquals(icons.str("newChat"), ShareTargets.NEW_CHAT_ICON)
        val direct = fixture.getValue("directShare").jsonObject
        assertEquals(direct.str("agentPrefix"), ShareTargets.AGENT_PREFIX)
        assertEquals(direct.str("chatPrefix"), ShareTargets.CHAT_PREFIX)
        assertEquals(direct.getValue("maxEntries").jsonPrimitive.int, ShareTargets.MAX_ENTRIES)
        val testIds = fixture.getValue("testIds").jsonObject
        assertEquals(testIds.str("page"), ShareTargets.TEST_ID_PAGE)
        assertEquals(testIds.str("preview"), ShareTargets.TEST_ID_PREVIEW)
        assertEquals(testIds.str("row")!!.replace("{id}", "x"), ShareTargets.rowTestId("x"))
    }

    @Test
    fun `the preview line counts images, a text-only share has none`() {
        assertEquals(null, ShareTargets.previewLabel(0))
        assertEquals("1 image", ShareTargets.previewLabel(1))
        assertEquals("3 images", ShareTargets.previewLabel(3))
    }

    @Test
    fun `targetCases match the fixture`() {
        for (case in cases("targetCases")) {
            val input = case.getValue("input").jsonObject
            val expected = case.getValue("expected").jsonObject
            val got = ShareTargets.shareTargets(agents(input), input.str("me")) { it }
            assertEquals(case.str("name"), strings(expected.getValue("rows")), got.rows)
            assertEquals(case.str("name"), strings(expected.getValue("agents")), got.agents.map { it.id })
        }
    }

    @Test
    fun `textCases match the fixture`() {
        for (case in cases("textCases")) {
            val input = case.getValue("input").jsonObject
            val got = ShareTargets.shareChatText(
                text = input.str("text"),
                subject = input.str("subject"),
                url = input.str("url"),
                hasImages = input.getValue("hasImages").jsonPrimitive.boolean,
            )
            assertEquals(case.str("name"), case.str("expected"), got)
        }
    }

    @Test
    fun `directShareCases match the fixture`() {
        for (case in cases("directShareCases")) {
            val input = case.getValue("input").jsonObject
            val chats = input.getValue("chats").jsonArray.map {
                val c = it.jsonObject
                ShareTargets.Chat(c.str("id")!!, c.str("title")!!, c.str("updatedAt")!!, c.str("agentId"), c["sealed"]?.jsonPrimitive?.booleanOrNull == true)
            }
            val got = ShareTargets.directShareEntries(agents(input), chats, input.str("me")) { it }
            val expected = case.getValue("expected").jsonArray.map {
                val e = it.jsonObject
                ShareTargets.Entry(e.str("id")!!, e.str("title")!!)
            }
            assertEquals(case.str("name"), expected, got)
        }
    }

    @Test
    fun `parseCases match the fixture`() {
        for (case in cases("parseCases")) {
            val got = ShareTargets.parseDirectShareId(case.getValue("input").jsonPrimitive.content)
            val expected = case["expected"]?.takeUnless { it is JsonNull }?.jsonObject
            assertEquals(case.str("name"), expected?.str("kind"), got?.kind?.name?.lowercase())
            assertEquals(case.str("name"), expected?.str("id"), got?.id)
        }
    }

    @Test
    fun `issueBoardCases match the fixture`() {
        for (case in cases("issueBoardCases")) {
            val input = case.getValue("input").jsonObject
            val boards = input.getValue("boards").jsonArray.map { it.jsonObject.str("id")!! to it.jsonObject.str("teamId")!! }
            val got = ShareTargets.shareIssueBoards(input.str("teamId"), boards, input.str("lastBoardId"), { it.first }, { it.second })
            val expected = case.getValue("expected").jsonObject
            assertEquals(case.str("name"), strings(expected.getValue("boards")), got.boards.map { it.first })
            assertEquals(case.str("name"), expected.str("board"), got.board?.first)
        }
    }
}
