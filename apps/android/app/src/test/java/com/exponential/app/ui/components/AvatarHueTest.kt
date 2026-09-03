package com.exponential.app.ui.components

import com.exponential.app.ui.theme.DesignTokens
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-698 r4 — the avatar hue contract. Four clients hash the same user id
 * into the same palette slot, so a person without a picture keeps ONE colour
 * everywhere. The fixture below is that contract, pinned identically by
 * `apps/web/src/lib/avatar-color.test.ts` and the iOS/desktop twins: changing
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
