package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * #VAPP-229: "via <app>", locked ×4 against `via-app.json` (TS via-app.ts,
 * desktop domain::via_app, iOS ViaApp.swift).
 */
class ViaAppTest {

    private fun fixture(): JsonObject =
        Json.parseToJsonElement(contractFixtureJson("via-app.json")).jsonObject

    private fun JsonObject.stringOrNull(key: String): String? =
        getValue(key).let { if (it is JsonNull) null else it.jsonPrimitive.content }

    @Test
    fun `the copy is the contract's`() {
        val copy = fixture().getValue("copy").jsonObject
        assertEquals(ViaApp.COPY, copy.getValue("viaApp").jsonPrimitive.content)
        assertEquals(ViaApp.SEPARATOR, copy.getValue("separator").jsonPrimitive.content)
    }

    @Test
    fun `captionCases replay`() {
        val cases = fixture().getValue("captionCases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (case in cases.map { it.jsonObject }) {
            val name = case.getValue("name").jsonPrimitive.content
            assertEquals(name, case.stringOrNull("text"), viaAppCaption(case.stringOrNull("viaAppName")))
        }
    }

    @Test
    fun `appendCases replay`() {
        val cases = fixture().getValue("appendCases").jsonArray
        assertTrue(cases.isNotEmpty())
        for (case in cases.map { it.jsonObject }) {
            val name = case.getValue("name").jsonPrimitive.content
            assertEquals(
                name,
                case.getValue("expected").jsonPrimitive.content,
                withViaApp(case.getValue("text").jsonPrimitive.content, case.stringOrNull("viaAppName")),
            )
        }
    }
}
