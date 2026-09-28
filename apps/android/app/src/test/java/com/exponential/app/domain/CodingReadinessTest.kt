package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-1121: the readiness model + its copy, fixture-locked ×4 (web
 * `coding-readiness.test.ts`, iOS `CodingReadinessTests`, desktop
 * `domain::coding_readiness`) against the ONE contract fixture — same copy
 * table, same ago cases, same named cases (each case's `name` is the assertion
 * message here). Change one, change all four.
 */
class CodingReadinessTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("coding-readiness.json")).jsonObject

    private fun JsonElement?.stringOrNull(): String? =
        if (this == null || this is JsonNull) null else jsonPrimitive.content

    private fun input(obj: JsonObject) = CodingReadiness.Input(
        isMember = obj.getValue("isMember").jsonPrimitive.boolean,
        remoteStartEnabled = obj["remoteStartEnabled"]
            ?.takeUnless { it is JsonNull }?.jsonPrimitive?.boolean,
        teamName = obj.getValue("teamName").jsonPrimitive.content,
        boardName = obj.getValue("boardName").jsonPrimitive.content,
        boardRepository = obj["boardRepository"].stringOrNull(),
        github = obj["github"]?.takeUnless { it is JsonNull }?.jsonObject?.let {
            CodingReadiness.Github(
                connected = it.getValue("connected").jsonPrimitive.boolean,
                label = it["label"].stringOrNull(),
            )
        },
        devices = obj["devices"]?.takeUnless { it is JsonNull }?.jsonArray?.map { device ->
            val d = device.jsonObject
            CodingReadiness.Device(
                label = d.getValue("label").jsonPrimitive.content,
                own = d.getValue("own").jsonPrimitive.boolean,
                online = d.getValue("online").jsonPrimitive.boolean,
                lastSeenAtMs = d["lastSeenAtMs"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.long,
            )
        },
        nowMs = obj.getValue("nowMs").jsonPrimitive.long,
    )

    private fun expected(obj: JsonObject) = CodingReadiness.Readiness(
        visible = obj.getValue("visible").jsonPrimitive.boolean,
        loading = obj.getValue("loading").jsonPrimitive.boolean,
        ready = obj.getValue("ready").jsonPrimitive.boolean,
        metCount = obj.getValue("metCount").jsonPrimitive.int,
        total = obj.getValue("total").jsonPrimitive.int,
        steps = obj.getValue("steps").jsonArray.map { step ->
            val s = step.jsonObject
            CodingReadiness.Step(
                key = CodingReadiness.StepKey.entries
                    .single { it.wire == s.getValue("key").jsonPrimitive.content },
                state = CodingReadiness.StepState.entries
                    .single { it.wire == s.getValue("state").jsonPrimitive.content },
                title = s.getValue("title").jsonPrimitive.content,
                body = s["body"].stringOrNull(),
                detail = s["detail"].stringOrNull(),
                fixes = s.getValue("fixes").jsonArray.map { fix ->
                    CodingReadiness.Fix.entries.single { it.wire == fix.jsonPrimitive.content }
                },
            )
        },
        summary = obj.getValue("summary").jsonPrimitive.content,
        caption = obj["caption"].stringOrNull(),
    )

    @Test
    fun locksTheCopyTable() {
        val derived = CodingReadiness.Copy.table + mapOf(
            "repositoryTitle(App)" to CodingReadiness.repositoryTitle("App"),
            "repositoryBody(App)" to CodingReadiness.repositoryBody("App"),
            "deviceBody(Acme)" to CodingReadiness.deviceBody("Acme"),
            "lastSeen(MacBook Pro, 2 h ago)" to CodingReadiness.lastSeen("MacBook Pro", "2 h ago"),
            "pickerUsedBy(Website)" to CodingReadiness.pickerUsedBy("Website"),
            "summary(0,3)" to CodingReadiness.summary(0, 3),
            "summary(2,3)" to CodingReadiness.summary(2, 3),
            "summary(3,3)" to CodingReadiness.summary(3, 3),
        )
        val copy = fixture.getValue("copy").jsonObject
            .mapValues { (_, value) -> value.jsonPrimitive.content }
        assertEquals(copy, derived)
    }

    @Test
    fun ago() {
        val cases = fixture.getValue("ago").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val c = element.jsonObject
            val expected = c.getValue("expected").jsonPrimitive.content
            assertEquals(
                "ago $expected",
                expected,
                CodingReadiness.ago(
                    c.getValue("nowMs").jsonPrimitive.long,
                    c.getValue("thenMs").jsonPrimitive.long,
                ),
            )
        }
    }

    @Test
    fun fixtureCases() {
        val cases = fixture.getValue("cases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (element in cases) {
            val c = element.jsonObject
            assertEquals(
                c.getValue("name").jsonPrimitive.content,
                expected(c.getValue("expected").jsonObject),
                CodingReadiness.derive(input(c.getValue("input").jsonObject)),
            )
        }
    }
}
