package com.exponential.app.domain

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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

    /** A fixture value that is either a JSON string or JSON null. */
    private fun JsonElement.stringOrNull(): String? =
        (this as? JsonPrimitive)?.takeIf { it.isString }?.content

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
                    // EXP-983: the engine's serialization edges, absent on the
                    // plain cases.
                    afterNodeIds = obj["afterNodeIds"]?.jsonArray
                        ?.map { it.jsonPrimitive.content }
                        .orEmpty(),
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
                    serial = obj.getValue("serial").jsonPrimitive.boolean,
                )
            }
            assertEquals(name, expected, WorkflowView.edges(nodes, relations, cycleEdges))
        }
    }

    // ── Speculative starts (EXP-983) ────────────────────────────────────────

    @Test
    fun `every edge is drawn in the style the fixture names`() {
        val cases = fixture.getValue("edgeStyles").jsonArray
        assertTrue(cases.size >= 7)
        cases.forEach { element ->
            val case = element.jsonObject
            val edge = case.getValue("edge").jsonObject
            assertEquals(
                case.getValue("style").jsonPrimitive.content,
                WorkflowView.edgeStyle(
                    WorkflowView.Edge(
                        from = "a",
                        to = "b",
                        cycle = edge.getValue("cycle").jsonPrimitive.boolean,
                        serial = edge.getValue("serial").jsonPrimitive.boolean,
                    ),
                    case.getValue("fromState").jsonPrimitive.content,
                    case.getValue("toState").jsonPrimitive.content,
                ).key,
            )
        }
    }

    @Test
    fun `the contract line is the shared sentence`() {
        assertEquals("Contract published", WorkflowView.CONTRACT_PUBLISHED_LABEL)
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

    // ── Running a workflow (EXP-982) ────────────────────────────────────────

    @Test
    fun `every start blocker is the fixture's sentence, in its order`() {
        val cases = fixture.getValue("startBlockers").jsonArray
        assertTrue(cases.size >= 7)
        cases.forEach { element ->
            val case = element.jsonObject
            val workflow = case.getValue("workflow").jsonObject
            val startable = WorkflowView.Startable(
                status = workflow.getValue("status").jsonPrimitive.content,
                deviceId = workflow.getValue("deviceId").stringOrNull(),
                repositoryId = workflow.getValue("repositoryId").stringOrNull(),
                startOn = workflow.getValue("startOn").jsonPrimitive.content,
            )
            val shape = shapeOf(case.getValue("metrics").jsonObject)
            assertEquals(
                case.getValue("blocker").stringOrNull(),
                WorkflowView.startBlocker(startable, shape),
            )
        }
        // EXP-983: no start mode is refused any more — all three run.
        DomainContract.wfStartOnValues.forEach { startOn ->
            assertNull(
                startOn,
                WorkflowView.startBlocker(
                    WorkflowView.Startable(
                        status = DomainContract.wfStatusDraft,
                        deviceId = "dev",
                        repositoryId = "repo",
                        startOn = startOn,
                    ),
                    WorkflowView.Shape(nodes = 3, depth = 2, width = 2),
                ),
            )
        }
    }

    @Test
    fun `the merge train orders and labels every waiting node`() {
        val cases = fixture.getValue("trains").jsonArray
        assertTrue(cases.size >= 2)
        cases.forEach { element ->
            val case = element.jsonObject
            val name = case.getValue("name").jsonPrimitive.content
            val nodes = case.getValue("nodes").jsonArray.map { node ->
                val obj = node.jsonObject
                WorkflowView.TrainNode(
                    id = obj.getValue("id").jsonPrimitive.content,
                    kind = obj.getValue("kind").jsonPrimitive.content,
                    state = obj.getValue("state").jsonPrimitive.content,
                    wave = obj.getValue("wave").jsonPrimitive.int,
                    lane = obj.getValue("lane").jsonPrimitive.int,
                    approvedAt = obj.getValue("approvedAt").stringOrNull(),
                )
            }
            val expected = case.getValue("expected").jsonArray.map { entry ->
                val obj = entry.jsonObject
                obj.getValue("id").jsonPrimitive.content to
                    obj.getValue("step").jsonPrimitive.content
            }
            assertEquals(
                name,
                expected,
                WorkflowView.mergeTrain(nodes).map { it.id to it.step.key },
            )
        }
    }

    @Test
    fun `each train step wears the fixture's word`() {
        val labels = fixture.getValue("trainStepLabels").jsonObject
        assertEquals(labels.size, WorkflowView.TrainStep.entries.size)
        WorkflowView.TrainStep.entries.forEach { step ->
            assertEquals(
                step.key,
                labels.getValue(step.key).jsonPrimitive.content,
                WorkflowView.trainStepLabel(step),
            )
        }
    }

    @Test
    fun `the final pull request appears only once everything is in`() {
        val cases = fixture.getValue("finalPr").jsonArray
        // EXP-984 added the two `proposed` cases: a proposal is not part of
        // the run, so it neither holds the final PR back nor stands in for a
        // workflow that has no real nodes at all.
        assertTrue(cases.size >= 8)
        cases.forEach { element ->
            val case = element.jsonObject
            val states = case.getValue("states").jsonArray.map { it.jsonPrimitive.content }
            val number = case.getValue("finalPrNumber").let { value ->
                (value as? JsonPrimitive)?.takeIf { !it.isString }?.intOrNull
            }
            assertEquals(
                case.getValue("caption").stringOrNull(),
                WorkflowView.finalPrCaption(
                    nodeStates = states,
                    finalPrState = case.getValue("finalPrState").stringOrNull(),
                    finalPrNumber = number,
                ),
            )
        }
    }

    @Test
    fun `a list row's subtitle names the status a band cannot`() {
        val cases = fixture.getValue("rowSubtitles").jsonArray
        assertTrue(cases.size >= 4)
        cases.forEach { element ->
            val case = element.jsonObject
            assertEquals(
                case.getValue("subtitle").jsonPrimitive.content,
                WorkflowView.rowSubtitle(
                    case.getValue("status").jsonPrimitive.content,
                    shapeOf(case.getValue("metrics").jsonObject),
                ),
            )
        }
    }

    @Test
    fun `the run words are the shared ones`() {
        assertEquals("Start", WorkflowView.START_WORKFLOW_LABEL)
        assertEquals("Pause", WorkflowView.PAUSE_WORKFLOW_LABEL)
        assertEquals("Resume", WorkflowView.RESUME_WORKFLOW_LABEL)
        assertEquals("Cancel workflow", WorkflowView.CANCEL_WORKFLOW_LABEL)
        assertEquals(
            "Its live runs end and its branch is deleted. Nothing reached the default branch.",
            WorkflowView.CANCEL_WORKFLOW_CONFIRM,
        )
        assertEquals("Approve and land", WorkflowView.APPROVE_NODE_LABEL)
        assertEquals("Withdraw approval", WorkflowView.WITHDRAW_APPROVAL_LABEL)
        assertEquals("Merge train", WorkflowView.MERGE_TRAIN_TITLE)
        assertEquals("Nothing is waiting to land.", WorkflowView.MERGE_TRAIN_EMPTY)
        assertEquals("Final pull request", WorkflowView.FINAL_PR_TITLE)
        assertEquals("Running now", WorkflowView.RUNNING_NOW_LABEL)
        assertEquals("Retry", WorkflowView.RETRY_NODE_LABEL)
        assertEquals("Skip", WorkflowView.SKIP_NODE_LABEL)
        assertEquals(
            "Its dependents go on without it. The node's work is not part of the " +
                "final pull request.",
            WorkflowView.SKIP_NODE_CONFIRM,
        )
    }

    // ── Review gate, dynamic graphs, budgets, metrics (EXP-984) ─────────────

    @Test
    fun `every agent review reads as the fixture's one line`() {
        val cases = fixture.getValue("reviewLines").jsonArray
        assertTrue(cases.size >= 5)
        cases.forEach { element ->
            val case = element.jsonObject
            val review = case.getValue("review").jsonObject
            assertEquals(
                case.getValue("line").jsonPrimitive.content,
                WorkflowView.reviewLine(
                    WorkflowView.ReviewLine(
                        verdict = review.getValue("verdict").jsonPrimitive.content,
                        round = review.getValue("round").jsonPrimitive.int,
                        // A null oracle = the reviewer ran no check.
                        oraclePassed = (review.getValue("oracle") as? kotlinx.serialization.json.JsonObject)
                            ?.getValue("passed")?.jsonPrimitive?.boolean,
                    ),
                    // Advisory only while the verdict did not clear the node.
                    nodeApproved = case.getValue("approved").jsonPrimitive.boolean,
                ),
            )
        }
    }

    @Test
    fun `the metrics section is the fixture's rows, in its order`() {
        val cases = fixture.getValue("metricRows").jsonArray
        assertTrue(cases.size >= 3)
        cases.forEach { element ->
            val case = element.jsonObject
            // Through the tolerant jsonb read the synced row takes: a counter
            // a newer server wrote as something other than a number simply
            // does not count.
            val counters = workflowMetricCounters(case.getValue("metrics").toString())
            val expected = case.getValue("rows").jsonArray.map { row ->
                val obj = row.jsonObject
                obj.getValue("label").jsonPrimitive.content to
                    obj.getValue("value").jsonPrimitive.content
            }
            assertEquals(
                expected,
                WorkflowView.metricRows(counters).map { it.label to it.value },
            )
        }
    }

    @Test
    fun `the review, proposal, budget and metrics words are the shared ones`() {
        assertEquals("Admit", WorkflowView.ADMIT_NODE_LABEL)
        assertEquals("Dismiss", WorkflowView.DISMISS_NODE_LABEL)
        assertEquals(
            "Filed during the run. Admit it into the workflow or dismiss it.",
            WorkflowView.PROPOSED_NODE_NOTE,
        )
        assertEquals("Agent review", WorkflowView.AGENT_REVIEW_TITLE)
        assertEquals("Review model", WorkflowView.REVIEW_MODEL_LABEL)
        assertEquals("Metrics", WorkflowView.METRICS_TITLE)
        assertEquals("Contract model", WorkflowView.CONTRACT_MODEL_LABEL)
        assertEquals("Integration model", WorkflowView.INTEGRATION_MODEL_LABEL)
        assertEquals("High-risk model", WorkflowView.RISK_MODEL_LABEL)
        assertEquals("Same as Model", WorkflowView.SAME_AS_MODEL_LABEL)
    }

    @Test
    fun `a review's line is read off the synced jsonb cell`() {
        val review = workflowNodeReview(
            """
                {
                  "verdict": "approve",
                  "findings": "The contract tests cover the new branch.",
                  "oracle": {"command": "bun run test", "passed": true},
                  "model": "opus",
                  "round": 1,
                  "at": "2026-09-19 12:00:00+00"
                }
            """.trimIndent(),
        )
        assertEquals("Approved · round 1 · checks passed", WorkflowView.reviewLine(review!!.line, nodeApproved = true))
        // A cell with no verdict is nothing to show, never an empty card.
        assertNull(workflowNodeReview("""{"round": 2}"""))
        assertNull(workflowNodeReview(null))
    }

    @Test
    fun `the how-it-runs pickers name every contract value`() {
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
