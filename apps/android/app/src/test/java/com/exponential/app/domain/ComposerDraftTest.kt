package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #FEED-219: every composer's synced draft, replayed against
 * `composer-draft.json` (`keys`, `unparsable`, `writes`, `fields`,
 * `debounceMs`) like web `composer-draft.test.ts`.
 */
class ComposerDraftTest {

    private val fixture: JsonObject =
        Json.parseToJsonElement(contractFixtureJson("composer-draft.json")).jsonObject

    private fun JsonObject.str(name: String): String = getValue(name).jsonPrimitive.content
    private fun JsonObject.strOrNull(name: String): String? = (get(name) as? JsonPrimitive)?.contentOrNull

    @Test
    fun `the debounce is the contract's`() {
        assertEquals(fixture.getValue("debounceMs").jsonPrimitive.long, ComposerDraft.DEBOUNCE_MS)
    }

    @Test
    fun `every key case builds and parses`() {
        for (case in fixture.getValue("keys").jsonArray.map { it.jsonObject }) {
            val name = case.str("name")
            val target = case.getValue("target").jsonObject
            val kind = ComposerDraft.Kind.fromWire(target.str("kind"))!!
            val built = ComposerDraft.Target(kind, target.str("id"))
            assertEquals(name, case.str("key"), ComposerDraft.key(built))
            assertEquals(name, built, ComposerDraft.parseKey(case.str("key")))
        }
    }

    @Test
    fun `every unparsable key is null`() {
        for (key in fixture.getValue("unparsable").jsonArray.map { it.jsonPrimitive.content }) {
            assertNull(key, ComposerDraft.parseKey(key))
        }
    }

    @Test
    fun `every write case matches`() {
        for (case in fixture.getValue("writes").jsonArray.map { it.jsonObject }) {
            val expected = case.getValue("expected").jsonObject
            val want = when (expected.str("op")) {
                "upsert" -> ComposerDraft.Write.Upsert(expected.str("text"))
                else -> ComposerDraft.Write.Delete
            }
            assertEquals(case.str("name"), want, ComposerDraft.write(case.str("text")))
        }
    }

    @Test
    fun `every field case matches`() {
        for (case in fixture.getValue("fields").jsonArray.map { it.jsonObject }) {
            val input = case.getValue("input").jsonObject
            assertEquals(
                case.str("name"),
                case.str("expected"),
                ComposerDraft.field(
                    field = input.str("field"),
                    edited = input.getValue("edited").jsonPrimitive.boolean,
                    local = input.strOrNull("local"),
                    synced = input.strOrNull("synced"),
                ),
            )
        }
    }
}
