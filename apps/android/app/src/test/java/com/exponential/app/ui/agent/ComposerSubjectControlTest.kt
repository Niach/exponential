package com.exponential.app.ui.agent

import com.exponential.app.domain.ComposerAgentRow
import com.exponential.app.domain.contractFixtureJson
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

// EXP-1450: composer-home.json `agentRow.controlCases` and `pickCases`
// replayed on the composer's own subject type (`subjectAfterControl`, the
// transition `AgentComposerViewModel.applyRowPick` runs).
class ComposerSubjectControlTest {
    private val row by lazy {
        Json.parseToJsonElement(contractFixtureJson("composer-home.json")).jsonObject.getValue("agentRow").jsonObject
    }

    private fun subject(element: JsonElement?): ComposerSubject? {
        if (element == null || element is JsonNull) return null
        val obj = element.jsonObject
        return when (val kind = obj.getValue("kind").jsonPrimitive.content) {
            "issues" -> ComposerSubject.Issues(obj.getValue("ids").jsonArray.map { it.jsonPrimitive.content })
            "prompt" -> ComposerSubject.Prompt(obj.getValue("id").jsonPrimitive.content)
            else -> error("unknown subject kind $kind")
        }
    }

    private fun control(name: String): ComposerControl = when (name) {
        "agentRow" -> ComposerControl.AgentRow
        "device" -> ComposerControl.Device
        "agent" -> ComposerControl.Agent
        "account" -> ComposerControl.Account
        "model" -> ComposerControl.Model
        "effort" -> ComposerControl.Effort
        "repository" -> ComposerControl.Repository
        "work" -> ComposerControl.Work
        "mcpServer", "mcp" -> ComposerControl.McpServer
        else -> error("unknown control $name")
    }

    @Test
    fun `controlCases match the fixture`() {
        val cases = row.getValue("controlCases").jsonArray.map { it.jsonObject }
        assertTrue(cases.isNotEmpty())
        for (case in cases) {
            val name = case.getValue("name").jsonPrimitive.content
            val before = subject(case["subject"])
            val expected = subject(case["expected"])
            val c = control(case.getValue("control").jsonPrimitive.content)
            // The agent row: an agent, the Temporary circle, then back again.
            for (pick in listOf("g-me", ComposerAgentRow.TEMPORARY_PICK, "g-b")) {
                assertEquals("$name ($pick)", expected, subjectAfterControl(before, c, pick))
            }
        }
    }

    @Test
    fun `pickCases match the fixture on the composer subject`() {
        for (case in row.getValue("pickCases").jsonArray.map { it.jsonObject }) {
            val pick = case.getValue("pick").jsonPrimitive.content
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                subject(case["expected"]),
                subjectAfterControl(subject(case["subject"]), ComposerControl.AgentRow, pick),
            )
        }
    }

    @Test
    fun `a picked agent keeps the home question even over issue chips`() {
        assertEquals(
            "What do you want to build?",
            composerPlaceholder(ComposerSubject.Issues(listOf("i-1")), home = true, recipient = true),
        )
    }
}
