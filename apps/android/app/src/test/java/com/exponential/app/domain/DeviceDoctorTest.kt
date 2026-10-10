package com.exponential.app.domain

import com.exponential.app.data.db.DeviceEntity
import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-1196/1218/1219: the readiness block is ONE spec ×5 — this reads the
// shared `packages/domain-contract/fixtures/device-doctor.json` (web, desktop,
// iOS and the CLI read the same file) and checks the Android row model
// against every case.
class DeviceDoctorTest {

    private val fixture: JsonObject = run {
        val candidates = listOf(
            "../../../packages/domain-contract/fixtures/device-doctor.json",
            "../../packages/domain-contract/fixtures/device-doctor.json",
            "packages/domain-contract/fixtures/device-doctor.json",
        )
        val file = candidates.map(::File).firstOrNull { it.isFile }
            ?: error("device-doctor.json not found from ${File(".").absolutePath}")
        Json.parseToJsonElement(file.readText()).jsonObject
    }

    private val fixtureGroups = fixture["groups"]!!.jsonArray.map { it.jsonObject }
    private val fixtureLabels = fixture["labels"]!!.jsonObject
        .mapValues { it.value.jsonPrimitive.content }
    private val fixtureStates = fixture["states"]!!.jsonObject.mapValues { it.value.jsonObject }
    private val fixtureActions = fixture["actions"]!!.jsonObject.mapValues { it.value.jsonObject }
    private val cases = fixture["cases"]!!.jsonArray.map { it.jsonObject }

    private fun JsonObject.strOrNull(key: String): String? =
        this[key]?.takeIf { it !is JsonNull }?.jsonPrimitive?.content

    private fun doctorOf(case: JsonObject): DeviceDoctor =
        parseDeviceDoctor(case["doctor"].toString())!!

    // ── The vocabulary ───────────────────────────────────────────────────────

    @Test
    fun `groups labels and actions match the fixture`() {
        assertEquals(
            fixtureGroups.map { Triple(it.strOrNull("key")!!, it.strOrNull("label")!!, it.strOrNull("tag")) },
            DeviceReadiness.GROUPS,
        )
        assertEquals(fixtureLabels, DeviceReadiness.LABELS)
        assertEquals(
            fixtureActions.mapValues {
                DeviceReadiness.Action(
                    it.value.strOrNull("label")!!,
                    it.value["remote"]!!.jsonPrimitive.booleanOrNull!!,
                )
            },
            DeviceReadiness.ACTIONS,
        )
    }

    @Test
    fun `every state maps to the fixture glyph and tone`() {
        for ((state, spec) in fixtureStates) {
            val (glyph, tone) = DeviceReadiness.glyphAndTone(state)
            assertEquals(state, spec.strOrNull("glyph"), glyph.name.lowercase())
            assertEquals(state, spec.strOrNull("tone"), tone.name.lowercase())
        }
    }

    // ── Every case ───────────────────────────────────────────────────────────

    @Test
    fun `each case renders its groups in order with their tags`() {
        for (case in cases) {
            val doctor = doctorOf(case)
            val present = doctor.items.map { it.group }.toSet()
            val expected = fixtureGroups.filter { it.strOrNull("key") in present }
            val groups = DeviceReadiness.groups(doctor, remote = false)
            assertEquals(case.strOrNull("name"), expected.map { it.strOrNull("key") }, groups.map { it.key })
            assertEquals(expected.map { it.strOrNull("label") }, groups.map { it.label })
            assertEquals(expected.map { it.strOrNull("tag") }, groups.map { it.tag })
            // The band renders the tag verbatim: lowercase, never "Optional".
            assertTrue(groups.mapNotNull { it.tag }.all { it == "optional" })
        }
    }

    @Test
    fun `each case renders one row per item with label detail glyph and tone`() {
        for (case in cases) {
            val name = case.strOrNull("name")
            val items = case["doctor"]!!.jsonObject["items"]!!.jsonArray.map { it.jsonObject }
            val rows = DeviceReadiness.groups(doctorOf(case), remote = false).flatMap { it.rows }
            // Fixture item order == block order in every case (items arrive grouped).
            assertEquals(name, items.map { it.strOrNull("key") }, rows.map { it.key })
            for ((item, row) in items.zip(rows)) {
                val key = item.strOrNull("key")!!
                val state = item.strOrNull("state")!!
                assertEquals("$name/$key", fixtureLabels[key], row.label)
                if (key == "computer_use") {
                    // The switch row: label + switch, no glyph, no detail.
                    assertTrue(row.isSwitch)
                    assertEquals(DoctorGlyph.None, row.glyph)
                    assertNull(row.detail)
                    assertEquals(state != "off", row.switchOn)
                    assertNull(row.action)
                } else {
                    assertFalse(row.isSwitch)
                    assertEquals("$name/$key", item.strOrNull("detail"), row.detail)
                    assertEquals(fixtureStates[state]!!.strOrNull("glyph"), row.glyph.name.lowercase())
                    assertEquals(fixtureStates[state]!!.strOrNull("tone"), row.tone.name.lowercase())
                    assertEquals(item.strOrNull("parent") != null, row.indented)
                    // On the device itself every action is offered.
                    assertEquals("$name/$key", item.strOrNull("action"), row.action)
                    assertEquals(
                        item.strOrNull("action")?.let { fixtureActions[it]!!.strOrNull("label") },
                        row.actionLabel,
                    )
                }
            }
        }
    }

