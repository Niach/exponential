package com.exponential.app.domain

import com.exponential.app.data.db.DeviceEntity
import com.exponential.app.ui.components.picker.PickerItem
import com.exponential.app.ui.components.picker.PickerRules
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #EXP-2161: [MemberSpace] replays packages/domain-contract/fixtures/
 * member-space.json (`optInCases`, `teamCases`, `deviceCases`, `startCases`,
 * `sectionCases`, the copy) like web, desktop and iOS do. `pinCases` and
 * `rootMarkCases` replay in `InstallTrustFixtureTest`.
 */
class MemberSpaceTest {
    private val fixture = Json.parseToJsonElement(contractFixtureJson("member-space.json")).jsonObject

    private fun cases(key: String): List<JsonObject> = fixture[key]!!.jsonArray.map { it.jsonObject }
    private fun JsonObject.str(key: String) = this[key]!!.jsonPrimitive.content
    private fun JsonObject.strOrNull(key: String) = (this[key] as? JsonNull)?.let { null } ?: this[key]?.jsonPrimitive?.contentOrNull
    private fun JsonObject.bool(key: String) = this[key]!!.jsonPrimitive.boolean
    private fun JsonObject.strings(key: String) = this[key]!!.jsonArray.map { it.jsonPrimitive.content }

    @Test
    fun `the copy and the keys are the fixture's`() {
        val copy = fixture["copy"]!!.jsonObject
        val mine = mapOf(
            "optIn" to MemberSpace.Copy.OPT_IN,
            "optInTitle" to MemberSpace.Copy.OPT_IN_TITLE,
            "optInBody" to MemberSpace.Copy.OPT_IN_BODY,
            "optInConfirm" to MemberSpace.Copy.OPT_IN_CONFIRM,
            "pickTitle" to MemberSpace.Copy.PICK_TITLE,
            "pickBody" to MemberSpace.Copy.PICK_BODY,
            "pickConfirm" to MemberSpace.Copy.PICK_CONFIRM,
            "members" to MemberSpace.Copy.MEMBERS,
            "remove" to MemberSpace.Copy.REMOVE,
            "waiting" to MemberSpace.Copy.WAITING,
            "noDevice" to MemberSpace.Copy.NO_DEVICE,
            "ended" to MemberSpace.Copy.ENDED,
        )
        assertEquals(copy.keys, mine.keys)
        for ((key, text) in mine) assertEquals(key, copy.str(key), text)
        assertEquals(fixture.str("doctorKey"), MemberSpace.DOCTOR_KEY)
        assertEquals(fixture.str("deviceIdPrefix"), MemberSpace.DEVICE_ID_PREFIX)
        assertEquals("Members' Assistants", DeviceReadiness.LABELS[MemberSpace.DOCTOR_KEY])
    }

    @Test
    fun `the confirms are Cancel and the primary verb`() {
        val pick = MemberSpace.pickPrompt("unraid", "Jonas")
        assertEquals("Run your Assistant on unraid?", pick.title)
        assertTrue(pick.body!!.startsWith("Jonas and admins"))
        assertEquals(listOf("Cancel", "Run here"), pick.actions.map { it.label })
        assertEquals(Prompts.Role.Primary, pick.actions.last().role)
        assertEquals(MemberSpace.ACTION_PICK, pick.focus)
        val optIn = MemberSpace.optInPrompt()
        assertEquals(listOf("Cancel", "Allow"), optIn.actions.map { it.label })
        assertEquals(MemberSpace.Copy.OPT_IN_TITLE, optIn.title)
    }

    @Test
    fun `optInCases`() {
        for (c in cases("optInCases")) {
            assertEquals(
                c.str("name"),
                c.bool("expected"),
                MemberSpace.mayOptIn(c.str("caller"), c.bool("allowed"), c.str("kind"), c.bool("shared"), c.strOrNull("doctor")),
            )
        }
    }

    @Test
    fun `the opt-in row is disabled with the doctor detail while it cannot, never when on`() {
        fun doctor(state: String, detail: String? = null) =
            DeviceDoctor(items = listOf(DeviceDoctorItem(key = MemberSpace.DOCTOR_KEY, group = "storage", state = state, detail = detail)))
        assertEquals(MemberSpace.OptInRow(false, null), MemberSpace.optInRow(false, "server", doctor("ok")))
        assertEquals(MemberSpace.OptInRow(true, "No user namespaces"), MemberSpace.optInRow(false, "server", doctor("missing", "No user namespaces")))
        assertEquals(MemberSpace.OptInRow(true, DeviceReadiness.UNAVAILABLE), MemberSpace.optInRow(false, "server", null))
        assertEquals(MemberSpace.OptInRow(true, DeviceReadiness.UNAVAILABLE), MemberSpace.optInRow(false, "desktop", doctor("ok")))
        assertEquals(false, MemberSpace.optInRow(true, "server", doctor("missing")).disabled)
    }

