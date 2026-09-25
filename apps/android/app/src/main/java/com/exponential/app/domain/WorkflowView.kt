package com.exponential.app.domain

/**
 * EXP-981: what every client SAYS about a workflow. The graph's geometry is
 * the server's (`wave`/`lane` on the synced nodes); this is the rest — bands,
 * captions, the edges between nodes — mirrored ×4 (web `lib/workflow-view.ts`,
 * iOS `WorkflowView.swift`, desktop `domain::workflow_view`) and locked by the
 * contract fixture `domain-contract/fixtures/workflow-view.json`. All strings
 * byte-identical.
 *
 * NOT to be confused with [WorkflowCaption], which reads the agent feed's
 * Claude Code Workflow TOOL card — an unrelated concept that owns the
 * contract's `workflowStatus` key. A workflow of THIS kind wears `wf*`.
 */
object WorkflowView {

    /** The list's three bands, in order. Flat rows under each, no row buttons. */
    enum class Band(val key: String, val title: String) {
        Running("running", "Running"),
        Draft("draft", "Draft"),
        Done("done", "Done"),
    }

    val BANDS: List<Band> = listOf(Band.Running, Band.Draft, Band.Done)

    const val WORKFLOWS_TITLE = "Workflows"
    const val WORKFLOWS_EMPTY_TITLE = "No workflows yet"
    const val WORKFLOWS_EMPTY_BODY =
        "Select backlog issues on a board and choose Create workflow to plan them as one parallel run."
    const val PLAN_WORKFLOW_LABEL = "Plan"
    const val DELETE_WORKFLOW_LABEL = "Delete workflow"

    /** The bulk bar's play menu (EXP-981), in order. */
    const val START_AS_BATCH_LABEL = "Start as batch"
    const val START_AS_STACK_LABEL = "Start as stack"
    const val CREATE_WORKFLOW_LABEL = "Create workflow…"

    /**
     * `paused` is a running workflow someone held; `cancelled` is over. An
     * unknown status (a newer server) lands in Done rather than vanishing.
     */
    fun band(status: String): Band = when (status) {
        DomainContract.wfStatusRunning, DomainContract.wfStatusPaused -> Band.Running
        DomainContract.wfStatusDraft -> Band.Draft
        else -> Band.Done
    }

    /**
     * `workflows.metrics`: the plan's shape. [edges] and [cycleEdges] ride
     * along because the same jsonb carries them; only [nodes], [depth],
     * [width] and [cycles] are read by the rules below.
     */
    data class Shape(
        val nodes: Int = 0,
        val edges: Int = 0,
        val depth: Int = 0,
        val width: Int = 0,
        /** One entry per blocking cycle: its issue identifiers. Empty = startable. */
        val cycles: List<List<String>> = emptyList(),
        /** `<fromNodeId>\n<toNodeId>` of every edge inside a cycle. */
        val cycleEdges: List<String> = emptyList(),
    )

    /** `12 nodes · depth 3 · width 8`; `1 node · depth 1 · width 1`. */
    fun shapeLine(metrics: Shape): String {
        val nodes = if (metrics.nodes == 1) "1 node" else "${metrics.nodes} nodes"
        return "$nodes · depth ${metrics.depth} · width ${metrics.width}"
    }

    /** Null while the workflow could start; else the cycles spelled out. */
    fun cycleNote(metrics: Shape): String? {
        if (metrics.cycles.isEmpty()) return null
        val spelled = metrics.cycles.joinToString("; ") { it.joinToString(", ") }
        return "These issues block each other in a cycle: $spelled. Remove one relation to start."
    }

    fun riskLabel(risk: String): String = risk.replaceFirstChar { it.uppercaseChar() }

    /**
     * `EXP-14 +3` for a compound node (a parent run as one batch with its
     * sub-issues), the bare identifier otherwise.
     */
    fun nodeTitle(identifier: String, memberCount: Int): String =
        if (memberCount > 0) "$identifier +$memberCount" else identifier

    data class EdgeNode(
        val id: String,
        val issueId: String,
        val memberIssueIds: List<String>,
        /** EXP-983: engine-written serialization edges (`after_node_ids`). */
        val afterNodeIds: List<String> = emptyList(),
    )

    data class EdgeRelation(
        val type: String,
        val issueId: String,
        val relatedIssueId: String,
    )

