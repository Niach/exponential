package com.exponential.app.ui.agent

import com.exponential.app.domain.ComposerAgentRow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * #EXP-1450: the agent row's recipient is not a subject. Picking an agent
 * over a Start-coding issue keeps the issue (headline + chips), and the send
 * to the agent forwards it.
 */
class AgentComposerRecipientTest {

    @Test
    fun `an agent pick keeps a seeded issue subject`() {
        val issues = ComposerSubject.Issues(listOf("i-1"))
        assertEquals(issues, issues.afterPick(ComposerAgentRow.Pick.AGENT))
        assertEquals(issues, issues.afterPick(ComposerAgentRow.Pick.TEMPORARY))
        assertEquals(issues, issues.afterPick(ComposerAgentRow.Pick.OPTION))
    }

    @Test
    fun `an agent pick drops only a prompt`() {
        val prompt = ComposerSubject.Prompt("p-1")
        assertNull(prompt.afterPick(ComposerAgentRow.Pick.AGENT))
        assertEquals(prompt, prompt.afterPick(ComposerAgentRow.Pick.TEMPORARY))
        assertEquals(prompt, prompt.afterPick(ComposerAgentRow.Pick.OPTION))
        assertNull((null as ComposerSubject?).afterPick(ComposerAgentRow.Pick.AGENT))
    }

    @Test
    fun `the agent send forwards the kept issues and needs no text`() {
        val input = agentSendInput("g-1", ComposerSubject.Issues(listOf("i-1", "i-2")), "", emptyList())
        assertEquals("g-1", input.agentId)
        assertEquals(listOf("i-1", "i-2"), input.issueIds)
        assertNull(input.text)
        assertNull(input.attachmentIds)
        assertNull(input.teamId)
        assertNull(input.deviceId)
    }

    @Test
    fun `a plain agent message carries no issues`() {
        val input = agentSendInput("g-1", null, "hi", listOf("a-1"))
        assertEquals("hi", input.text)
        assertEquals(listOf("a-1"), input.attachmentIds)
        assertNull(input.issueIds)
    }
}
