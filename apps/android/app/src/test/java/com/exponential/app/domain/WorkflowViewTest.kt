package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * EXP-981: the workflow presentation rule, locked ×4 (web
 * `workflow-view.test.ts`, iOS `WorkflowViewTests`, desktop
 * `domain::workflow_view`) against the ONE contract fixture — same cases, the
 * same fixture-loading precedent `IssueGraphTest` uses.
 */
class WorkflowViewTest {

    private val fixture = Json.parseToJsonElement(contractFixtureJson("workflow-view.json")).jsonObject

    private fun shapeOf(metrics: kotlinx.serialization.json.JsonObject) = WorkflowView.Shape(
        nodes = metrics.getValue("nodes").jsonPrimitive.int,
        depth = metrics.getValue("depth").jsonPrimitive.int,
        width = metrics.getValue("width").jsonPrimitive.int,
        cycles = metrics.getValue("cycles").jsonArray.map { cycle ->
            cycle.jsonArray.map { it.jsonPrimitive.content }
        },
    )

    @Test
    fun `every status lands in the band the fixture names`() {
        val cases = fixture.getValue("bands").jsonArray
        assertTrue(cases.size >= 6)
        cases.forEach { element ->
            val case = element.jsonObject
            val status = case.getValue("status").jsonPrimitive.content
            assertEquals(
                status,
                case.getValue("band").jsonPrimitive.content,
                WorkflowView.band(status).key,
            )
        }
        // An unknown status is Done, never nothing.
        assertEquals(WorkflowView.Band.Done, WorkflowView.band("something-new"))
    }

    @Test
    fun `the shape line and the cycle note are byte exact`() {
        val cases = fixture.getValue("shapeLines").jsonArray
        assertTrue(cases.size >= 3)
        cases.forEach { element ->
            val case = element.jsonObject
            val shape = shapeOf(case.getValue("metrics").jsonObject)
            assertEquals(case.getValue("line").jsonPrimitive.content, WorkflowView.shapeLine(shape))
            val note = case.getValue("cycleNote").let { value ->
                (value as? kotlinx.serialization.json.JsonPrimitive)?.takeIf { it.isString }?.content
            }
            if (note == null) {
                assertNull(WorkflowView.cycleNote(shape))
            } else {
                assertEquals(note, WorkflowView.cycleNote(shape))
            }
        }
    }

    @Test
    fun `every caption and its tone match the fixture`() {
        val cases = fixture.getValue("captions").jsonArray
        assertTrue(cases.size >= 8)
        cases.forEach { element ->
            val case = element.jsonObject
            val node = case.getValue("node").jsonObject
            val caption = WorkflowView.CaptionNode(
                kind = node.getValue("kind").jsonPrimitive.content,
                state = node.getValue("state").jsonPrimitive.content,
                risk = node.getValue("risk").jsonPrimitive.content,
            )
            val status = case.getValue("workflowStatus").jsonPrimitive.content
            assertEquals(
                case.getValue("caption").jsonPrimitive.content,
                WorkflowView.nodeCaption(caption, status),
            )
            assertEquals(
                case.getValue("tone").jsonPrimitive.content,
                WorkflowView.nodeTone(caption.state).key,
            )
        }
    }

    @Test
    fun `a compound node names its member count`() {
        fixture.getValue("titles").jsonArray.forEach { element ->
            val case = element.jsonObject
            assertEquals(
                case.getValue("title").jsonPrimitive.content,
                WorkflowView.nodeTitle(
                    case.getValue("identifier").jsonPrimitive.content,
                    case.getValue("members").jsonPrimitive.int,
                ),
            )
        }
    }

    @Test
    fun `the blocks relations lift to one edge per node pair`() {
        val cases = fixture.getValue("edges").jsonArray
        assertTrue(cases.isNotEmpty())
        cases.forEach { element ->
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val nodes = case.getValue("nodes").jsonArray.map { node ->
                val obj = node.jsonObject
                WorkflowView.EdgeNode(
                    id = obj.getValue("id").jsonPrimitive.content,
                    issueId = obj.getValue("issueId").jsonPrimitive.content,
                    memberIssueIds = obj.getValue("memberIssueIds").jsonArray
                        .map { it.jsonPrimitive.content },
                )
            }
            val relations = case.getValue("relations").jsonArray.map { relation ->
                val obj = relation.jsonObject
                WorkflowView.EdgeRelation(
                    type = obj.getValue("type").jsonPrimitive.content,
                    issueId = obj.getValue("issueId").jsonPrimitive.content,
                    relatedIssueId = obj.getValue("relatedIssueId").jsonPrimitive.content,
                )
            }
            val cycleEdges = case["cycleEdges"]?.jsonArray?.map { it.jsonPrimitive.content }
                .orEmpty()
            val expected = case.getValue("expected").jsonArray.map { edge ->
                val obj = edge.jsonObject
                WorkflowView.Edge(
                    from = obj.getValue("from").jsonPrimitive.content,
                    to = obj.getValue("to").jsonPrimitive.content,
                    cycle = obj.getValue("cycle").jsonPrimitive.boolean,
                )
            }
            assertEquals(name, expected, WorkflowView.edges(nodes, relations, cycleEdges))
        }
    }

    @Test
    fun `the list and bulk-bar words are the shared ones`() {
        assertEquals(listOf("Running", "Draft", "Done"), WorkflowView.BANDS.map { it.title })
        assertEquals("Workflows", WorkflowView.WORKFLOWS_TITLE)
        assertEquals("No workflows yet", WorkflowView.WORKFLOWS_EMPTY_TITLE)
        assertEquals(
            "Select backlog issues on a board and choose Create workflow to plan them as one parallel run.",
            WorkflowView.WORKFLOWS_EMPTY_BODY,
        )
        assertEquals("Plan", WorkflowView.PLAN_WORKFLOW_LABEL)
        assertEquals("Delete workflow", WorkflowView.DELETE_WORKFLOW_LABEL)
        assertEquals("Start as batch", WorkflowView.START_AS_BATCH_LABEL)
        assertEquals("Start as stack", WorkflowView.START_AS_STACK_LABEL)
        assertEquals("Create workflow…", WorkflowView.CREATE_WORKFLOW_LABEL)
    }

    @Test
    fun `the how-it-runs pickers name every contract value`() {
        assertEquals(
            listOf("No gate", "Agent review", "Human review"),
            DomainContract.wfGateValues.map(WorkflowView::gateLabel),
        )
        assertEquals(
            listOf("On contract", "On PR open", "When landed"),
            DomainContract.wfStartOnValues.map(WorkflowView::startOnLabel),
        )
        assertEquals(
            listOf("Low", "Medium", "High"),
            DomainContract.wfRiskValues.map(WorkflowView::riskLabel),
        )
        assertEquals(
            listOf("Contract", "Leaf", "Integration"),
            DomainContract.wfNodeKindValues.map(WorkflowView::nodeKindLabel),
        )
    }
}