    data class Edge(
        val from: String,
        val to: String,
        /** Inside a blocking cycle (`metrics.cycleEdges`): drawn red. */
        val cycle: Boolean,
        /**
         * EXP-983: not a `blocks` relation but a SERIALIZATION edge the engine
         * added after two siblings' work collided: `to` merges `from` in first.
         */
        val serial: Boolean = false,
    )

    /**
     * The edges between a workflow's nodes, from the synced `blocks` relations:
     * a relation between ANY two covered issues of two different nodes (the
     * server's `nodeEdges`). One edge per node pair, ordered by (from, to) node
     * id. [cycleEdges] = the workflow's `metrics.cycleEdges` (`<from>\n<to>`).
     * The engine's serialization edges (EXP-983) join them unless the pair
     * already has one.
     */
    fun edges(
        nodes: List<EdgeNode>,
        relations: List<EdgeRelation>,
        cycleEdges: List<String> = emptyList(),
    ): List<Edge> {
        val nodeOf = HashMap<String, String>()
        for (node in nodes) {
            nodeOf[node.issueId] = node.id
            for (member in node.memberIssueIds) nodeOf[member] = node.id
        }
        val onCycle = cycleEdges.toSet()
        val seen = HashSet<String>()
        val edges = ArrayList<Edge>()
        for (relation in relations) {
            if (relation.type != DomainContract.issueRelationTypeBlocks) continue
            val from = nodeOf[relation.issueId] ?: continue
            val to = nodeOf[relation.relatedIssueId] ?: continue
            if (from == to) continue
            val key = "$from\n$to"
            if (!seen.add(key)) continue
            edges.add(Edge(from = from, to = to, cycle = key in onCycle))
        }
        // Serialization edges, unless a real edge already joins the pair.
        val known = nodes.mapTo(HashSet()) { it.id }
        for (node in nodes) {
            for (from in node.afterNodeIds) {
                if (from !in known || from == node.id) continue
                if (!seen.add("$from\n${node.id}")) continue
                edges.add(Edge(from = from, to = node.id, cycle = false, serial = true))
            }
        }
        return edges.sortedWith(compareBy({ it.from }, { it.to }))
    }

    // ── Running a workflow (EXP-982) ────────────────────────────────────────

    const val START_WORKFLOW_LABEL = "Start"
    const val PAUSE_WORKFLOW_LABEL = "Pause"
    const val RESUME_WORKFLOW_LABEL = "Resume"
    const val CANCEL_WORKFLOW_CONFIRM =
        "Its live runs end and its branch is deleted. Nothing reached the default branch."
    const val FINAL_PR_TITLE = "Final pull request"

    /**
     * EXP-1033: the ONE human review of the whole run — squash-merging the
     * workflow's final pull request from the workflow screen.
     */
    const val MERGE_FINAL_PR_LABEL = "Merge"
    const val MERGE_FINAL_PR_CONFIRM =
        "The workflow's branch is squash-merged into the default branch and the run is done."

    /** The three fields Start is judged on, off the synced workflow row. */
    data class Startable(
        val status: String,
        val deviceId: String?,
        val repositoryId: String?,
    )

    /**
     * Why Start is disabled, or null when the draft can start. One reason, the
     * most fundamental first; the server refuses with the same sentences.
     */
    fun startBlocker(workflow: Startable, metrics: Shape): String? {
        if (workflow.status != DomainContract.wfStatusDraft) return "The workflow has already started."
        if (metrics.nodes == 0) return "The workflow has no issues."
        cycleNote(metrics)?.let { return it }
        if (workflow.repositoryId.isNullOrEmpty()) return "The workflow's repository is gone."
        if (workflow.deviceId.isNullOrEmpty()) return "Pick the device that runs this workflow first."
        return null
    }

    /**
     * The final-PR node's caption, or null while the node is not drawn: it
     * appears once every node landed (or was skipped), after the last wave.
     */
    fun finalPrCaption(
        nodeStates: List<String>,
        finalPrState: String?,
        finalPrNumber: Int?,
    ): String? {
        // EXP-984: a `proposed` node was never admitted; it is not part of the
        // run, so nothing waits for it.
        val real = nodeStates.filter { it != DomainContract.wfNodeStateProposed }
        if (real.isEmpty()) return null
        val allIn = real.all {
            it == DomainContract.wfNodeStateLanded || it == DomainContract.wfNodeStateSkipped
        }
        if (!allIn && finalPrNumber == null) return null
        if (finalPrNumber == null) return "Opening the pull request"
        val label = when (finalPrState) {
            DomainContract.prStateMerged -> "Merged"
            DomainContract.prStateClosed -> "Closed"
            else -> "Open"
        }
        return "#$finalPrNumber · $label"
    }

