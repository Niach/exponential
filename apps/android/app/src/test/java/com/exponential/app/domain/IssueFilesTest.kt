package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * NEW-83: the issue's Files list, locked ×4 against
 * `domain-contract/fixtures/issue-files.json` — same cases, same names as the
 * web, desktop and iOS. Each case's `name` is the assertion message.
 */
class IssueFilesTest {

    private data class Row(val id: String, val contentType: String?, val asFile: Boolean, val commentId: String?)

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("issue-files.json")).jsonObject

    @Test
    fun `issue file rows`() {
        val cases = fixture.getValue("cases").jsonArray.map { it.jsonObject }
        check(cases.isNotEmpty())
        for (case in cases) {
            val name = case.getValue("name").jsonPrimitive.content
            val texts = case.getValue("texts").jsonArray.map { if (it is JsonNull) null else it.jsonPrimitive.content }
            val rows = case.getValue("rows").jsonArray.map { it.jsonObject }.map { row ->
                Row(
                    id = row.getValue("id").jsonPrimitive.content,
                    contentType = row["contentType"]?.let { if (it is JsonNull) null else it.jsonPrimitive.content },
                    asFile = row.getValue("asFile").jsonPrimitive.boolean,
                    commentId = row["commentId"]?.let { if (it is JsonNull) null else it.jsonPrimitive.content },
                )
            }
            val files = issueFileRows(
                rows = rows,
                texts = texts,
                id = { it.id },
                contentType = { it.contentType },
                asFile = { it.asFile },
                commentId = { it.commentId },
            ).map { it.id }
            assertEquals(name, case.getValue("files").jsonArray.map { it.jsonPrimitive.content }, files)
        }
    }
}
