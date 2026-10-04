package com.exponential.app.ui.components

import com.exponential.app.data.db.CommentEntity
import com.exponential.app.data.db.UserEntity
import com.exponential.app.domain.contractFixtureJson
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * SLOP-4: the comment caption and author name, locked ×4 against
 * `reporter-reply.json` `captions` + `authorNames`.
 */
class CommentCaptionTest {

    private fun fixture(): JsonObject =
        Json.parseToJsonElement(contractFixtureJson("reporter-reply.json")).jsonObject

    private fun JsonObject.stringOrNull(key: String): String? =
        get(key)?.takeIf { it !is JsonNull }?.jsonPrimitive?.content

    private fun comment(source: String, audience: String = "team", authorId: String? = "u1") =
        CommentEntity(
            id = "c1", issueId = "i1", teamId = "t1", authorId = authorId,
            source = source, audience = audience, createdAt = "c", updatedAt = "u",
        )

    @Test
    fun `the caption parts follow the pinned order`() {
        val captions = fixture().getValue("captions").jsonObject
        assertEquals(VIA_MCP_CAPTION, captions.getValue("viaMcp").jsonPrimitive.content)
        assertEquals(CAPTION_SEPARATOR, captions.getValue("separator").jsonPrimitive.content)
        val cases = captions.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val row = case.jsonObject
            val source = row.getValue("source").jsonPrimitive.content
            val audience = row.getValue("audience").jsonPrimitive.content
            assertEquals(
                "$source/$audience",
                row.stringOrNull("caption"),
                commentCaption(comment(source, audience)),
            )
        }
    }

    @Test
    fun `the author name follows the pinned rules`() {
        val cases = fixture().getValue("authorNames").jsonObject.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        val member = UserEntity(id = "u1", name = "Ada Lovelace", email = "ada@x.io", createdAt = "c", updatedAt = "u")
        for (case in cases) {
            val row = case.jsonObject
            val source = row.getValue("source").jsonPrimitive.content
            val authorId = row.stringOrNull("authorId")
            val synced = row.getValue("authorSynced").jsonPrimitive.content.toBoolean()
            val expected = row.getValue("name").jsonPrimitive.content
                .replace("{author}", userDisplayName(member, member.id))
            assertEquals(
                "$source/$authorId",
                expected,
                commentAuthorName(
                    comment(source, authorId = authorId),
                    if (synced) member else null,
                    row.stringOrNull("reporterName"),
                ),
            )
        }
    }
}