    const val RETRY_NODE_LABEL = "Retry"
    const val SKIP_NODE_LABEL = "Skip"
    const val SKIP_NODE_CONFIRM =
        "Its dependents go on without it. The node's work is not part of the final pull request."

    /**
     * A list row's secondary text: the shape line, led by the status word for
     * the two statuses a band alone does not tell apart.
     */
    fun rowSubtitle(status: String, metrics: Shape): String {
        val shape = shapeLine(metrics)
        return when (status) {
            DomainContract.wfStatusPaused -> "Paused · $shape"
            DomainContract.wfStatusCancelled -> "Cancelled · $shape"
            else -> shape
        }
    }

    // ── Speculative starts (EXP-983) ────────────────────────────────────────

    /** How an edge is drawn. Grey solid is the default; the others say something. */
    enum class EdgeStyle(val key: String) {
        Plain("plain"),
        Cycle("cycle"),
        Stale("stale"),
        Landed("landed"),
        Speculative("speculative"),
    }

    private val STARTED_STATES = setOf(
        DomainContract.wfNodeStateRunning,
        DomainContract.wfNodeStateWaiting,
        DomainContract.wfNodeStateInReview,
        DomainContract.wfNodeStateUpdating,
    )

    /**
     * - [EdgeStyle.Cycle] (red): inside a blocking cycle.
     * - [EdgeStyle.Stale] (red): upstream moved and the dependent is merging it
     *   in (`to` is `updating`).
     * - [EdgeStyle.Landed] (green): the blocker landed.
     * - [EdgeStyle.Speculative] (dashed): the dependent started before its
     *   blocker landed, or the edge is a serialization edge.
     * - [EdgeStyle.Plain] (grey): nothing to say yet.
     */
    fun edgeStyle(edge: Edge, fromState: String, toState: String): EdgeStyle = when {
        edge.cycle -> EdgeStyle.Cycle
        fromState == DomainContract.wfNodeStateLanded -> EdgeStyle.Landed
        toState == DomainContract.wfNodeStateUpdating -> EdgeStyle.Stale
        edge.serial || toState in STARTED_STATES -> EdgeStyle.Speculative
        else -> EdgeStyle.Plain
    }

    // ── Review gate, dynamic graphs (EXP-984) ─────────────

    const val ADMIT_NODE_LABEL = "Admit"
    const val DISMISS_NODE_LABEL = "Dismiss"
    const val PROPOSED_NODE_NOTE =
        "Filed during the run. Admit it into the workflow or dismiss it."

    /**
     * EXP-1014: the chip of a node whose issue row has not synced yet — the
     * identifier slot shows the first 8 characters of the issue id, the title
     * this line. Byte-identical ×4.
     */
    const val NODE_UNSYNCED_TITLE = "Not synced yet"

    // ── EXP-1082: the workflow contract's display layer ────────────────────
    // The fixture sections `displayStates` / `needsYouLabel` / `nodeStrips` /
    // `headerCaptions` / `primaryActions` / `chipMenus`. Byte-identical ×4
    // with web `lib/workflow-view.ts`.

    /** The chip suffix a node whose run waits on a person wears. */
    const val NEEDS_YOU_LABEL = "needs you"

    /** The FIVE states a person sees, folded from the internal `wfNodeState`.
     *  An unknown state reads as queued, never nothing. */
    fun nodeDisplayState(state: String): WorkflowNodeDisplayState = when (state) {
        "proposed", "blocked", "ready" -> WorkflowNodeDisplayState.QUEUED
        "running", "waiting", "in_review", "updating" -> WorkflowNodeDisplayState.RUNNING
        "landed" -> WorkflowNodeDisplayState.DONE
        "failed" -> WorkflowNodeDisplayState.FAILED
        "skipped" -> WorkflowNodeDisplayState.SKIPPED
        else -> WorkflowNodeDisplayState.QUEUED
    }

