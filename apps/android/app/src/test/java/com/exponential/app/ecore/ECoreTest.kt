package com.exponential.app.ecore

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.longOrNull
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * #EXP-1504: packages/ecore/fixtures/ecore.json, replayed against the
 * generated [ECore] (Rust tests/fixtures.rs, TS ecore.test.ts and Swift
 * ECoreTests replay the same file).
 */
class ECoreTest {
    private val fixture: JsonObject = run {
        val path = System.getProperty("exponential.ecore.fixture")
            ?: error("exponential.ecore.fixture is not set (app/build.gradle.kts)")
        Json.parseToJsonElement(File(path).readText()).jsonObject
    }

    private fun cases(key: String) = fixture.getValue(key).jsonArray.map { it.jsonObject }
    private fun JsonObject.str(key: String) = getValue(key).jsonPrimitive.content
    private fun JsonObject.bool(key: String) = getValue(key).jsonPrimitive.boolean

    @Test
    fun ids() {
        for (c in cases("idCases")) assertEquals(c.str("id"), c.bool("valid"), ECore.isValidId(c.str("id")))
    }

    @Test
    fun tokensAndBearer() {
        for (c in cases("tokenCases")) assertEquals(c.str("token"), c.bool("valid"), ECore.isValidToken(c.str("token")))
        for (c in cases("bearerCases")) {
            assertEquals(c.str("header"), c.getValue("token").jsonPrimitive.contentOrNull, ECore.parseBearer(c.str("header")))
        }
        val b = fixture.getValue("bearerHeader").jsonObject
        assertEquals(b.str("header"), ECore.bearerHeader(b.str("token")))
    }

    @Test
    fun capabilitiesAndGrants() {
        for (c in cases("capabilityCases")) {
            assertEquals(c.str("name"), c.bool("known"), ECore.Capability.parse(c.str("name")) != null)
        }
        for (c in cases("grantCases")) {
            val capabilities = c.getValue("capabilities").jsonArray.map { ECore.Capability.parse(it.jsonPrimitive.content)!! }
            val admits = ECore.grantAdmits(
                capabilities,
                c.getValue("expiresAt").jsonPrimitive.longOrNull,
                c.getValue("now").jsonPrimitive.long,
                ECore.Capability.parse(c.str("capability"))!!,
            )
            assertEquals(c.toString(), c.bool("admits"), admits)
        }
    }

    @Test
    fun verdictsAndErrors() {
        assertEquals(cases("verdictCases").map { it.str("verdict") }, ECore.DeliveryVerdict.entries.map { it.wire })
        for (c in cases("verdictCases")) {
            assertEquals(c.bool("mayRetry"), ECore.mayRetry(ECore.DeliveryVerdict.parse(c.str("verdict"))!!))
        }
        assertEquals(cases("errorCases").map { it.str("code") }, ECore.ErrorCode.entries.map { it.wire })
        for (c in cases("errorCases")) {
            val code = ECore.ErrorCode.parse(c.str("code"))!!
            assertEquals(c.bool("refusal"), ECore.isRefusal(code))
            assertEquals(c.bool("retryable"), ECore.defaultRetryable(code))
        }
        for (c in fixture.getValue("errorMessage").jsonObject.getValue("cases").jsonArray.map { it.jsonObject }) {
            if ("message" in c) {
                assertEquals(c.str("clamped"), ECore.clampMessage(c.str("message")))
            } else {
                val clamped = ECore.clampMessage(c.str("repeat").repeat(c.getValue("times").jsonPrimitive.int))
                assertEquals(c.getValue("clampedLength").jsonPrimitive.int, clamped.codePointCount(0, clamped.length))
            }
        }
    }

    @Test
    fun envelopes() {
        for (c in cases("envelopeCases")) {
            val e = c.getValue("envelope").jsonObject
            val sessionId = e.getValue("sessionId").let { if (it is JsonNull) null else it.jsonPrimitive.content }
            val errors = ECore.envelopeErrors(
                e.getValue("v").jsonPrimitive.int,
                e.str("id"),
                e.str("kind"),
                e.getValue("at").jsonPrimitive.long,
                e.str("source"),
                sessionId,
            )
            assertEquals(e.toString(), c.getValue("errors").jsonArray.map { it.jsonPrimitive.content }, errors)
        }
    }
}
