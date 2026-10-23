package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

// feedback-states.json `starting` (final review): one startId per draft
// start, bound to the device + the draft fingerprint. Recheck R2: the
// fingerprint holds every `starting.fingerprint.fields` entry and replays
// `starting.fingerprint.cases`. iOS mirrors these cases
// (`DraftStartTests.swift`).
class DraftStartTest {

    private val draft = DraftStart.Fingerprint(
        deviceId = "mac",
        text = "fix the login",
        issueIds = setOf("i1"),
        attachments = setOf("a1"),
        agent = "claude",
        model = "opus",
        effort = "high",
        planMode = false,
    )
    private var counter = 0
    private val mint = { "id-${++counter}" }

    @Test
    fun theFirstStartMints() {
        val start = DraftStart.resolve(null, draft, mint)
        assertEquals("id-1", start.startId)
        assertEquals("mac", start.deviceId)
    }

    @Test
    fun theSameDraftOnTheSameDeviceReusesTheId() {
        val first = DraftStart.resolve(null, draft, mint)
        assertSame(first, DraftStart.resolve(first, draft, mint))
    }

    @Test
    fun anotherDeviceMintsAFreshId() {
        val first = DraftStart.resolve(null, draft, mint)
        assertEquals("id-2", DraftStart.resolve(first, draft.copy(deviceId = "linux"), mint).startId)
    }

    @Test
    fun aChangedDraftMintsAFreshId() {
        val first = DraftStart.resolve(null, draft, mint)
        listOf(
            draft.copy(text = "fix it"),
            draft.copy(issueIds = setOf("i1", "i2")),
            draft.copy(issueIds = emptySet()),
            draft.copy(attachments = emptySet()),
            draft.copy(model = "sonnet"),
            draft.copy(account = "profile-2"),
        ).forEach { change ->
            assertNotEquals(first.startId, DraftStart.resolve(first, change, mint).startId)
        }
    }

    @Test
    fun aLandingClearsOnlyAnUnchangedDraft() {
        val start = DraftStart.resolve(null, draft, mint)
        assertTrue(start.landsInto(draft))
        assertFalse(start.landsInto(draft.copy(text = "a new prompt", issueIds = emptySet(), attachments = emptySet())))
        assertFalse(start.landsInto(null))
    }

    // ── feedback-states.json `starting.fingerprint` ─────────────────────────

    private val fingerprintFixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("feedback-states.json")).jsonObject
            .getValue("starting").jsonObject
            .getValue("fingerprint").jsonObject

    private fun fingerprint(raw: JsonObject): DraftStart.Fingerprint {
        fun string(key: String): String? =
            raw[key]?.takeIf { it !is JsonNull }?.jsonPrimitive?.contentOrNull
        fun bool(key: String): Boolean? =
            raw[key]?.takeIf { it !is JsonNull }?.jsonPrimitive?.booleanOrNull
        fun set(key: String): Set<String> =
            raw[key]?.takeIf { it !is JsonNull }?.jsonArray?.map { it.jsonPrimitive.content }?.toSet().orEmpty()
        return DraftStart.Fingerprint(
            deviceId = string("deviceId")!!,
            text = string("text")!!,
            issueIds = set("issueIds"),
            repositoryId = string("repositoryId"),
            attachments = set("attachments"),
            agent = string("agent"),
            model = string("model"),
            effort = string("effort"),
            planMode = bool("planMode"),
            account = string("account"),
            mcpServerIds = set("mcpServerIds"),
            computerUse = bool("computerUse"),
            ultracode = bool("ultracode"),
            subagentModel = string("subagentModel"),
            resume = bool("resume") ?: false,
        )
    }

    @Test
    fun theFingerprintReplaysEveryCase() {
        val cases = fingerprintFixture.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val a = fingerprint(case.getValue("a").jsonObject)
            val b = fingerprint(case.getValue("b").jsonObject)
            val same = case.getValue("same").jsonPrimitive.boolean
            assertEquals(name, same, a == b)
            val first = DraftStart.resolve(null, a, mint)
            assertEquals(name, same, DraftStart.resolve(first, b, mint).startId == first.startId)
        }
    }

    @Test
    fun everyFingerprintFieldIsCovered() {
        val fields = fingerprintFixture.getValue("fields").jsonArray
            .map { it.jsonPrimitive.content.substringBefore(" ") }
            .toSet()
        val declared = DraftStart.Fingerprint::class.java.declaredFields
            .filterNot { java.lang.reflect.Modifier.isStatic(it.modifiers) }
            .map { it.name }
            .toSet()
        assertEquals(fields, declared)
    }
}
