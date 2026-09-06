package com.exponential.app.ui.issue

import com.exponential.app.data.api.builtinChatAction
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

/**
 * EXP-756: the Chat tab's repository is OPTIONAL (EXP-739 made the hidden
 * builtin declare it so). The sheet sends `repo` ONLY when one is picked — an
 * empty pick means a repo-less chat that runs worktree-less in the agent's
 * scratch dir, and an empty string on the wire would read as a bogus repo id.
 */
class ChatRunPayloadTest {

    @Test
    fun `a repo-less chat sends only the prompt`() {
        val payload = chatRunPayload(prompt = "  Summarize the open bugs  ", repoId = "")
        assertEquals(mapOf("prompt" to "Summarize the open bugs"), payload)
        assertFalse(payload.containsKey("repo"))
    }

    @Test
    fun `a picked repository rides as the repo input`() {
        val payload = chatRunPayload(prompt = "Refactor the parser", repoId = "repo-1")
        assertEquals(mapOf("prompt" to "Refactor the parser", "repo" to "repo-1"), payload)
    }

    /** The payload keys are exactly the hidden builtin's input keys. */
    @Test
    fun `payload keys match the builtin chat inputs`() {
        val keys = builtinChatAction("team-1").inputs.orEmpty().map { it.key }.toSet()
        val payload = chatRunPayload(prompt = "p", repoId = "repo-1")
        assertEquals(keys, payload.keys)
    }
}