    /**
     * The strip IS the graph: waves left to right (only the waves that hold a
     * node), lanes top to bottom within a wave, ties by id. The strip takes
     * nodes only: the edges are the mini-graph popover's business. A compound node is
     * `stacked`; the caption is the node's note while it has one, else its
     * display label.
     */
    fun nodeStrip(nodes: List<StripNodeInput>): List<StripWave> =
        nodes.groupBy { it.wave }
            .toSortedMap()
            .map { (wave, members) ->
                StripWave(
                    wave = wave,
                    nodes = members
                        .sortedWith(compareBy<StripNodeInput>({ it.lane }, { it.id }))
                        .map { node ->
                            val display = nodeDisplayState(node.state)
                            NodeChip(
                                id = node.id,
                                title = nodeTitle(node.identifier, node.members),
                                display = display,
                                caption = node.note?.trim()?.takeIf { it.isNotEmpty() }
                                    ?: if (node.state == DomainContract.wfNodeStateProposed) {
                                        PROPOSED_NODE_NOTE
                                    } else {
                                        display.label
                                    },
                                stacked = node.members > 0,
                                members = node.members,
                                live = node.live,
                                needsYou = node.needsYou,
                            )
                        },
                )
            }

    private val STATUS_WORDS = mapOf(
        "draft" to "Draft",
        "done" to "Done",
        "failed" to "Failed",
        "cancelled" to "Cancelled",
    )

    /**
     * The one line under the workflow's name. A draft counts its ISSUES
     * (members included): `Draft · 8 issues`. A started workflow names its
     * runner and counts NODES: `on MacBook · 5 of 8 done · 2 running` (the
     * running tail only while something runs). Over, it counts what happened:
     * `Done · 2 done · 1 skipped`. A `proposed` node is never counted.
     */
    fun headerCaption(status: String, nodes: List<HeaderNode>, deviceLabel: String?): String {
        val admitted = nodes.filter { it.state != DomainContract.wfNodeStateProposed }
        if (status == DomainContract.wfStatusDraft) {
            val issues = admitted.sumOf { 1 + it.members }
            return "Draft · ${if (issues == 1) "1 issue" else "$issues issues"}"
        }
        fun tally(display: WorkflowNodeDisplayState) =
            admitted.count { nodeDisplayState(it.state) == display }
        val done = tally(WorkflowNodeDisplayState.DONE)
        if (status == DomainContract.wfStatusRunning || status == DomainContract.wfStatusPaused) {
            val parts = ArrayList<String>()
            if (deviceLabel != null) parts.add("on $deviceLabel")
            parts.add("$done of ${admitted.size} done")
            val running = tally(WorkflowNodeDisplayState.RUNNING)
            if (running > 0) parts.add("$running running")
            return parts.joinToString(" · ")
        }
        val word = STATUS_WORDS[status] ?: status.replaceFirstChar { it.uppercaseChar() }
        val parts = arrayListOf(word, "$done done")
        val failed = tally(WorkflowNodeDisplayState.FAILED)
        if (failed > 0) parts.add("$failed failed")
        val skipped = tally(WorkflowNodeDisplayState.SKIPPED)
        if (skipped > 0) parts.add("$skipped skipped")
        return parts.joinToString(" · ")
    }

    /**
     * The header's ONE primary button: a draft without a runner picks one, a
     * draft starts; a started workflow whose final PR is OPEN reviews it (the
     * one human review), else running pauses and paused resumes; done reviews
     * the merged final PR. Failed and cancelled offer nothing; Stop and Delete
     * live in the overflow.
     */
    fun primaryAction(
        status: String,
        deviceLabel: String?,
        finalPrState: String? = null,
    ): WorkflowPrimaryAction? = when (status) {
        DomainContract.wfStatusDraft ->
            if (deviceLabel != null) WorkflowPrimaryAction.START else WorkflowPrimaryAction.PICK_DEVICE
        DomainContract.wfStatusRunning, DomainContract.wfStatusPaused -> when {
            finalPrState == DomainContract.prStateOpen -> WorkflowPrimaryAction.REVIEW_FINAL_PR
            status == DomainContract.wfStatusRunning -> WorkflowPrimaryAction.PAUSE
            else -> WorkflowPrimaryAction.RESUME
        }
        DomainContract.wfStatusDone -> WorkflowPrimaryAction.REVIEW_FINAL_PR
        else -> null
    }

    /**
     * The header's status glyph, as the node display state it reads like:
     * draft → queued, running/paused → running, done → done, failed →
     * failed, cancelled → skipped, anything newer → queued. Locked ×4.
     */
    fun statusGlyph(status: String): WorkflowNodeDisplayState = when (status) {
        DomainContract.wfStatusRunning, DomainContract.wfStatusPaused -> WorkflowNodeDisplayState.RUNNING
        DomainContract.wfStatusDone -> WorkflowNodeDisplayState.DONE
        "failed" -> WorkflowNodeDisplayState.FAILED
        DomainContract.wfStatusCancelled -> WorkflowNodeDisplayState.SKIPPED
        else -> WorkflowNodeDisplayState.QUEUED
    }

