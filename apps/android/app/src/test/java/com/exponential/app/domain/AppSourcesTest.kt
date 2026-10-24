package com.exponential.app.domain

import at.exponential.ui.json.JsonValue
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

// VAPP-119: replays `packages/domain-contract/fixtures/app-sources.json`
// (web, desktop and iOS replay the same cases) and locks the constants.
class AppSourcesTest {
    private fun read(relative: String): JsonValue {
        val candidates = listOf("../../../$relative", "../../$relative", relative)
        val file = candidates.map(::File).firstOrNull { it.isFile } ?: error("$relative not found from ${File(".").absolutePath}")
        return JsonValue.parse(file.readText())
    }

    private val fixture = read("packages/domain-contract/fixtures/app-sources.json")

    private fun JsonValue.s(key: String): String = this[key]!!.string!!
    private fun JsonValue.os(key: String): String? = this[key]?.string
    private fun JsonValue.n(key: String): Double = this[key]!!.number!!
    private fun JsonValue.on(key: String): Double? = this[key]?.takeIf { !it.isNull }?.number
    private fun JsonValue.rows(key: String): List<JsonValue> = this[key]!!.array!!

    private val store: AppSources.Store = fixture["store"]!!.let { s ->
        AppSources.Store(
            boards = s.rows("boards").map {
                AppSources.Board(it.s("id"), it.s("slug"), it.s("name"), it.s("prefix"), it.os("icon"), it.os("color"), it.n("sortOrder"), it.os("deletedAt"))
            },
            issues = s.rows("issues").map {
                AppSources.Issue(
                    it.s("id"), it.s("boardId"), it.s("identifier"), it.s("title"), it.s("status"), it.os("priority"),
                    it.os("assigneeId"), it.os("creatorId"), it.os("dueDate"), it.n("sortOrder"), it.s("createdAt"), it.s("updatedAt"),
                )
            },
            members = s.rows("members").map { AppSources.Member(it.s("userId"), it.s("role")) },
            users = s.rows("users").map { AppSources.User(it.s("id"), it.s("name"), it.os("email"), it.os("image")) },
            sessions = s.rows("sessions").map {
                AppSources.Session(
                    it.s("id"), it.os("parentId"), it.os("agentId"), it.os("appId"), it.os("userId"), it.s("status"),
                    it["busy"]!!.bool!!, it["needsInput"]!!.bool!!, it.os("title"), it.s("createdAt"), it.s("updatedAt"),
                )
            },
            comments = s.rows("comments").map {
                AppSources.Comment(it.s("id"), it.s("issueId"), it.os("authorId"), it.os("parentId"), it.s("source"), it.s("body"), it.s("createdAt"))
            },
            attachments = s.rows("attachments").map {
                AppSources.Attachment(
                    it.s("id"), it.os("issueId"), it.os("commentId"), it.os("uploaderId"), it.s("filename"), it.s("contentType"),
                    it.n("sizeBytes").toLong(), it.on("width")?.toInt(), it.on("height")?.toInt(), it.s("createdAt"),
                )
            },
        )
    }

    @Test
    fun `the constants match the fixture`() {
        assertEquals(fixture["sources"]!!.array!!.map { it.string }, AppSources.SOURCES)
        assertEquals(fixture["limits"]!!.n("default").toInt(), AppSources.LIMIT_DEFAULT)
        assertEquals(fixture["limits"]!!.n("max").toInt(), AppSources.LIMIT_MAX)
    }

    @Test
    fun `sourceValue replays the cases`() {
        val all = fixture.rows("cases")
        assert(all.size >= 17)
        for (case in all) {
            val params = case["params"]?.obj?.mapValues { it.value.string!! } ?: emptyMap()
            val got = AppSources.sourceValue(case.s("source"), params, store, case.os("me"))
            val expected = case["expected"]!!
            // `json` sorts object keys: the comparison is key-order-insensitive.
            if (expected.isNull) assertNull(case.s("name"), got) else assertEquals(case.s("name"), expected.json, got!!.json)
        }
    }

    @Test
    fun `an unknown name is null`() {
        assertNull(AppSources.sourceValue("teams", emptyMap(), store, "u1"))
        assertNull(AppSources.sourceValue("records", emptyMap(), store, "u1"))
    }

    @Test
    fun `the limit is capped and defaults`() {
        assertEquals(50, AppSources.limitOf(emptyMap()))
        assertEquals(50, AppSources.limitOf(mapOf("limit" to "0")))
        assertEquals(50, AppSources.limitOf(mapOf("limit" to "1.5")))
        assertEquals(500, AppSources.limitOf(mapOf("limit" to "9000")))
        assertEquals(7, AppSources.limitOf(mapOf("limit" to "7")))
    }
}
