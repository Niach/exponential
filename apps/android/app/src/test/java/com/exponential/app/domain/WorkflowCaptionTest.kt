package com.exponential.app.domain

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-850 (S7): the workflow caption, locked ×4 (TS
 * `workflow-caption.test.ts`, Rust `steer::workflow`, iOS
 * `WorkflowCaptionTests`) against the ONE contract fixture — same cases, same
 * expectations, byte for byte.
 *
 * The fixture speaks the caption's INPUT (name / status / phases / agents with
 * a state), not the full wire `workflow` frame — no id, no summary, no
 * telemetry — so each case is lifted into a [WorkflowState] here rather than
 * loosening the decoder.
 */
class WorkflowCaptionTest {

    private data class FixtureCase(
        val name: String,
        val workflow: WorkflowState,
        val expected: String,
    )

    private fun cases(): List<FixtureCase> =
        Json.parseToJsonElement(fixtureJson()).jsonArray.map { element ->
            val case = element.jsonObject
            val workflow = case.getValue("workflow").jsonObject
            FixtureCase(
                name = case.getValue("name").jsonPrimitive.content,
                workflow = WorkflowState(
                    id = "toolu_fixture",
                    name = workflow.getValue("name").jsonPrimitive.content,
                    status = workflow.getValue("status").jsonPrimitive.content,
                    phases = workflow["phases"]?.takeUnless { it is JsonNull }?.jsonArray
                        ?.map { phase ->
                            WorkflowPhase(
                                index = phase.jsonObject.getValue("index").jsonPrimitive.int,
                                title = phase.jsonObject.getValue("title").jsonPrimitive.content,
                            )
                        }
                        .orEmpty(),
                    agents = workflow["agents"]?.takeUnless { it is JsonNull }?.jsonArray
                        ?.map { agent ->
                            WorkflowAgent(
                                index = agent.jsonObject.getValue("index").jsonPrimitive.int,
                                label = agent.jsonObject["label"]?.jsonPrimitive?.content.orEmpty(),
                                phaseIndex = agent.jsonObject["phaseIndex"]?.jsonPrimitive?.int,
                                state = agent.jsonObject.getValue("state").jsonPrimitive.content,
                            )
                        }
                        .orEmpty(),
                ),
                expected = case.getValue("expected").jsonPrimitive.content,
            )
        }

    @Test
    fun `every fixture case renders byte exact`() {
        val cases = cases()
        assertTrue(cases.size >= 12)
        for (case in cases) {
            assertEquals(case.name, case.expected, WorkflowCaption.of(case.workflow))
            // The extension is the same function under a nicer name.
            assertEquals(case.name, case.expected, case.workflow.caption())
        }
    }

    @Test
    fun `the fixture covers every branch`() {
        val expectations = cases().map { it.expected }
        fun covers(needle: String) = expectations.any { it.contains(needle) }
        assertTrue(covers("· starting"))
        assertTrue(covers("agents done"))
        assertTrue(covers("· done · 1 agent"))
        assertTrue(covers("· done · 3 agents"))
        assertTrue(covers("· failed"))
        assertTrue(covers("· stopped"))
        assertEquals(" · ", WorkflowCaption.SEPARATOR)
        assertTrue(expectations.all { it.startsWith("${WorkflowCaption.PREFIX} ") })
    }

    // The phase segment is the caption's one derived value: the RUNNING agent
    // with the highest index names it, and a queued tail never does.
    @Test
    fun `the lead agent is the highest-index running one`() {
        val workflow = WorkflowState(
            id = "w",
            name = "probe",
            status = WORKFLOW_STATUS_RUNNING,
            phases = listOf(WorkflowPhase(1, "Alpha"), WorkflowPhase(2, "Beta")),
            agents = listOf(
                WorkflowAgent(index = 1, label = "one", phaseIndex = 1, state = WORKFLOW_AGENT_STATE_RUNNING),
                WorkflowAgent(index = 2, label = "two", phaseIndex = 2, state = WORKFLOW_AGENT_STATE_RUNNING),
                WorkflowAgent(index = 3, label = "three", phaseIndex = 2, state = WORKFLOW_AGENT_STATE_QUEUED),
            ),
        )
        assertEquals(2, workflow.leadAgent()?.index)
        assertEquals("Beta", workflow.phaseTitle(workflow.leadAgent()))
        assertEquals("Workflow probe · 0/3 agents done · Beta", workflow.caption())
    }

    @Test
    fun `with nothing running the highest-index agent names the phase`() {
        val workflow = WorkflowState(
            id = "w",
            name = "probe",
            status = WORKFLOW_STATUS_RUNNING,
            phases = listOf(WorkflowPhase(1, "Alpha"), WorkflowPhase(2, "Beta")),
            agents = listOf(
                WorkflowAgent(index = 1, label = "one", phaseIndex = 1, state = WORKFLOW_AGENT_STATE_DONE),
                WorkflowAgent(index = 2, label = "two", phaseIndex = 2, state = WORKFLOW_AGENT_STATE_QUEUED),
            ),
        )
        assertEquals("Workflow probe · 1/2 agents done · Beta", workflow.caption())
    }

    // An engine newer than this build: an unknown status is a workflow still
    // in flight, never a blank caption.
    @Test
    fun `an unknown status still reads as running`() {
        val workflow = WorkflowState(
            id = "w",
            name = "probe",
            status = "paused",
            agents = listOf(WorkflowAgent(index = 1, label = "one", state = WORKFLOW_AGENT_STATE_RUNNING)),
        )
        assertEquals("Workflow probe · 0/1 agents done", workflow.caption())
    }
}

/**
 * The contract fixture, located relative to the Gradle test working directory
 * (the `app` module dir) with fallbacks so the suite also runs from the
 * `apps/android` dir or the repo root — the [ToolGroupSummaryTest] recipe.
 */
private fun fixtureJson(): String {
    val candidates = listOf(
        "../../../packages/domain-contract/fixtures/workflow-caption.json",
        "../../packages/domain-contract/fixtures/workflow-caption.json",
        "packages/domain-contract/fixtures/workflow-caption.json",
    )
    val file = candidates.map(::File).firstOrNull { it.isFile }
        ?: error("workflow-caption.json not found from ${File(".").absolutePath}")
    return file.readText()
}