    // ── EXP-1087: the page's own words + the header overflow ─────────────
    // Fixture sections `pageLabels` / `overflowMenus`; byte-identical ×4.

    const val ALL_NODES_LABEL = "All"
    const val DECISIONS_LABEL = "Decisions"
    const val STOP_WORKFLOW_LABEL = "Stop"
    const val PICK_DEVICE_LABEL = "Pick device"
    const val RUNS_ON_LABEL = "Runs on"
    const val REVIEW_FINAL_PR_LABEL = "Review final PR"

    /** A node row on All × Changes with nothing to open. */
    const val NO_CHANGES_LABEL = "No changes yet"

    /** The Run(s) face when no run is in scope. */
    const val NO_RUNS_LABEL = "No runs yet"

    /** The Results face when no run published a screenshot. */
    const val NO_RESULTS_LABEL = "No results yet"

    /** Dismiss's confirm on a `proposed` node. */
    const val DISMISS_NODE_CONFIRM = "The node is removed from the workflow."

    /**
     * The header overflow: a draft is planned, re-bound or deleted; a live
     * (running or paused) one is stopped; anything else is deleted.
     */
    fun overflowMenu(status: String): List<WorkflowOverflowItem> = when (status) {
        DomainContract.wfStatusDraft ->
            listOf(WorkflowOverflowItem.PLAN, WorkflowOverflowItem.RUNS_ON, WorkflowOverflowItem.DELETE)
        DomainContract.wfStatusRunning, DomainContract.wfStatusPaused -> listOf(WorkflowOverflowItem.STOP)
        else -> listOf(WorkflowOverflowItem.DELETE)
    }

    /**
     * What a node chip's overflow offers: Retry / Skip on a `failed` node,
     * Admit / Dismiss on a `proposed` one, nothing else anywhere.
     */
    fun nodeChipMenu(state: String): List<NodeChipAction> = when (state) {
        DomainContract.wfNodeStateFailed -> listOf(NodeChipAction.RETRY, NodeChipAction.SKIP)
        DomainContract.wfNodeStateProposed -> listOf(NodeChipAction.ADMIT, NodeChipAction.DISMISS)
        else -> emptyList()
    }
}

/** EXP-1082: contract `wfNodeDisplayState` — `wire` = the value, `label` the caption. */
enum class WorkflowNodeDisplayState(val wire: String, val label: String) {
    QUEUED("queued", "Queued"),
    RUNNING("running", "Running"),
    DONE("done", "Done"),
    FAILED("failed", "Failed"),
    SKIPPED("skipped", "Skipped"),
}

/** One node as the strip reads it (fixture `nodeStrips[].nodes[]`). */
data class StripNodeInput(
    val id: String,
    val identifier: String,
    val state: String,
    val wave: Int,
    val lane: Int,
    val members: Int,
    val live: Boolean,
    val needsYou: Boolean,
    val note: String? = null,
)

/** One wave column of the strip. */
data class StripWave(val wave: Int, val nodes: List<NodeChip>)

/** One drawn node chip (fixture `nodeStrips[].strip[].nodes[]`). */
data class NodeChip(
    val id: String,
    val title: String,
    val display: WorkflowNodeDisplayState,
    val caption: String,
    val stacked: Boolean,
    val members: Int,
    val live: Boolean,
    val needsYou: Boolean,
)

/** One node as the header caption counts it. */
data class HeaderNode(val state: String, val members: Int)

enum class WorkflowPrimaryAction(val wire: String) {
    PICK_DEVICE("pick_device"),
    START("start"),
    PAUSE("pause"),
    RESUME("resume"),
    REVIEW_FINAL_PR("review_final_pr"),
}

enum class NodeChipAction(val wire: String) {
    RETRY("retry"),
    SKIP("skip"),
    ADMIT("admit"),
    DISMISS("dismiss"),
}

/** EXP-1087: one entry of the workflow header's overflow (fixture `overflowMenus`). */
enum class WorkflowOverflowItem(val wire: String) {
    PLAN("plan"),
    RUNS_ON("runs_on"),
    STOP("stop"),
    DELETE("delete"),
}
