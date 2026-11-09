package com.exponential.app.ui.components

import com.exponential.app.ui.theme.DesignTokens
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File

/**
 * EXP-698 r4 — the avatar hue contract. Four clients hash the same user id
 * into the same palette slot, so a person without a picture keeps ONE colour
 * everywhere. The fixture below is that contract, pinned identically by
 * `packages/ui/src/avatar-color.test.ts` and the iOS/desktop twins: changing
 * a pair here recolours existing users on this client only.
 */
class AvatarHueTest {

    private val fixture = listOf(
        "" to 5,
        "demo-mira" to 2,
        "demo-jonas" to 4,
        "demo-sofia" to 1,
        "alex" to 5,
        "7c9e6679-7425-40de-944b-e07fc1f90ae7" to 3,
        "user_01HZY" to 1,
        "ünïcödé" to 2,
    )

    @Test
    fun matchesTheCrossClientFixture() {
        fixture.forEach { (id, index) -> assertEquals(id, index, avatarHueIndex(id)) }
    }

    /** No id at all is the empty id — the same slot, never a crash. */
    @Test
    fun nullIsTheEmptyId() {
        assertEquals(avatarHueIndex(""), avatarHueIndex(null))
    }

    /**
     * #EXP-1754: `packages/exponential-ui/fixtures/avatar.json` `keys`: the key
     * is the id, else the name, else the email (trimmed; blank = absent),
     * null = grey; the key's hue is the pinned slot (iOS `AvatarColorTests`).
     */
    @Test
    fun replaysTheFixtureKeys() {
        val text = listOf(
            "../../../packages/exponential-ui/fixtures/avatar.json",
            "../../packages/exponential-ui/fixtures/avatar.json",
            "packages/exponential-ui/fixtures/avatar.json",
        ).map(::File).first { it.isFile }.readText()
        val keys = Json.parseToJsonElement(text).jsonObject.getValue("keys").jsonArray.map { it.jsonObject }
        assertTrue(keys.isNotEmpty())
        for (case in keys) {
            val name = case.getValue("case").jsonPrimitive.content
            fun field(k: String) = case[k]?.takeIf { it !is JsonNull }?.jsonPrimitive?.content
            val key = avatarHueKey(field("id"), field("name"), field("email"))
            assertEquals(name, field("key"), key)
            val hue = case["hue"]?.takeIf { it !is JsonNull }?.jsonPrimitive?.int
            assertEquals(name, hue, key?.let(::avatarHueIndex))
        }
    }

    /** The palette is eight hues wide, and the index never leaves it. */
    @Test
    fun staysInsideThePalette() {
        assertEquals(8, DesignTokens.Avatar.Hues.size)
        repeat(500) { i ->
            val index = avatarHueIndex("user-$i")
            assertTrue("index $index out of range", index in DesignTokens.Avatar.Hues.indices)
        }
    }
}