    @Test
    fun `the primary pill is the first action or error row's action`() {
        for (case in cases) {
            val rows = DeviceReadiness.groups(doctorOf(case), remote = false).flatMap { it.rows }
            val primary = rows.filter { it.primary }
            assertTrue(primary.size <= 1)
            assertEquals(case.strOrNull("name"), case.strOrNull("localPrimary"), primary.firstOrNull()?.key)
        }
    }

    @Test
    fun `a remote device offers only update and sign_in`() {
        for (case in cases) {
            val rows = DeviceReadiness.groups(doctorOf(case), remote = true).flatMap { it.rows }
            val pills = rows.mapNotNull { row -> row.action?.let { row.key to it } }.toMap()
            val expected = case["remotePills"]!!.jsonObject.mapValues { it.value.jsonPrimitive.content }
            assertEquals(case.strOrNull("name"), expected, pills)
            assertTrue(pills.values.all { it == "update" || it == "sign_in" })
        }
    }

    @Test
    fun `import pills ride the agent rows that carry an ambient login`() {
        for (case in cases) {
            val name = case.strOrNull("name")
            val expected = case["importPills"]?.jsonObject
                ?.mapValues { it.value.jsonPrimitive.content }
                .orEmpty()
            fun pills(rows: List<DoctorRow>) =
                rows.mapNotNull { row -> row.importEmail?.let { row.key to it } }.toMap()
            // On the device itself, and on another device declaring `agent-import`.
            val local = DeviceReadiness.groups(doctorOf(case), remote = false).flatMap { it.rows }
            assertEquals(name, expected, pills(local))
            val capable = DeviceReadiness.groups(doctorOf(case), remote = true, canImport = true)
                .flatMap { it.rows }
            assertEquals(name, expected, pills(capable))
            // Another device without the cap: no import pill, the action stays.
            val old = DeviceReadiness.groups(doctorOf(case), remote = true).flatMap { it.rows }
            assertEquals(name, emptyMap<String, String>(), pills(old))
            // The import never takes the primary pill: it is always plain.
            assertEquals(
                name,
                case.strOrNull("localPrimary"),
                local.firstOrNull { it.primary }?.key,
            )
        }
        // The composer's single row carries it too.
        val imported = cases.first { it["importPills"] != null }
        val row = DeviceReadiness.failingRow(doctorOf(imported), "claude", canImport = true)!!
        assertEquals("dev@acme.test", row.importEmail)
        assertEquals("sign_in", row.action)
        assertNull(DeviceReadiness.failingRow(doctorOf(imported), "claude")!!.importEmail)
    }

    @Test
    fun `the import confirm and the duplicate toast are the fixture copy`() {
        val copy = fixture["copy"]!!.jsonObject.mapValues { it.value.jsonPrimitive.content }
        assertEquals(
            copy["importTitle"]!!.replace("{email}", "a@x.test"),
            DeviceReadiness.importTitle("a@x.test"),
        )
        assertEquals(copy["importBody"], DeviceReadiness.IMPORT_BODY)
        assertEquals(
            copy["alreadyAdded"]!!.replace("{email}", "a@x.test"),
            DeviceReadiness.alreadyAdded("a@x.test"),
        )
    }

    @Test
    fun `runnable agents are the ok agent rows`() {
        for (case in cases) {
            val expected = case["runnable"]!!.jsonArray.map { it.jsonPrimitive.content }
            assertEquals(case.strOrNull("name"), expected, DeviceReadiness.runnableAgents(doctorOf(case)))
        }
    }

    // ── The rules ────────────────────────────────────────────────────────────

