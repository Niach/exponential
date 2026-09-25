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
            )
            val shape = shapeOf(case.getValue("metrics").jsonObject)
            assertEquals(
                case.getValue("blocker").stringOrNull(),
                WorkflowView.startBlocker(startable, shape),
            )
        }
        // EXP-1066: one start rule — a ready draft with a runner starts.
        assertNull(
            WorkflowView.startBlocker(
                WorkflowView.Startable(
                    status = DomainContract.wfStatusDraft,
                    deviceId = "dev",
                    repositoryId = "repo",
                ),
                WorkflowView.Shape(nodes = 3, depth = 2, width = 2),
            ),
        )
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
        assertEquals(
            "Its live runs end and its branch is deleted. Nothing reached the default branch.",
            WorkflowView.CANCEL_WORKFLOW_CONFIRM,
        )
        assertEquals("Final pull request", WorkflowView.FINAL_PR_TITLE)
        // EXP-1033: merging that pull request is the run's ONE human review.
        assertEquals("Merge", WorkflowView.MERGE_FINAL_PR_LABEL)
        assertEquals(
            "The workflow's branch is squash-merged into the default branch and " +
                "the run is done.",
            WorkflowView.MERGE_FINAL_PR_CONFIRM,
        )
        assertEquals("Retry", WorkflowView.RETRY_NODE_LABEL)
        assertEquals("Skip", WorkflowView.SKIP_NODE_LABEL)
        assertEquals(
            "Its dependents go on without it. The node's work is not part of the " +
                "final pull request.",
            WorkflowView.SKIP_NODE_CONFIRM,
        )
    }

    // ── Review gate, dynamic graphs (EXP-984) ─────────────

    @Test
    fun `the review and proposal words are the shared ones`() {
        assertEquals("Admit", WorkflowView.ADMIT_NODE_LABEL)
        assertEquals("Dismiss", WorkflowView.DISMISS_NODE_LABEL)
        assertEquals(
            "Filed during the run. Admit it into the workflow or dismiss it.",
            WorkflowView.PROPOSED_NODE_NOTE,
        )
        // The chip of a node whose issue row has not synced: the first 8
        // characters of the issue id stand in for the identifier, this is the
        // title. A compound one still reads `abcd1234 +2`.
        assertEquals("Not synced yet", WorkflowView.NODE_UNSYNCED_TITLE)
        assertEquals("abcd1234 +2", WorkflowView.nodeTitle("abcd1234", 2))
    }

    @Test
    fun `a review is read off the synced jsonb cell`() {
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
        assertEquals("approve", review!!.verdict)
        assertEquals(1, review.round)
        assertEquals(true, review.oracle?.passed)
        // A cell with no verdict is nothing to show, never an empty card.
        assertNull(workflowNodeReview("""{"round": 2}"""))
        assertNull(workflowNodeReview(null))
    }

    // ── The launch (EXP-1029) ──────────────────────────────────────────────
    // The same acceptance table web's `workflow-launch.test.ts` carries, case
    // for case: ONE rule picks a workflow run's models on every client.

    private val claude = NormalizedWorkflowLaunch(
        agent = "claude",
        model = "opus",
        strongModel = "fable",
    )

    @Test
    fun `reads the new shape verbatim`() {
        assertEquals(
            NormalizedWorkflowLaunch(
                agent = "claude",
                model = "sonnet",
                strongModel = "opus",
                account = "p-1",
            ),
            normalizedLaunch(
                workflowLaunch(
                    """{"agent":"claude","account":"p-1","model":"sonnet","strongModel":"opus"}""",
                ),
            ),
        )
    }

    @Test
    fun `fills an empty row from the claude defaults`() {
        assertEquals(claude, normalizedLaunch(workflowLaunch("{}")))
        assertEquals(claude, normalizedLaunch(workflowLaunch(null)))
        assertEquals(claude, normalizedLaunch(workflowLaunch("garbage")))
    }

    @Test
    fun `fills a codex row from the codex defaults`() {
        assertEquals(
            NormalizedWorkflowLaunch(
                agent = "codex",
                model = "gpt-5.6-sol",
                strongModel = "gpt-5.6-luna",
            ),
            normalizedLaunch(workflowLaunch("""{"agent":"codex"}""")),
        )
    }

    @Test
    fun `degrades an unknown agent to claude`() {
        assertEquals("claude", normalizedLaunch(workflowLaunch("""{"agent":"pi"}""")).agent)
    }

    @Test
    fun `folds an old row's pins into strongModel, reviewModel first`() {
        assertEquals(
            "sonnet",
            normalizedLaunch(
                workflowLaunch("""{"agent":"claude","model":"opus","contractModel":"sonnet"}"""),
            ).strongModel,
        )
        assertEquals(
            "opus",
            normalizedLaunch(
                workflowLaunch("""{"agent":"claude","riskModel":"sonnet","reviewModel":"opus"}"""),
            ).strongModel,
        )
        assertEquals(
            "sonnet",
            normalizedLaunch(
                workflowLaunch("""{"agent":"claude","integrationModel":"sonnet"}"""),
            ).strongModel,
        )
    }

    @Test
    fun `lets a stored strongModel win over every legacy pin`() {
        assertEquals(
            "opus",
            normalizedLaunch(
                workflowLaunch("""{"strongModel":"opus","contractModel":"sonnet"}"""),
            ).strongModel,
        )
    }

    @Test
    fun `drops subagentModel, effort and maxParallel`() {
        assertEquals(
            claude,
            normalizedLaunch(
                workflowLaunch(
                    """{"agent":"claude","subagentModel":"sonnet","effort":"high","maxParallel":5}""",
                ),
            ),
        )
    }

    @Test
    fun `keeps model as model, even beside old pins`() {
        assertEquals(
            NormalizedWorkflowLaunch(agent = "claude", model = "sonnet", strongModel = "fable"),
            normalizedLaunch(workflowLaunch("""{"model":"sonnet","contractModel":"fable"}""")),
        )
    }

    @Test
    fun `drops a blank account`() {
        assertEquals(claude, normalizedLaunch(workflowLaunch("""{"account":""}""")))
        assertEquals(claude, normalizedLaunch(workflowLaunch("""{"account":"   "}""")))
    }

    @Test
    fun `runs a leaf on the cheap model`() {
        assertEquals("opus", modelForNode(claude, "leaf", "low"))
        assertEquals("opus", modelForNode(claude, "leaf", "medium"))
    }

    @Test
    fun `runs contract and integration nodes on the strong model`() {
        assertEquals("fable", modelForNode(claude, "contract", "low"))
        assertEquals("fable", modelForNode(claude, "integration", "low"))
    }

    @Test
    fun `runs a high-risk node on the strong model, whatever its kind`() {
        assertEquals("fable", modelForNode(claude, "leaf", "high"))
    }

    @Test
    fun `reviews every node on the strong model`() {
        assertEquals("fable", reviewModelFor(claude))
        assertEquals(
            "gpt-5.6-luna",
            reviewModelFor(
                NormalizedWorkflowLaunch(
                    agent = "codex",
                    model = "gpt-5.6-sol",
                    strongModel = "gpt-5.6-luna",
                ),
            ),
        )
    }

    @Test
    fun `the plan labels name every contract value`() {
        assertEquals(
            listOf("Low", "Medium", "High"),
            DomainContract.wfRiskValues.map(WorkflowView::riskLabel),
        )
    }

    // ── EXP-1082: display states, the strip, the header, the actions ──────

    @Test
    fun `every internal state folds into the display state the fixture names`() {
        val cases = fixture.getValue("displayStates").jsonArray
        assertTrue(cases.size >= 10)
        cases.forEach { element ->
            val case = element.jsonObject
            val state = case.getValue("state").jsonPrimitive.content
            val display = WorkflowView.nodeDisplayState(state)
            assertEquals(state, case.getValue("display").jsonPrimitive.content, display.wire)
            assertEquals(state, case.getValue("caption").jsonPrimitive.content, display.label)
        }
        assertEquals(WorkflowNodeDisplayState.QUEUED, WorkflowView.nodeDisplayState("something-new"))
    }

    @Test
    fun `the needs-you label is byte exact`() {
        assertEquals(fixture.getValue("needsYouLabel").jsonPrimitive.content, WorkflowView.NEEDS_YOU_LABEL)
    }

    @Test
    fun `the node strip lays waves out as the fixture draws them`() {
        fixture.getValue("nodeStrips").jsonArray.forEach { element ->
            val case = element.jsonObject
            val nodes = case.getValue("nodes").jsonArray.map { raw ->
                val n = raw.jsonObject
                StripNodeInput(
                    id = n.getValue("id").jsonPrimitive.content,
                    identifier = n.getValue("identifier").jsonPrimitive.content,
                    state = n.getValue("state").jsonPrimitive.content,
                    wave = n.getValue("wave").jsonPrimitive.int,
                    lane = n.getValue("lane").jsonPrimitive.int,
                    members = n.getValue("members").jsonPrimitive.int,
                    live = n.getValue("live").jsonPrimitive.boolean,
                    needsYou = n.getValue("needsYou").jsonPrimitive.boolean,
                    note = n["note"]?.stringOrNull(),
                )
            }
            val edges = case.getValue("edges").jsonArray.map { edge ->
                val pair = edge.jsonArray
                pair[0].jsonPrimitive.content to pair[1].jsonPrimitive.content
            }
            val expected = case.getValue("strip").jsonArray.map { raw ->
                val w = raw.jsonObject
                StripWave(
                    wave = w.getValue("wave").jsonPrimitive.int,
                    nodes = w.getValue("nodes").jsonArray.map { chipRaw ->
                        val c = chipRaw.jsonObject
                        val display = c.getValue("display").jsonPrimitive.content
                        NodeChip(
                            id = c.getValue("id").jsonPrimitive.content,
                            title = c.getValue("title").jsonPrimitive.content,
                            display = WorkflowNodeDisplayState.entries.first { it.wire == display },
                            caption = c.getValue("caption").jsonPrimitive.content,
                            stacked = c.getValue("stacked").jsonPrimitive.boolean,
                            members = c.getValue("members").jsonPrimitive.int,
                            live = c.getValue("live").jsonPrimitive.boolean,
                            needsYou = c.getValue("needsYou").jsonPrimitive.boolean,
                        )
                    },
                )
            }
            assertEquals(case.getValue("name").jsonPrimitive.content, expected, WorkflowView.nodeStrip(nodes, edges))
        }
    }

    @Test
    fun `the header caption is byte exact`() {
        fixture.getValue("headerCaptions").jsonArray.forEach { element ->
            val case = element.jsonObject
            val nodes = case.getValue("nodes").jsonArray.map { raw ->
                val n = raw.jsonObject
                HeaderNode(n.getValue("state").jsonPrimitive.content, n.getValue("members").jsonPrimitive.int)
            }
            assertEquals(
                case.getValue("name").jsonPrimitive.content,
                case.getValue("caption").jsonPrimitive.content,
                WorkflowView.headerCaption(
                    case.getValue("status").jsonPrimitive.content,
                    nodes,
                    case["device"]?.stringOrNull(),
                ),
            )
        }
    }

    @Test
    fun `the primary action follows status and runner`() {
        fixture.getValue("primaryActions").jsonArray.forEach { element ->
            val case = element.jsonObject
            val status = case.getValue("status").jsonPrimitive.content
            assertEquals(
                status,
                case["action"]?.stringOrNull(),
                WorkflowView.primaryAction(
                    status,
                    case["device"]?.stringOrNull(),
                    case["finalPrState"]?.stringOrNull(),
                )?.wire,
            )
        }
    }

    @Test
    fun `a node chip menu offers only what its state allows`() {
        fixture.getValue("chipMenus").jsonArray.forEach { element ->
            val case = element.jsonObject
            val state = case.getValue("state").jsonPrimitive.content
            assertEquals(
                state,
                case.getValue("menu").jsonArray.map { it.jsonPrimitive.content },
                WorkflowView.nodeChipMenu(state).map { it.wire },
            )
        }
    }

    @Test
    fun `the header overflow follows the status`() {
        val cases = fixture.getValue("overflowMenus").jsonArray
        assertTrue(cases.size >= 7)
        cases.forEach { element ->
            val case = element.jsonObject
            val status = case.getValue("status").jsonPrimitive.content
            assertEquals(
                status,
                case.getValue("menu").jsonArray.map { it.jsonPrimitive.content },
                WorkflowView.overflowMenu(status).map { it.wire },
            )
        }
    }

    @Test
    fun `the page labels are byte exact`() {
        val labels = fixture.getValue("pageLabels").jsonObject
        fun label(key: String) = labels.getValue(key).jsonPrimitive.content
        assertEquals(label("allNodes"), WorkflowView.ALL_NODES_LABEL)
        assertEquals(label("decisions"), WorkflowView.DECISIONS_LABEL)
        assertEquals(label("stop"), WorkflowView.STOP_WORKFLOW_LABEL)
        assertEquals(label("pickDevice"), WorkflowView.PICK_DEVICE_LABEL)
        assertEquals(label("runsOn"), WorkflowView.RUNS_ON_LABEL)
        assertEquals(label("reviewFinalPr"), WorkflowView.REVIEW_FINAL_PR_LABEL)
    }

    @Test
    fun `the header status glyph reads like a node display state`() {
        val cases = fixture.getValue("statusGlyphs").jsonArray
        assertTrue(cases.size >= 7)
        cases.forEach { element ->
            val case = element.jsonObject
            val status = case.getValue("status").jsonPrimitive.content
            assertEquals(
                status,
                case.getValue("display").jsonPrimitive.content,
                WorkflowView.statusGlyph(status).wire,
            )
        }
    }

    @Test
    fun `a proposed node's strip caption is the shared note`() {
        assertEquals(fixture.getValue("proposedNodeNote").jsonPrimitive.content, WorkflowView.PROPOSED_NODE_NOTE)
        val strip = WorkflowView.nodeStrip(
            listOf(StripNodeInput("p", "EXP-9", "proposed", 0, 0, 0, live = false, needsYou = false)),
            emptyList(),
        )
        assertEquals(WorkflowView.PROPOSED_NODE_NOTE, strip.single().nodes.single().caption)
    }
}
