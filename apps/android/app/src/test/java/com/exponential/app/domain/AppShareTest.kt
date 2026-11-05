package com.exponential.app.domain

import com.exponential.app.data.api.AppToolsDto
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * VAPP-241: public links, locked ×4 against `app-share.json` (TS
 * app-share.ts, desktop domain::app_share, iOS AppShare.swift).
 */
class AppShareTest {

    private fun fixture(): JsonObject =
        Json.parseToJsonElement(contractFixtureJson("app-share.json")).jsonObject

    private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content

    private fun JsonObject.stringOrNull(key: String): String? =
        getValue(key).let { if (it is JsonNull) null else it.jsonPrimitive.content }

    private fun cases(key: String): List<JsonObject> {
        val cases = fixture().getValue(key).jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        return cases
    }

    @Test
    fun `the constants are the contract's`() {
        val f = fixture()
        val caps = f.getValue("caps").jsonObject
        assertEquals(AppShare.PER_IP_PER_HOUR, caps.getValue("perIpPerHour").jsonPrimitive.int)
        assertEquals(AppShare.PER_LINK_PER_HOUR, caps.getValue("perLinkPerHour").jsonPrimitive.int)
        val ids = f.getValue("testIds").jsonObject
        assertEquals(AppShare.TestIds.SHARE_LINKS, ids.string("shareLinks"))
        assertEquals(AppShare.TestIds.SHARE_ROW, ids.string("shareRow"))
        val copy = f.getValue("copy").jsonObject
        assertEquals(AppShare.Copy.TITLE, copy.string("title"))
        assertEquals(AppShare.Copy.SUBMIT, copy.string("submit"))
        assertEquals(AppShare.Copy.VIEW, copy.string("view"))
        assertEquals(AppShare.Copy.OFF, copy.string("off"))
        assertEquals(AppShare.Copy.ON, copy.string("on"))
        assertEquals(AppShare.Copy.EXPIRED, copy.string("expired"))
        assertEquals(AppShare.Copy.ORPHANED, copy.string("orphaned"))
        assertEquals(AppShare.Copy.EXPIRES_ON, copy.string("expiresOn"))
        assertEquals(AppShare.Copy.COPY_LINK, copy.string("copyLink"))
        assertEquals(AppShare.Copy.NEW_LINK, copy.string("newLink"))
        assertEquals(AppShare.Copy.GUEST, copy.string("guest"))
        val choices = f.getValue("expiryChoices").jsonArray.map { it.jsonObject }
        assertEquals(choices.map { it.string("label") }, AppShare.EXPIRY_CHOICES.map { it.label })
        assertEquals(choices.map { it.stringOrNull("days")?.toInt() }, AppShare.EXPIRY_CHOICES.map { it.days })
    }

    @Test
    fun `stateCases replay`() {
        for (case in cases("stateCases")) {
            val link = case.getValue("link").let { if (it is JsonNull) null else it.jsonObject }
                ?.let { AppShare.Link(it.stringOrNull("revokedAt"), it.stringOrNull("expiresAt")) }
            assertEquals(
                case.string("name"),
                case.string("state"),
                linkState(link, case.getValue("declared").jsonPrimitive.boolean, case.string("now")),
            )
        }
    }

    @Test
    fun `capCases replay`() {
        for (case in cases("capCases")) {
            assertEquals(
                case.string("name"),
                case.getValue("allowed").jsonPrimitive.boolean,
                underShareCaps(case.getValue("ipCount").jsonPrimitive.int, case.getValue("linkCount").jsonPrimitive.int),
            )
        }
    }

    @Test
    fun `bylineCases replay`() {
        for (case in cases("bylineCases")) {
            assertEquals(case.string("name"), case.string("text"), shareByline(case.stringOrNull("shareName"), case.string("byline")))
        }
    }

    @Test
    fun `switchCases replay`() {
        for (case in cases("switchCases")) {
            assertEquals(case.string("state"), case["on"]!!.jsonPrimitive.boolean, shareSwitchOn(case.string("state")))
        }
    }

    @Test
    fun `rowCases replay`() {
        for (case in cases("rowCases")) {
            assertEquals(
                case.string("name"),
                case.string("text"),
                shareRowSubtitle(case.string("mode"), case.string("state"), case.stringOrNull("expiresAt")),
            )
        }
    }

    @Test
    fun `historyCases replay`() {
        for (case in cases("historyCases")) {
            val names = case.getValue("names").jsonObject.mapValues { it.value.jsonPrimitive.content }
            val event = AppRecords.RecordEvent(
                op = "create",
                before = null,
                patch = null,
                actor = case.stringOrNull("actor"),
                via = case.stringOrNull("via"),
            )
            assertEquals(case.string("name"), case.string("who"), AppRecords.historyRows(listOf(event), names, "fallback").single().who)
        }
    }

    @Test
    fun `appTools decodes shares, none by default`() {
        val json = Json { ignoreUnknownKeys = true }
        assertTrue(json.decodeFromString(AppToolsDto.serializer(), "{}").shares.isEmpty())
        val share = json.decodeFromString(
            AppToolsDto.serializer(),
            """{"shares":[{"name":"signup","mode":"submit","kind":"lead","state":"on","url":"https://x/s/t","expiresAt":null}]}""",
        ).shares.single()
        assertEquals("signup", share.name)
        assertEquals("submit", share.mode)
        assertEquals("on", share.state)
        assertEquals("https://x/s/t", share.url)
        assertEquals(null, share.expiresAt)
    }
}