    @Test
    fun `permission rows hide while computer use is off`() {
        val doctor = doctorOf(cases[1])
        val on = DeviceReadiness.groups(doctor, remote = false)
            .first { it.key == DeviceReadiness.GROUP_COMPUTER_USE }.rows.map { it.key }
        assertEquals(listOf("computer_use", "screen_recording", "accessibility"), on)

        // The device reports the parent off: no children.
        val reportedOff = doctor.copy(
            items = doctor.items.map { if (it.key == "computer_use") it.copy(state = "off") else it },
        )
        assertEquals(
            listOf("computer_use"),
            DeviceReadiness.groups(reportedOff, remote = false)
                .first { it.key == DeviceReadiness.GROUP_COMPUTER_USE }.rows.map { it.key },
        )

        // The local switch turned off ahead of the next heartbeat: same.
        val switchedOff = DeviceReadiness.groups(doctor, remote = false, computerUseOn = false)
            .first { it.key == DeviceReadiness.GROUP_COMPUTER_USE }.rows
        assertEquals(listOf("computer_use"), switchedOff.map { it.key })
        assertFalse(switchedOff.single().switchOn)
    }

    @Test
    fun `the composer shows only the failing row of the picked agent, Git first`() {
        val ready = doctorOf(cases[0])
        assertNull(DeviceReadiness.failingRow(ready, "claude"))
        assertEquals("codex", DeviceReadiness.failingRow(ready, "codex")?.key)
        // EXP-1232: codex is a managed download — its missing row offers the
        // (remote) sign-in, which fetches the build on the device.
        assertEquals("sign_in", DeviceReadiness.failingRow(ready, "codex")?.action)

        val old = DeviceReadiness.failingRow(doctorOf(cases[1]), "claude")!!
        assertEquals("claude", old.key)
        assertEquals("update", old.action)
        assertEquals("Update", old.actionLabel)
        assertTrue(old.primary)

        val noGit = DeviceReadiness.failingRow(doctorOf(cases[2]), "codex")!!
        assertEquals("git", noGit.key)
        assertEquals(DoctorGlyph.X, noGit.glyph)
        assertNull(noGit.action)
    }

    @Test
    fun `the own-device list shows only rows that need attention`() {
        assertEquals(emptyList<String>(), DeviceReadiness.attentionRows(doctorOf(cases[0])).map { it.key })
        assertEquals(
            listOf("claude", "accessibility"),
            DeviceReadiness.attentionRows(doctorOf(cases[1])).map { it.key },
        )
        assertEquals(listOf("git", "claude"), DeviceReadiness.attentionRows(doctorOf(cases[2])).map { it.key })
    }

    // ── Lenient decode ───────────────────────────────────────────────────────

    @Test
    fun `a null or bad column is no doctor`() {
        assertNull(parseDeviceDoctor(null))
        assertNull(parseDeviceDoctor(""))
        assertNull(parseDeviceDoctor("not json"))
        assertNull(parseDeviceDoctor("[1,2]"))
    }

    @Test
    fun `unknown keys and states are kept, malformed items dropped`() {
        val doctor = parseDeviceDoctor(
            """{"checkedAt":"x","future":1,"items":[
                {"key":"git","group":"required","state":"ok","detail":"2.55.0","extra":true},
                {"key":"claude","group":"agents","state":"thinking"},
                {"group":"agents","state":"ok"},
                {"key":"gpu","group":"hardware","state":"ok"}
            ]}""",
        )!!
        assertEquals("x", doctor.checkedAt)
        assertEquals(listOf("git", "claude", "gpu"), doctor.items.map { it.key })
        assertEquals("thinking", doctor.items[1].state)
        val groups = DeviceReadiness.groups(doctor, remote = true)
        // Known groups first, an unknown one after them under its key.
        assertEquals(listOf("required", "agents", "hardware"), groups.map { it.key })
        val claude = groups[1].rows.single()
        assertEquals(DoctorGlyph.Dash, claude.glyph)
        assertEquals(DoctorTone.Muted, claude.tone)
    }

    @Test
    fun `the synced row carries the doctor onto SteerDevice`() {
        val raw = cases[1]["doctor"].toString()
        val entity = DeviceEntity(id = "row", userId = "me", deviceId = "dev", doctor = raw)
        val device = entity.toSteerDevice(nowMs = 0L, currentUserId = "me")
        assertNotNull(device.doctor)
        assertEquals(6, device.doctor!!.items.size)
        assertNull(entity.copy(doctor = null).toSteerDevice(0L, "me").doctor)
    }

    @Test
    fun `the Electric row decodes the jsonb doctor as raw text`() {
        val json = Json { ignoreUnknownKeys = true }
        val row = """{"id":"row","user_id":"me","device_id":"dev","doctor":${cases[2]["doctor"]}}"""
        val entity = json.decodeFromString(DeviceEntity.serializer(), row)
        assertEquals(4, parseDeviceDoctor(entity.doctor)!!.items.size)
        val nullRow = """{"id":"row","user_id":"me","device_id":"dev","doctor":null}"""
        assertNull(json.decodeFromString(DeviceEntity.serializer(), nullRow).doctor)
        assertTrue(cases.all { it["doctor"]!!.jsonObject["items"] is JsonArray })
    }
}
