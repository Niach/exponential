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

/** A-16: [DevicesList] replays `devices-list.json` (copy + every case). */
class DevicesListTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("devices-list.json")).jsonObject

    private fun JsonObject.str(key: String): String? =
        this[key]?.takeIf { it !is JsonNull }?.jsonPrimitive?.content

    private fun JsonObject.bool(key: String): Boolean = getValue(key).jsonPrimitive.boolean

    @Test
    fun copyMatchesTheFixture() {
        val copy = fixture.getValue("copy").jsonObject
        assertEquals(copy.keys, DevicesList.Copy.all.keys)
        for ((key, value) in DevicesList.Copy.all) {
            assertEquals(key, copy.getValue(key).jsonPrimitive.content, value)
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
                        )
                    },
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
            }
        }
    }
}
