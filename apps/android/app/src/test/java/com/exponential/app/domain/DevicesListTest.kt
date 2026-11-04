package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** A-16 / EXP-1560: [DevicesList] replays `devices-list.json` (copy, card,
 *  every case, accountIcons, accountPanelCases). */
class DevicesListTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("devices-list.json")).jsonObject

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeIf { it !is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.bool(key: String): Boolean = getValue(key).jsonPrimitive.boolean

    @Test
    fun copyMatchesTheFixture() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.keys - "status", DevicesList.Copy.all.keys)
        for ((key, value) in DevicesList.Copy.all) {
            assertEquals(key, copy.getValue(key).jsonPrimitive.content, value)
        }
        val status = copy.getValue("status").jsonObject
        assertEquals(status.keys, DevicesList.Copy.STATUS.keys.map { it.wire }.toSet())
        for ((key, value) in DevicesList.Copy.STATUS) {
            assertEquals(key.wire, status.getValue(key.wire).jsonPrimitive.content, value)
        }
    }

    @Test
    fun cardGeometryMatchesTheFixture() {
        val card = fixture.getValue("card").jsonObject
        fun int(key: String) = card.getValue(key).jsonPrimitive.content.toInt()
        assertEquals(8, card.size)
        assertEquals(int("minWidth"), DevicesList.Card.MIN_WIDTH)
        assertEquals(int("gap"), DevicesList.Card.GAP)
        assertEquals(int("padding"), DevicesList.Card.PADDING)
        assertEquals(int("iconSize"), DevicesList.Card.ICON_SIZE)
        assertEquals(int("markSize"), DevicesList.Card.MARK_SIZE)
        assertEquals(int("maxAccountIcons"), DevicesList.Card.MAX_ACCOUNT_ICONS)
        assertEquals(int("deviceIconSize"), DevicesList.Card.DEVICE_ICON_SIZE)
        assertEquals(int("panelGap"), DevicesList.Card.PANEL_GAP)
        // EXP-1626: no renewal reminder, so no reminder window.
        assertFalse(fixture.containsKey("keepSignedIn"))
        // A 390dp phone (minus the page's 16 + 16) = 1 column; a wide one = 4.
        assertEquals(1, DevicesList.columns(358))
        assertEquals(4, DevicesList.columns(1100))
        assertEquals(1, DevicesList.columns(0))
    }

    @Test
    fun everyAccountIconsCaseReplays() {
        val cases = fixture.getValue("accountIcons").jsonArray
        assert(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val input = case.getValue("input").jsonObject
            val expect = case.getValue("expect").jsonObject
            val got = DevicesList.accountIcons(
                count = input.getValue("count").jsonPrimitive.content.toInt(),
                expanded = input.bool("expanded"),
            )
            assertEquals(name, expect.getValue("visible").jsonPrimitive.content.toInt(), got.visible)
            assertEquals(name, expect.bool("more"), got.more)
        }
    }

    @Test
    fun everyAccountPanelCaseReplays() {
        val cases = fixture.getValue("accountPanelCases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val remembered = when {
                "remembered" !in case -> DevicesList.RememberedAccount.Untouched
                case.getValue("remembered") is JsonNull -> DevicesList.RememberedAccount.Closed
                else -> DevicesList.RememberedAccount.Login(case.str("remembered")!!)
            }
            assertEquals(
                case.str("name"),
                case.str("expect"),
                DevicesList.openDeviceAccount(
                    case.getValue("loginKeys").jsonArray.map { it.jsonPrimitive.content },
                    remembered,
                ),
            )
        }
    }

    @Test
    fun everyCaseReplays() {
        val cases = fixture.getValue("cases").jsonArray
        assert(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val input = case.getValue("input").jsonObject
            val expect = case.getValue("expect").jsonObject
            val row = DevicesList.row(
                DevicesList.Input(
                    online = input.bool("online"),
                    mine = input.bool("mine"),
                    server = input.bool("server"),
                    registered = input.bool("registered"),
                    outdated = input.bool("outdated"),
                    latest = input.str("latest"),
                    updateQueued = input.bool("updateQueued"),
                    accountsReported = input.bool("accountsReported"),
                    logins = input.getValue("logins").jsonArray.map {
                        val login = it.jsonObject
                        DevicesList.DeviceListLogin(
                            agent = login.str("agent")!!,
                            profileId = login.str("profileId")!!,
                            label = login.str("label")!!,
                            health = AgentHealthRules.parse(login.str("health")),
                            limitHit = login["limitHit"]?.jsonPrimitive?.boolean ?: false,
                            tokenExpiresAt = login.str("tokenExpiresAt"),
                        )
                    },
                    now = input.str("now")!!,
                ),
            )
            assertEquals(name, expect.str("group"), row.group.wire)
            assertEquals(name, expect.bool("noAccount"), row.noAccount)
            val want = expect.getValue("attention").jsonArray.map { it.jsonObject }
            assertEquals(name, want.size, row.attention.size)
            want.zip(row.attention).forEach { (w, got) ->
                assertEquals(name, w.str("kind"), got.kind.wire)
                assertEquals(name, w.str("text"), got.text)
                assertEquals(name, w.str("action"), got.action?.wire)
                assertEquals(name, w.str("agent"), got.agent)
                assertEquals(name, w.str("profileId"), got.profileId)
                assertEquals(name, w.str("status"), got.status?.wire)
            }
        }
    }
}