    @Test
    fun `teamCases`() {
        for (c in cases("teamCases")) {
            assertEquals(
                c.str("name"),
                c.strings("expected"),
                MemberSpace.spaceTeams(c.strings("assistantTeamIds"), c.strings("hostOwnerTeamIds"), c.strings("memberTeamIds")),
            )
        }
    }

    @Test
    fun `deviceCases and startCases`() {
        for (c in cases("deviceCases")) {
            assertEquals(c.str("name"), c.bool("expected"), MemberSpace.spaceMayUseDevice(c.str("device"), c.bool("ownTree")))
        }
        for (c in cases("startCases")) {
            // A child runs where its root does: the rule reads the root.
            assertEquals(c.str("name"), c.bool("expected"), MemberSpace.maySpaceRun(c.str("tree"), c.bool("rootDeviceIsSpace")))
        }
    }

    @Test
    fun `sectionCases`() {
        val all = cases("sectionCases")
        assertTrue(all.size >= 3)
        for (c in all) {
            val devices = c["devices"]!!.jsonArray.map { it.jsonObject }.map { d ->
                MemberSpace.Device(
                    rowId = d.str("rowId"),
                    deviceId = d.str("deviceId"),
                    userId = d.str("userId"),
                    label = d.str("label"),
                    online = d.bool("online"),
                    assistantTeamIds = d.strings("assistantTeamIds"),
                    spaceHostId = d.strOrNull("spaceHostId"),
                )
            }
            val teams = c["teams"]!!.jsonArray.map { it.jsonObject }.map { MemberSpace.Team(it.str("id"), it.str("name")) }
            val expected = (c["expected"] as JsonArray).map { it.jsonObject }.map { s ->
                MemberSpace.Section(
                    s.str("teamId"),
                    s.str("title"),
                    s["rows"]!!.jsonArray.map { it.jsonObject }.map { r ->
                        MemberSpace.SectionRow(r.str("id"), r.str("label"), r.bool("disabled"), r.bool("selected"))
                    },
                )
            }
            assertEquals(c.str("name"), expected, MemberSpace.spaceSections(devices, teams, c.strOrNull("boundId"), c.str("me")))
        }
    }

    private fun row(id: String, deviceId: String, userId: String, label: String, teams: List<String> = emptyList(), host: String? = null, spaceTeam: String? = null) =
        DeviceEntity(
            id = id,
            userId = userId,
            deviceId = deviceId,
            label = label,
            kind = "server",
            lastSeenAt = "2099-01-01T00:00:00Z",
            assistantTeamIds = teams,
            spaceHostId = host,
            spaceTeamId = spaceTeam,
        )

    @Test
    fun `a host under two teams is one pick per team, the bound one checked in both, spaces never listed`() {
        val rows = listOf(
            row("r1", "srv", "u2", "unraid", teams = listOf("t1", "t2")),
            row("r4", "space-1", "u1", "unraid", host = "r1", spaceTeam = "t2"),
            row("r0", "mac", "u1", "mac"),
        )
        val teams = listOf(MemberSpace.Team("t1", "Acme"), MemberSpace.Team("t2", "Beta"))
        val picks = MemberSpace.picks(rows, teams, "space-1", "u1", System.currentTimeMillis())
        assertEquals(listOf("t1:srv", "t2:srv"), picks.picks.map { it.value })
        assertEquals(listOf("Acme", "Beta"), picks.picks.map { it.section })
        assertTrue(picks.picks.all { it.selected && it.online && it.ownerId == "u2" })
        // The space's own team names the pick.
        assertEquals("t2:srv", picks.value)
        assertEquals(listOf("r1", "r0"), rows.listed().map { it.id })
    }

    @Test
    fun `the picker draws plain rows first, then each section under its heading`() {
        val items = listOf(
            PickerItem("t1:s", "unraid", section = "Acme"),
            PickerItem("mac", "Mac"),
            PickerItem("t2:s", "unraid", section = "Beta"),
            PickerItem("t1:b", "build", section = "Acme"),
        )
        val (ordered, headings) = PickerRules.sectioned(items)
        assertEquals(listOf("mac", "t1:s", "t1:b", "t2:s"), ordered.map { it.value })
        assertEquals(mapOf(1 to "Acme", 3 to "Beta"), headings)
        val plain = listOf(PickerItem("a", "A"), PickerItem("b", "B"))
        assertEquals(plain to emptyMap<Int, String>(), PickerRules.sectioned(plain))
    }
}
