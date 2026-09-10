package com.exponential.app.domain

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-825: the composer's `prompt` on the wire — the steer embed format with
 * the images below the prose (byte-identical ×4), NULL when there is nothing
 * to say (so the key is omitted and the server sees no prompt), the contract
 * cap, and the ×4 submit labels.
 */
class AgentComposerPromptTest {

    @Test
    fun `blank text without images is no prompt at all`() {
        assertNull(AgentComposerPrompt.build("", emptyList()))
        assertNull(AgentComposerPrompt.build("  \n\t ", emptyList()))
    }

    @Test
    fun `text is trimmed and images ride as embed lines`() {
        assertEquals("Fix the parser", AgentComposerPrompt.build("  Fix the parser \n", emptyList()))
        assertEquals(
            "Crop [Image #1]\n\n![image](/api/attachments/a-1)\n![image](/api/attachments/a-2)",
            AgentComposerPrompt.build("Crop [Image #1]", listOf("a-1", "a-2")),
        )
        // Images alone are a prompt: the embed block with no prose.
        assertEquals(
            "![image](/api/attachments/a-1)",
            AgentComposerPrompt.build("   ", listOf("a-1")),
        )
    }

    @Test
    fun `the cap is the contract's`() {
        assertEquals(DomainContract.startPromptMaxLength, AgentComposerPrompt.MAX_LENGTH)
        assertEquals(DomainContract.startPromptMaxImages, AgentComposerPrompt.MAX_IMAGES)
        assertEquals(MAX_STEER_IMAGES, AgentComposerPrompt.MAX_IMAGES)
        assertTrue(AgentComposerPrompt.withinLimit("x".repeat(AgentComposerPrompt.MAX_LENGTH)))
        assertFalse(AgentComposerPrompt.withinLimit("x".repeat(AgentComposerPrompt.MAX_LENGTH + 1)))
    }

    // The server caps the COMPOSED prompt (`startPromptSchema` covers the
    // whole string): a draft that fits alone can overflow once its embed
    // lines ride along, and the gate must measure what is actually sent.
    @Test
    fun `the cap measures the draft with its embed lines`() {
        val embedLine = "\n\n![image](/api/attachments/00000000-0000-0000-0000-000000000000)"
        val fits = "x".repeat(AgentComposerPrompt.MAX_LENGTH - embedLine.length)
        assertTrue(AgentComposerPrompt.withinLimit(fits, imageCount = 1))
        assertFalse(AgentComposerPrompt.withinLimit(fits + "x", imageCount = 1))
        // The stand-in is UUID-width, so the pre-upload measure equals the
        // composed length with real ids.
        val realId = "3f9d2a1c-1111-4aaa-8bbb-000000000001"
        assertEquals(
            AgentComposerPrompt.build(fits, listOf(realId))!!.length,
            AgentComposerPrompt.MAX_LENGTH,
        )
        // Images alone count too; surrounding whitespace is trimmed first.
        assertTrue(AgentComposerPrompt.withinLimit("   ", imageCount = AgentComposerPrompt.MAX_IMAGES))
        assertTrue(AgentComposerPrompt.withinLimit("x".repeat(AgentComposerPrompt.MAX_LENGTH) + "  "))
    }

    @Test
    fun `submit titles follow the subject`() {
        assertEquals("Start chat", AgentComposerPrompt.submitTitle(AgentComposerPrompt.Subject.None))
        assertEquals("Start coding", AgentComposerPrompt.submitTitle(AgentComposerPrompt.Subject.Issues(1)))
        assertEquals("Start batch · 2", AgentComposerPrompt.submitTitle(AgentComposerPrompt.Subject.Issues(2)))
        assertEquals("Start batch · 30", AgentComposerPrompt.submitTitle(AgentComposerPrompt.Subject.Issues(30)))
        assertEquals("Run action", AgentComposerPrompt.submitTitle(AgentComposerPrompt.Subject.Action))
    }
}
