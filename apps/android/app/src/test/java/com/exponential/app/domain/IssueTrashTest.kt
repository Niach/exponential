package com.exponential.app.domain

import java.time.Instant
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/** #EXP-1797: the issue trash, replayed from `issue-lifecycle.json` `trash` (web, desktop, iOS replay it too). */
class IssueTrashTest {
    private val trash: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("issue-lifecycle.json")).jsonObject.getValue("trash").jsonObject

    private fun cases(key: String) = trash.getValue(key).jsonArray.map { it.jsonObject }

    private fun JsonObject.str(key: String): String? =
        get(key)?.takeIf { it != JsonNull }?.jsonPrimitive?.content

    @Test
    fun `copy and retention are the fixture's`() {
        val copy = trash.getValue("copy").jsonObject
        assertEquals(copy.str("band"), IssueTrash.BAND)
        assertEquals(copy.str("row"), IssueTrash.ROW)
        assertEquals(copy.str("restore"), IssueTrash.RESTORE)
        assertEquals(copy.str("purge"), IssueTrash.PURGE)
        assertEquals(trash.getValue("retentionDays").jsonPrimitive.content.toLong(), IssueTrash.RETENTION_DAYS)
        assertEquals(trash.getValue("access").jsonObject.str("purge"), DomainContract.teamRoleOwner)
    }

    @Test
    fun `caption cases`() {
        for (case in cases("captionCases")) {
            val input = case.getValue("input").jsonObject
            assertEquals(
                case.str("name"),
                case.str("expected"),
                IssueTrash.caption(input.str("purgeAt"), Instant.parse(input.str("now"))),
            )
        }
    }

    @Test
    fun `action cases`() {
        for (case in cases("actionCases")) {
            val role = case.getValue("input").jsonObject.str("role")!!
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonArray.map { it.jsonPrimitive.content },
                IssueTrash.actions(role).map { it.key },
            )
        }
    }

    @Test
    fun `purge due cases`() {
        for (case in cases("purgeDueCases")) {
            val input = case.getValue("input").jsonObject
            assertEquals(
                case.str("name"),
                case.getValue("expected").jsonPrimitive.boolean,
                IssueTrash.isPurgeDue(input.str("deletedAt"), Instant.parse(input.str("now"))),
            )
        }
    }

    @Test
    fun `row text fills the identifier and title`() {
        assertEquals("EXP-12 Fix the login", IssueTrash.rowText("EXP-12", "Fix the login"))
    }
}
