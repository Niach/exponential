package com.exponential.app.ui.session

import org.junit.Assert.assertEquals
import org.junit.Test

// EXP-511: the composed steer message is a cross-client contract — web, iOS and
// Android build the exact same string from the same inputs, and the host's
// reverse rewrite only matches if it stays byte-identical.
class SteerImageMessageTest {

    private val idA = "11111111-1111-4111-8111-111111111111"
    private val idB = "22222222-2222-4222-8222-222222222222"

    @Test
    fun `text and images are joined by a blank line`() {
        assertEquals(
            "fix the header\n\n![image](/api/attachments/$idA)\n![image](/api/attachments/$idB)",
            buildSteerImageMessage("fix the header", listOf(idA, idB)),
        )
    }

    @Test
    fun `blank text yields the embeds alone`() {
        assertEquals(
            "![image](/api/attachments/$idA)",
            buildSteerImageMessage("  \n ", listOf(idA)),
        )
    }

    @Test
    fun `no images leaves the trimmed text unchanged`() {
        assertEquals("hello", buildSteerImageMessage("  hello  ", emptyList()))
    }

    @Test
    fun `empty input stays empty`() {
        assertEquals("", buildSteerImageMessage("", emptyList()))
    }
}
